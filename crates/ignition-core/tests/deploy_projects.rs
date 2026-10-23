//! Engine suite for `deploy projects` (Phase 25): the check's decisions,
//! driven through `deploy_projects_prepare` over `FakeProjects`, an
//! in-memory project store that copies the C8 captures. No network.
//!
//! Every later slice (25-06 to 25-15) adds its engine cases here and its
//! behaviour to the fake.
//!
//! Checkout descriptors carry `"attributes":{}` or a non-empty attributes
//! object, as every descriptor in the C8 captures did. The fake stamps
//! `lastModification` and `lastModificationSignature` into each export (C8b),
//! so a green comparison proves the digest ignores them.

mod common;

use std::path::{Path, PathBuf};

use common::fake_projects::{FailingRecordWriter, FakeProjects, ForbidProjectWrites};
use ignition_core::actions::deploy_projects::{
    ChildStatus, DeployProjectsResult, LoadedProject, NoProjectsObserver, PROJECTS_CANARY_EXEMPT,
    ProjectGateway, ProjectOutcome, ProjectsEvent, ProjectsInput, ProjectsObserver,
    ProjectsPrepared, ProjectsRunOptions, ProjectsSurfaces, deploy_projects_prepare,
    deploy_projects_run, load_project,
};
use ignition_core::client::projects::{ImportOutcome, ProjectRecord};
use ignition_core::deploy::apply::preflight::DesignerSeen;
use ignition_core::deploy::apply::preflight::{DesignersMode, DesignersPolicy};
use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::{GitEntry, ProjectHost, ReleaseRecordWriter};
use ignition_core::error::CoreError;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PROJECT: &str = "CapApp";

/// The sweep's wait bound and poll interval in the engine suite: short, set
/// through the options and never through the environment.
const CHILDREN_SETTLE: Duration = Duration::from_secs(1);
const CHILDREN_POLL: Duration = Duration::from_millis(5);

const PROJECT_JSON: &str = r#"{"title":"Cap App","enabled":true}"#;
const SCRIPT_RESOURCE: &str =
    r#"{"scope":"A","version":1,"files":["code.py"],"attributes":{"enabled":true}}"#;
const VIEW_RESOURCE: &str = r#"{"scope":"G","version":1,"files":["view.json"],"attributes":{}}"#;
const BARE_RESOURCE: &str = r#"{"scope":"A","version":1,"files":[],"attributes":{}}"#;
const CODE_V1: &[u8] = b"def f():\n    return 1\n";
const CODE_V2: &[u8] = b"def f():\n    return 2\n";
const VIEW: &[u8] = br#"{"root":{"type":"ia.container.flex"}}"#;

/// The project as the repo holds it and as the gateway holds it when they
/// agree: six file members.
fn base_members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("project.json", PROJECT_JSON.as_bytes().to_vec()),
        (
            "ignition/script-python/cap/resource.json",
            SCRIPT_RESOURCE.as_bytes().to_vec(),
        ),
        ("ignition/script-python/cap/code.py", CODE_V1.to_vec()),
        (
            "com.inductiveautomation.perspective/views/Cap/resource.json",
            VIEW_RESOURCE.as_bytes().to_vec(),
        ),
        (
            "com.inductiveautomation.perspective/views/Cap/view.json",
            VIEW.to_vec(),
        ),
    ]
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

/// A project repo plus a targets root holding `targets/dev.yaml`.
struct Checkout {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
    /// The master node's data directory; `projects/` exists and is empty.
    data: PathBuf,
}

impl Checkout {
    /// The host the check probes: the checkout's data directory.
    fn host(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.data.clone(), "nodes.master")
    }

    /// The name the host's `system-properties` carries.
    fn set_host_system_name(&self, name: &str) {
        put(
            &self.data,
            "config/resources/core/ignition/system-properties/config.json",
            format!(r#"{{"systemName":"{name}"}}"#).as_bytes(),
        );
    }

    /// The target file declares `nodes.master.gateway_name: name`.
    fn declare_gateway_name(&self, name: &str) {
        put(
            &self.targets,
            "targets/dev.yaml",
            format!(
                "schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    gateway_name: {name}\n    local:\n      data_dir: '{}'\n",
                self.data.display()
            )
            .as_bytes(),
        );
    }

    fn input(&self) -> ProjectsInput {
        ProjectsInput {
            repo: self.repo.clone(),
            targets: self.targets.clone(),
            env: "dev".to_string(),
            release: "v1.0.0".to_string(),
            allow_downgrade: false,
            allow_first_change: None,
            artifact_sha256: None,
        }
    }
}

fn checkout(members: &[(&str, Vec<u8>)]) -> Checkout {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    std::fs::create_dir_all(data.join("projects")).expect("data dir");
    for (rel, bytes) in members {
        put(&repo, rel, bytes);
    }
    put(
        &repo,
        "deploy/project.yaml",
        format!("schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\n").as_bytes(),
    );
    // Repo content outside the allowlist: never part of the artifact.
    put(&repo, "docs/notes.md", b"not shipped");
    // The host's own name, which the wrong-target pre-check compares with
    // what the gateway reports over REST (25-14).
    put(
        &data,
        "config/resources/core/ignition/system-properties/config.json",
        br#"{"systemName":"capgw"}"#,
    );
    put(
        &targets,
        "targets/dev.yaml",
        format!(
            "schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: '{}'\n",
            data.display()
        )
        .as_bytes(),
    );
    Checkout {
        _scratch: scratch,
        repo,
        targets,
        data,
    }
}

async fn prepare(checkout: &Checkout, gateway: &FakeProjects) -> DeployProjectsResult {
    let input = checkout.input();
    let loaded = load_project(&input).expect("the checkout loads");
    let host = checkout.host();
    deploy_projects_prepare(
        &loaded,
        &input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
    )
    .await
    .expect("the check completes")
    .into_check_result()
}

fn replace(
    members: &[(&'static str, Vec<u8>)],
    key: &str,
    bytes: &[u8],
) -> Vec<(&'static str, Vec<u8>)> {
    members
        .iter()
        .map(|(k, v)| {
            if *k == key {
                (*k, bytes.to_vec())
            } else {
                (*k, v.clone())
            }
        })
        .collect()
}

/// The canonical digest of the artifact the checkout builds.
fn artifact_sha(checkout: &Checkout) -> String {
    load_project(&checkout.input())
        .expect("the checkout loads")
        .artifact
        .digest
        .sha256
}

/// Write the release record `ign` left for `CapApp` on the master node.
fn put_record(checkout: &Checkout, release: &str, artifact_sha256: &str) {
    let record = ReleaseRecord {
        schema_version: 1,
        gateway: "capgw".to_string(),
        project: PROJECT.to_string(),
        release: release.to_string(),
        artifact_sha256: artifact_sha256.to_string(),
        run_id: "20261020T101500Z-ab12".to_string(),
        deployed_at: "2026-10-20T10:15:00Z".to_string(),
    };
    put(
        &checkout.data,
        "ign-deploy/capgw/projects/CapApp/release.json",
        &record.to_bytes(),
    );
}

/// A record naming `release` and the checkout's own artifact: what the last
/// `ign` deploy left when the gateway still holds that artifact.
fn put_matching_record(checkout: &Checkout, release: &str) {
    put_record(checkout, release, &artifact_sha(checkout));
}

async fn prepare_with(
    checkout: &Checkout,
    gateway: &FakeProjects,
    input: &ProjectsInput,
) -> Result<DeployProjectsResult, CoreError> {
    let loaded = load_project(input).expect("the checkout loads");
    let host = checkout.host();
    deploy_projects_prepare(
        &loaded,
        input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
    )
    .await
    .map(|prepared| prepared.into_check_result())
}

/// C8b: an export of an unchanged project is a fixed point once the volatile
/// stamps are ignored, and C8c: the gateway adds `description` and `parent`
/// to `project.json`. Fresh stamps on every export must still read `skip`.
#[tokio::test]
async fn check_skip_when_equal() {
    let members = base_members();
    let repo = checkout(&members);
    // The same release is already recorded (K14): nothing to write.
    put_matching_record(&repo, "v1.0.0");
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

    let first = prepare(&repo, &gateway).await;
    assert_eq!(first.action, ProjectAction::Skip);
    assert!(!first.would_change);
    assert_eq!(first.live_sha256.as_deref(), Some(&*first.artifact_sha256));
    assert_eq!(first.members.same, 5);
    assert_eq!(
        (
            first.members.added,
            first.members.changed,
            first.members.removed
        ),
        (0, 0, 0)
    );
    assert!(first.removed.is_empty());

    // A second check reads a freshly stamped export and says the same.
    let second = prepare(&repo, &gateway).await;
    assert_eq!(second.action, ProjectAction::Skip);
    assert_eq!(second.live_sha256, first.live_sha256);
    assert_eq!(gateway.export_count(PROJECT), 2, "each check exported once");
}

/// The stamps really are different on every export, so the skip above is the
/// digest ignoring them and not the fake repeating itself.
#[tokio::test]
async fn fake_export_stamps_a_fresh_modification_on_every_export() {
    let members = base_members();
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);
    let one = gateway.export_member(PROJECT, "ignition/script-python/cap/resource.json");
    let two = gateway.export_member(PROJECT, "ignition/script-python/cap/resource.json");
    assert_ne!(one, two, "fresh stamps each time");
    let value: serde_json::Value = serde_json::from_slice(&one).expect("json");
    assert!(value["attributes"]["lastModification"].is_object());
    assert!(value["attributes"]["lastModificationSignature"].is_string());
    assert_eq!(
        value["attributes"]["enabled"],
        serde_json::json!(true),
        "other attributes survive"
    );
}

#[tokio::test]
async fn check_import_counts() {
    // Live: the base project with `code.py` edited, no `fresh` resource, and
    // one resource folder (`old`) the repo does not have.
    let mut live = replace(
        &base_members(),
        "ignition/script-python/cap/code.py",
        CODE_V2,
    );
    live.push((
        "ignition/script-python/old/resource.json",
        BARE_RESOURCE.as_bytes().to_vec(),
    ));
    // Repo: the base project plus one new resource folder holding only its
    // descriptor.
    let mut repo_members = base_members();
    repo_members.push((
        "ignition/script-python/fresh/resource.json",
        BARE_RESOURCE.as_bytes().to_vec(),
    ));
    let repo = checkout(&repo_members);
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &live);

    // With no record this would be a first deploy and need the reason
    // (D-12); the member counts are the subject here, so one is recorded.
    put_matching_record(&repo, "v0.9.0");
    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Import);
    assert!(result.would_change);
    assert_ne!(
        result.live_sha256.as_deref(),
        Some(&*result.artifact_sha256)
    );
    assert_eq!(result.members.added, 1);
    assert_eq!(result.members.changed, 1);
    assert_eq!(result.members.removed, 1);
    assert_eq!(result.members.same, 4);
    assert_eq!(
        result.removed,
        vec!["ignition/script-python/old/resource.json".to_string()]
    );
    assert_eq!(result.removed_count, 1);
}

/// D-13: a project the gateway does not have is `create`, not a refusal and
/// not an error; there is no live digest and no export to read.
#[tokio::test]
async fn check_create_when_absent() {
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_names(3);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Create);
    assert!(result.would_change);
    assert_eq!(result.live_sha256, None);
    assert_eq!(result.members.added, 5);
    assert_eq!(
        (
            result.members.changed,
            result.members.removed,
            result.members.same
        ),
        (0, 0, 0)
    );
    assert!(result.removed.is_empty());
    assert_eq!(result.unmanaged_count, 3);
    assert_eq!(
        gateway.export_count(PROJECT),
        0,
        "an absent project is never exported"
    );
}

/// DPRJ-04: the other projects are named and nothing more, byte-sorted, 200 at
/// most with the full count beside them.
#[tokio::test]
async fn check_unmanaged_names_only() {
    let repo = checkout(&base_members());
    let members = base_members();
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members)
        .with_names(205);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.unmanaged_count, 205);
    assert_eq!(result.unmanaged.len(), 200);
    let expected: Vec<String> = (0..200).map(|i| format!("Other-{i:03}")).collect();
    assert_eq!(result.unmanaged, expected);
    assert!(
        !result.unmanaged.iter().any(|name| name == PROJECT),
        "the named project is never unmanaged"
    );
    // Names only: nothing of the other projects is read.
    assert_eq!(gateway.export_calls(), vec![PROJECT.to_string()]);
    assert_eq!(gateway.find_calls(), vec![PROJECT.to_string()]);
}

/// Byte order, not the gateway's listing order or a locale order: uppercase
/// sorts before lowercase.
#[tokio::test]
async fn check_unmanaged_is_byte_sorted() {
    let repo = checkout(&base_members());
    let members = base_members();
    let gateway = FakeProjects::new()
        .with_project("beta", None, false, &[("project.json", b"{}".to_vec())])
        .with_project(PROJECT, None, false, &members)
        .with_project("Zeta", None, false, &[("project.json", b"{}".to_vec())])
        .with_project("alpha", None, false, &[("project.json", b"{}".to_vec())]);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.unmanaged, vec!["Zeta", "alpha", "beta"]);
    assert_eq!(result.unmanaged_count, 3);
}

/// D-07, D-14: the target is read below the targets root. A layout that
/// keeps `targets/dev.yaml` outside the repo works when the root is named and
/// refuses, naming the file, when the repo is the base.
#[tokio::test]
async fn check_targets_root() {
    let members = base_members();
    let repo = checkout(&members);
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);
    assert!(
        !repo.repo.join("targets").exists(),
        "the repo holds no targets"
    );

    let found = prepare(&repo, &gateway).await;
    assert_eq!(found.gateway, "capgw");
    assert_eq!(found.env, "dev");

    let mut default_base = repo.input();
    default_base.targets = default_base.repo.clone();
    let refused = load_project(&default_base).expect_err("no targets/dev.yaml in the repo");
    match refused {
        CoreError::DeployTreeInvalid(details) => {
            assert!(
                details
                    .findings
                    .iter()
                    .any(|finding| finding.file == "targets/dev.yaml"),
                "the finding names targets/dev.yaml: {details}"
            );
        }
        other => panic!("expected deploy_tree_invalid, got {other:?}"),
    }
}

/// DPRJ-05: the check leaves nothing behind, in the repo, the targets root or
/// the host: not even for `record`, the action whose apply writes the release
/// record (K14).
#[tokio::test]
async fn check_never_creates_ign_dir() {
    let members = base_members();
    let edited = replace(&members, "ignition/script-python/cap/code.py", CODE_V2);
    for (label, gateway, recorded) in [
        ("create", FakeProjects::new(), false),
        (
            "skip",
            FakeProjects::new().with_project(PROJECT, None, false, &members),
            true,
        ),
        (
            "record",
            FakeProjects::new().with_project(PROJECT, None, false, &members),
            false,
        ),
        (
            "import",
            FakeProjects::new().with_project(PROJECT, None, false, &edited),
            true,
        ),
    ] {
        let repo = checkout(&members);
        if recorded {
            put_matching_record(&repo, "v1.0.0");
        }
        let before = std::fs::read_dir(&repo.repo).expect("repo").count();
        let host_before = std::fs::read_dir(&repo.data).expect("data").count();
        let mut input = repo.input();
        input.release = if label == "import" {
            "v1.1.0"
        } else {
            "v1.0.0"
        }
        .to_string();
        let result = prepare_with(&repo, &gateway, &input)
            .await
            .expect("the check completes");
        assert!(result.check, "{label}");
        assert_eq!(
            format!("{:?}", result.action).to_lowercase(),
            label,
            "{label}"
        );
        assert!(!repo.repo.join(".ign").exists(), "{label}: repo/.ign");
        assert!(!repo.targets.join(".ign").exists(), "{label}: targets/.ign");
        assert_eq!(
            std::fs::read_dir(&repo.repo).expect("repo").count(),
            before,
            "{label}: the repo gained nothing"
        );
        assert_eq!(
            std::fs::read_dir(&repo.data).expect("data").count(),
            host_before,
            "{label}: the host gained nothing"
        );
    }
}

/// The check asks the gateway three read questions and only about the named
/// project (the fake has no write method to call, so this is the whole
/// footprint).
#[tokio::test]
async fn check_reads_only_the_named_project() {
    let repo = checkout(&base_members());
    let members = base_members();
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members)
        .with_names(4);
    prepare(&repo, &gateway).await;
    assert_eq!(gateway.list_count(), 1);
    assert_eq!(gateway.find_calls(), vec![PROJECT.to_string()]);
    assert_eq!(gateway.export_calls(), vec![PROJECT.to_string()]);
}

/// Run the check expecting a refusal or failure.
async fn prepare_err(checkout: &Checkout, gateway: &FakeProjects) -> CoreError {
    let input = checkout.input();
    let loaded = load_project(&input).expect("the checkout loads");
    let host = checkout.host();
    deploy_projects_prepare(
        &loaded,
        &input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
    )
    .await
    .expect_err("the check refuses")
}

/// The gateway fake took no call of any kind.
fn assert_untouched(gateway: &FakeProjects) {
    assert_eq!(gateway.list_count(), 0, "no project list read");
    assert!(gateway.find_calls().is_empty(), "no project lookup");
    assert!(gateway.export_calls().is_empty(), "no export");
}

/// Creates one `.git` shape at the given path.
type GitShape = Box<dyn Fn(&Path)>;

/// DPRJ-04, D-11, K9: a `.git` of any kind under the project's directory
/// refuses `project_git_managed` with the matching kind, naming the project
/// and the location, before the gateway is asked anything.
#[tokio::test]
async fn git_present_refuses_before_gateway() {
    let members = base_members();
    let mut shapes: Vec<(&str, GitShape)> = vec![
        (
            "directory",
            Box::new(|git: &Path| std::fs::create_dir_all(git).expect("git dir")),
        ),
        (
            "file",
            Box::new(|git: &Path| {
                std::fs::write(git, b"gitdir: ../elsewhere\n").expect("git file");
            }),
        ),
    ];
    #[cfg(unix)]
    shapes.push((
        "symlink",
        Box::new(|git: &Path| {
            // A dangling target: the probe never follows the link.
            std::os::unix::fs::symlink("/nonexistent-target-for-git-probe", git).expect("symlink");
        }),
    ));
    for (kind, make) in shapes {
        let repo = checkout(&members);
        let project_dir = repo.data.join("projects").join(PROJECT);
        std::fs::create_dir_all(&project_dir).expect("project dir");
        make(&project_dir.join(".git"));
        let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

        let err = prepare_err(&repo, &gateway).await;
        match &err {
            CoreError::ProjectGitManaged {
                project,
                location,
                kind: found,
            } => {
                assert_eq!(project, PROJECT, "{kind}");
                assert_eq!(found, kind, "{kind}: the kind is named");
                assert!(
                    location.contains("projects/CapApp/.git"),
                    "{kind}: location {location}"
                );
            }
            other => panic!("{kind}: expected project_git_managed, got {other:?}"),
        }
        assert_eq!(err.code(), "project_git_managed", "{kind}");
        assert_untouched(&gateway);
    }
}

/// No `.git` (with and without the project's directory) proceeds as before.
#[tokio::test]
async fn git_absent_proceeds() {
    let members = base_members();

    // The project's directory is absent: the `create` case's host state.
    let repo = checkout(&members);
    let gateway = FakeProjects::new();
    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Create);
    assert_eq!(gateway.list_count(), 1);

    // The directory exists and holds no `.git`.
    let repo = checkout(&members);
    std::fs::create_dir_all(repo.data.join("projects").join(PROJECT).join("resources"))
        .expect("project dir");
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);
    let result = prepare(&repo, &gateway).await;
    // Equal to the gateway, no record yet: the record-only adoption (K14).
    assert_eq!(result.action, ProjectAction::Record);
    assert_eq!(gateway.export_calls(), vec![PROJECT.to_string()]);
}

/// Research Pitfall 3, T-25-20: a probe that cannot read is a transport
/// failure, never "no git", and the gateway is not asked anything.
#[cfg(unix)]
#[tokio::test]
async fn git_probe_unreadable_is_transport_failed() {
    use std::os::unix::fs::PermissionsExt;

    let members = base_members();
    let repo = checkout(&members);
    let project_dir = repo.data.join("projects").join(PROJECT);
    std::fs::create_dir_all(project_dir.join(".git")).expect("git dir");
    std::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    // A privileged user ignores the mode; nothing can be denied there.
    let privileged = std::fs::read_dir(&project_dir).is_ok();
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

    let err = prepare_err(&repo, &gateway).await;
    std::fs::set_permissions(&project_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore");
    if privileged {
        assert_eq!(err.code(), "project_git_managed");
    } else {
        assert_eq!(err.code(), "transport_failed", "{err}");
        assert!(err.to_string().contains("projects/CapApp/.git"), "{err}");
    }
    assert_untouched(&gateway);

    // A missing `projects` directory is the same failure, not "absent".
    let repo = checkout(&members);
    std::fs::remove_dir_all(repo.data.join("projects")).expect("remove projects");
    let gateway = FakeProjects::new();
    let err = prepare_err(&repo, &gateway).await;
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert_untouched(&gateway);
}

/// The live project with `code.py` edited: differs from the checkout.
fn differing_live() -> Vec<(&'static str, Vec<u8>)> {
    replace(
        &base_members(),
        "ignition/script-python/cap/code.py",
        CODE_V2,
    )
}

fn first_change_input(checkout: &Checkout, reason: Option<&str>) -> ProjectsInput {
    let mut input = checkout.input();
    input.allow_first_change = reason.map(str::to_string);
    input
}

/// D-12, V3: no record and a live project that differs from the artifact is
/// the first `ign` deploy changing what the gateway runs; it refuses without
/// the reason, and the reason is reported when given.
#[tokio::test]
async fn first_deploy_change_refuses() {
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &differing_live());

    let err = prepare_with(&repo, &gateway, &repo.input())
        .await
        .expect_err("the first change refuses");
    match &err {
        CoreError::DestructiveChangeRefused { gate, keys, count } => {
            assert_eq!(gate, "first_deploy");
            assert_eq!(keys, &vec![PROJECT.to_string()]);
            assert_eq!(*count, 1);
        }
        other => panic!("expected destructive_change_refused, got {other:?}"),
    }
    assert_eq!(err.code(), "destructive_change_refused");
    assert!(err.to_string().contains("--allow-first-change"), "{err}");

    let input = first_change_input(&repo, Some("adopt v1.3.0 changes"));
    let result = prepare_with(&repo, &gateway, &input)
        .await
        .expect("the reason lets it through");
    assert_eq!(result.action, ProjectAction::Import);
    assert!(result.would_change);
    assert_eq!(
        result.allow_first_change.as_deref(),
        Some("adopt v1.3.0 changes")
    );
    assert_eq!(result.previous_release, None);
    assert_eq!(result.recorded_sha256, None);
    assert_eq!(result.live_matches_last_deploy, None);
}

/// A record present: the live project differing from the artifact is an
/// ordinary import, not a first deploy.
#[tokio::test]
async fn recorded_project_imports_without_the_first_change_reason() {
    let repo = checkout(&base_members());
    put_matching_record(&repo, "v0.9.0");
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &differing_live());

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Import);
    assert_eq!(result.allow_first_change, None);
    assert_eq!(result.previous_release.as_deref(), Some("v0.9.0"));
}

/// K14, D-12: the git module left the project exactly as the repo has it and
/// there is no record: only the record is planned.
#[tokio::test]
async fn first_deploy_equal_records() {
    let members = base_members();
    let repo = checkout(&members);
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Record);
    assert!(result.would_change);
    assert_eq!(result.previous_release, None);
    assert_eq!(result.recorded_sha256, None);
    assert_eq!(result.live_matches_last_deploy, None);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

/// K14: the same release recorded and the content equal: nothing at all.
#[tokio::test]
async fn same_release_skips() {
    let members = base_members();
    let repo = checkout(&members);
    put_matching_record(&repo, "v1.0.0");
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Skip);
    assert!(!result.would_change);
    assert_eq!(result.previous_release.as_deref(), Some("v1.0.0"));
    assert_eq!(result.live_matches_last_deploy, Some(true));
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);

    // The same version written without its `v` is the same release.
    let repo = checkout(&members);
    put_matching_record(&repo, "1.0.0");
    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Skip);
}

/// K14: a new release with unchanged content records the release only.
#[tokio::test]
async fn new_release_same_content_records() {
    let members = base_members();
    let repo = checkout(&members);
    put_matching_record(&repo, "v1.0.0");
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

    let mut input = repo.input();
    input.release = "v1.1.0".to_string();
    let result = prepare_with(&repo, &gateway, &input)
        .await
        .expect("the check completes");
    assert_eq!(result.action, ProjectAction::Record);
    assert!(result.would_change);
    assert_eq!(result.previous_release.as_deref(), Some("v1.0.0"));
    assert_eq!(result.release, "v1.1.0");
}

/// D-13: a project the gateway does not have is created whatever the record
/// says; there is nothing to adopt, so no first-deploy reason is asked for.
#[tokio::test]
async fn create_needs_no_first_change_flag() {
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_names(2);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Create);
    assert_eq!(result.allow_first_change, None);
    assert_eq!(result.live_matches_last_deploy, None);
}

/// K13: the gateway no longer holds what `ign` last deployed. A warning, not
/// a refusal.
#[tokio::test]
async fn live_drift_warns() {
    let members = base_members();
    let repo = checkout(&members);
    let recorded = "b".repeat(64);
    put_record(&repo, "v1.0.0", &recorded);
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members);

    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.recorded_sha256.as_deref(), Some(&*recorded));
    assert_eq!(result.live_matches_last_deploy, Some(false));
    assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
    assert!(
        result.warnings[0].contains(PROJECT) && result.warnings[0].contains("v1.0.0"),
        "{:?}",
        result.warnings
    );
    // Not a refusal: the content equals the artifact and the release is the
    // same, so the action is still `skip`.
    assert_eq!(result.action, ProjectAction::Skip);

    // And an import over a drifted gateway still goes ahead.
    let repo = checkout(&members);
    put_record(&repo, "v0.9.0", &recorded);
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &differing_live());
    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Import);
    assert_eq!(result.live_matches_last_deploy, Some(false));
    assert_eq!(result.warnings.len(), 1);
}

/// The gateway holds what the record says it received: no warning.
#[tokio::test]
async fn live_matching_the_record_is_silent() {
    let members = base_members();
    let repo = checkout(&members);
    // The gateway holds the old content; the record is its digest.
    let old = FakeProjects::new().with_project(PROJECT, None, false, &differing_live());
    let old_sha = prepare_with(
        &repo,
        &old,
        &first_change_input(&repo, Some("learn the live digest")),
    )
    .await
    .expect("check")
    .live_sha256
    .expect("live digest");
    put_record(&repo, "v0.9.0", &old_sha);

    let result = prepare(&repo, &old).await;
    assert_eq!(result.action, ProjectAction::Import);
    assert_eq!(result.recorded_sha256.as_deref(), Some(&*old_sha));
    assert_eq!(result.live_matches_last_deploy, Some(true));
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

/// The checkout's members with `project.json` naming `parent`.
fn members_with_parent(parent: &str) -> Vec<(&'static str, Vec<u8>)> {
    replace(
        &base_members(),
        "project.json",
        format!(r#"{{"title":"Cap App","enabled":true,"parent":"{parent}"}}"#).as_bytes(),
    )
}

/// A gateway project that serves as somebody's parent.
fn with_parent_project(
    gateway: FakeProjects,
    name: &str,
    parent: Option<&str>,
    inheritable: bool,
) -> FakeProjects {
    gateway.with_project(
        name,
        parent,
        inheritable,
        &[("project.json", b"{}".to_vec())],
    )
}

/// What a refused parent check must have left behind: the list was read once
/// and no project, the parent included, was looked up or exported.
fn assert_parent_refused(err: &CoreError, reason: &str, gateway: &FakeProjects) {
    match err {
        CoreError::ProjectParentInvalid {
            project,
            parent: _,
            reason: got,
        } => {
            assert_eq!(project, PROJECT);
            assert_eq!(got, reason, "{err}");
        }
        other => panic!("expected project_parent_invalid, got {other}"),
    }
    assert_eq!(err.code(), "project_parent_invalid");
    assert!(err.to_string().contains(reason), "{err}");
    assert!(
        gateway.export_calls().is_empty(),
        "no export of any project"
    );
    assert!(gateway.find_calls().is_empty(), "no per-name lookup (K6)");
}

/// D-05, K6 a: a project named as its own parent is a cycle, whether or not
/// the gateway lists it yet.
#[tokio::test]
async fn parent_cycle_self_refuses() {
    let repo = checkout(&members_with_parent(PROJECT));
    let gateway = FakeProjects::new().with_names(2);
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "cycle", &gateway);

    // The project already on the gateway changes nothing.
    let gateway = FakeProjects::new().with_project(PROJECT, None, true, &base_members());
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "cycle", &gateway);
}

/// D-05, K6 a: a parent whose own chain reaches the project is a cycle.
#[tokio::test]
async fn parent_cycle_chain_refuses() {
    let repo = checkout(&members_with_parent("Mid"));

    // Mid -> Top -> CapApp, with CapApp on the gateway.
    let gateway = with_parent_project(FakeProjects::new(), "Top", Some(PROJECT), true);
    let gateway = with_parent_project(gateway, "Mid", Some("Top"), true);
    let gateway = gateway.with_project(PROJECT, None, true, &base_members());
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "cycle", &gateway);

    // Mid -> CapApp, with CapApp not on the gateway at all (a create).
    let gateway = with_parent_project(FakeProjects::new(), "Mid", Some(PROJECT), true);
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "cycle", &gateway);
}

/// K6 a: a parent the gateway lists but does not allow as one refuses.
#[tokio::test]
async fn parent_not_inheritable_refuses() {
    let repo = checkout(&members_with_parent("Closed"));
    let gateway = with_parent_project(FakeProjects::new(), "Closed", None, false);
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "not_inheritable", &gateway);
}

/// D-05: a parent the gateway does not list refuses `missing`.
#[tokio::test]
async fn parent_missing_refuses() {
    let repo = checkout(&members_with_parent("WHK-Global"));
    let gateway = FakeProjects::new().with_names(2);
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "missing", &gateway);
}

/// T-25-31, K6: a ring of other projects never ends, and the walk over it
/// reads the project list once and looks nothing up, however long it spins.
#[tokio::test]
async fn parent_chain_is_bounded() {
    let repo = checkout(&members_with_parent("Ring-A"));
    let gateway = with_parent_project(FakeProjects::new(), "Ring-A", Some("Ring-B"), true);
    let gateway = with_parent_project(gateway, "Ring-B", Some("Ring-C"), true);
    let gateway = with_parent_project(gateway, "Ring-C", Some("Ring-A"), true);
    let err = prepare_err(&repo, &gateway).await;
    assert_parent_refused(&err, "cycle", &gateway);
    assert_eq!(
        gateway.list_count(),
        1,
        "one list read, not one find per hop"
    );
    assert!(gateway.find_calls().is_empty(), "zero finds");
}

/// D-05: a live project under another parent is reparented by an import.
/// That is a warning naming both parents, not a refusal.
#[tokio::test]
async fn reparent_warns() {
    let repo = checkout(&members_with_parent("WHK-Global"));
    let gateway = with_parent_project(FakeProjects::new(), "WHK-Global", None, true);
    let gateway = with_parent_project(gateway, "Old", None, true);
    let gateway = gateway.with_project(PROJECT, Some("Old"), true, &differing_live());

    let result = prepare_with(
        &repo,
        &gateway,
        &first_change_input(&repo, Some("adopt the repo")),
    )
    .await
    .expect("a reparent is not a refusal");
    assert_eq!(result.action, ProjectAction::Import);
    assert_eq!(result.parent.as_deref(), Some("WHK-Global"));
    let reparent: Vec<&String> = result
        .warnings
        .iter()
        .filter(|w| w.contains("Old") && w.contains("WHK-Global"))
        .collect();
    assert_eq!(reparent.len(), 1, "{:?}", result.warnings);

    // The same parent on both sides is silent.
    let gateway = with_parent_project(FakeProjects::new(), "WHK-Global", None, true);
    let gateway = gateway.with_project(PROJECT, Some("WHK-Global"), true, &differing_live());
    let result = prepare_with(
        &repo,
        &gateway,
        &first_change_input(&repo, Some("adopt the repo")),
    )
    .await
    .expect("check");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);

    // A project the gateway does not have yet is not a reparent.
    let gateway = with_parent_project(FakeProjects::new(), "WHK-Global", None, true);
    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.action, ProjectAction::Create);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

/// T-25-32, D-05: a child check holds one list read, one lookup and one export
/// of the child itself. Nothing is asked of the parent by name, and the fake
/// has no write to call.
#[tokio::test]
async fn child_check_reads_parent_only() {
    let repo = checkout(&members_with_parent("WHK-Global"));
    let gateway = with_parent_project(FakeProjects::new(), "WHK-Global", None, true);
    let gateway = gateway.with_project(PROJECT, Some("WHK-Global"), true, &differing_live());

    let result = prepare_with(
        &repo,
        &gateway,
        &first_change_input(&repo, Some("adopt the repo")),
    )
    .await
    .expect("check");
    assert_eq!(result.parent.as_deref(), Some("WHK-Global"));
    assert_eq!(gateway.list_count(), 1);
    assert_eq!(gateway.find_calls(), vec![PROJECT.to_string()]);
    assert_eq!(gateway.export_calls(), vec![PROJECT.to_string()]);
}

// ---------------------------------------------------------------------------
// 25-11: the write path. `deploy_projects_run` over `FakeProjects`, on a local
// node whose data directory is the checkout's own.
// ---------------------------------------------------------------------------

const RECORD_REL: &str = "ign-deploy/capgw/projects/CapApp/release.json";
const OTHER_CODE: &[u8] = b"def f():\n    return 'someone else'\n";

/// A checkout whose gateway already holds a differing `CapApp`, with a record
/// of an earlier release that names exactly that live content: the import is
/// an ordinary one (not a first deploy) and nothing has drifted (K13).
async fn recorded_import(extra_projects: usize) -> (Checkout, FakeProjects) {
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_names(extra_projects).with_project(
        PROJECT,
        None,
        false,
        &differing_live(),
    );
    let live = gateway
        .export_bytes(PROJECT)
        .await
        .expect("the fake exports its project");
    let digest =
        ignition_core::deploy::project_artifact::canonical_digest(&live).expect("a project export");
    put_record(&repo, "v0.9.0", &digest.sha256);
    (repo, gateway)
}

/// Prepare the plan for `input` against `gateway` without running it.
async fn plan(
    checkout: &Checkout,
    gateway: &dyn ProjectGateway,
    input: &ProjectsInput,
) -> (LoadedProject, ProjectsPrepared) {
    let loaded = load_project(input).expect("the checkout loads");
    let host = checkout.host();
    let prepared = deploy_projects_prepare(
        &loaded,
        input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
    )
    .await
    .expect("the check completes");
    (loaded, prepared)
}

/// Run a prepared plan the way the CLI does, optionally with a writer double.
async fn run(
    checkout: &Checkout,
    gateway: &dyn ProjectGateway,
    input: &ProjectsInput,
    loaded: &LoadedProject,
    prepared: ProjectsPrepared,
    writer: Option<&dyn ReleaseRecordWriter>,
) -> Result<DeployProjectsResult, CoreError> {
    run_with_policy(
        checkout,
        gateway,
        input,
        loaded,
        prepared,
        writer,
        DesignersPolicy::default(),
    )
    .await
}

/// [`run`] under an explicit Designer policy.
async fn run_with_policy(
    checkout: &Checkout,
    gateway: &dyn ProjectGateway,
    input: &ProjectsInput,
    loaded: &LoadedProject,
    prepared: ProjectsPrepared,
    writer: Option<&dyn ReleaseRecordWriter>,
    designers: DesignersPolicy,
) -> Result<DeployProjectsResult, CoreError> {
    run_observed(
        checkout,
        gateway,
        input,
        loaded,
        prepared,
        writer,
        designers,
        &NoProjectsObserver,
    )
    .await
}

/// [`run_with_policy`] reporting to `observer`.
#[allow(clippy::too_many_arguments)]
async fn run_observed(
    checkout: &Checkout,
    gateway: &dyn ProjectGateway,
    input: &ProjectsInput,
    loaded: &LoadedProject,
    prepared: ProjectsPrepared,
    writer: Option<&dyn ReleaseRecordWriter>,
    designers: DesignersPolicy,
    observer: &dyn ProjectsObserver,
) -> Result<DeployProjectsResult, CoreError> {
    let host = checkout.host();
    deploy_projects_run(
        prepared,
        loaded,
        input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: writer,
            designers,
            children_settle: CHILDREN_SETTLE,
            children_poll: CHILDREN_POLL,
        },
        observer,
    )
    .await
}

/// Prepare and run in one step.
async fn apply(
    checkout: &Checkout,
    gateway: &dyn ProjectGateway,
    input: &ProjectsInput,
) -> DeployProjectsResult {
    let (loaded, prepared) = plan(checkout, gateway, input).await;
    run(checkout, gateway, input, &loaded, prepared, None)
        .await
        .expect("the apply completes")
}

fn run_dir_of(checkout: &Checkout, result: &DeployProjectsResult) -> PathBuf {
    checkout
        .repo
        .join(result.run_dir.as_deref().expect("the run has a directory"))
}

/// The only run directory below `.ign/deploy/capgw`.
fn only_run_dir(checkout: &Checkout) -> PathBuf {
    let root = checkout.repo.join(".ign/deploy/capgw");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("read {}: {e}", root.display()))
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(dirs.len(), 1, "exactly one run directory: {dirs:?}");
    dirs.remove(0)
}

fn record_path(checkout: &Checkout) -> PathBuf {
    checkout.data.join(RECORD_REL)
}

fn read_record(checkout: &Checkout) -> ReleaseRecord {
    let bytes = std::fs::read(record_path(checkout)).expect("the record exists");
    ReleaseRecord::parse(&bytes, RECORD_REL, "capgw", PROJECT).expect("the record parses")
}

/// The calls the run made, after the `before` the plan made.
fn calls_after(gateway: &FakeProjects, before: usize) -> Vec<String> {
    gateway.call_log().split_off(before)
}

/// DPRJ-03: the live project is saved, then replaced, then read back; the
/// record names the artifact. The order is the safety property.
#[tokio::test]
async fn export_before_import_and_verify() {
    let (repo, gateway) = recorded_import(0).await;
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;
    let before = gateway.call_log().len();

    let result = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect("the apply completes");

    assert_eq!(
        calls_after(&gateway, before),
        vec![
            "export:CapApp",
            "import:CapApp:true",
            "export:CapApp",
            "list"
        ],
        "pre-image, import, verify, then the listing that finds the children (25-15)"
    );
    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.ok && result.record_written, "{result:?}");
    assert!(!result.check);
    assert_eq!(
        gateway.stored_member(PROJECT, "ignition/script-python/cap/code.py"),
        Some(CODE_V1.to_vec()),
        "the gateway now holds the repo's code"
    );

    // The pre-image is the live project as the plan saw it.
    let run_dir = run_dir_of(&repo, &result);
    let pre = std::fs::read(run_dir.join("export/CapApp.zip")).expect("the pre-image export");
    let pre_digest = ignition_core::deploy::project_artifact::canonical_digest(&pre)
        .expect("the pre-image is a project export");
    assert_eq!(
        result.live_sha256.as_deref(),
        Some(pre_digest.sha256.as_str()),
        "the saved export is the project the plan was drawn against"
    );
    for name in ["run.json", "plan.json", "events.ndjson", "result.json"] {
        assert!(run_dir.join(name).is_file(), "{name} is in the run dir");
    }
    let events = std::fs::read_to_string(run_dir.join("events.ndjson")).expect("events");
    let names: Vec<String> = events
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).expect("an event line")["event"]
                .as_str()
                .expect("an event name")
                .to_string()
        })
        .collect();
    assert_eq!(
        names,
        [
            "run_started",
            "pre_image_exported",
            "import_sent",
            "import_answered",
            "verified",
            "record_written",
            "finished"
        ]
    );

    let record = read_record(&repo);
    assert_eq!(record.release, "v1.0.0");
    assert_eq!(record.artifact_sha256, artifact_sha(&repo));
    assert_eq!(Some(record.run_id.as_str()), result.run_id.as_deref());
}

/// D-13: a project the gateway lacks is created: `overwrite=false`, no
/// pre-image, and the verify still runs.
#[tokio::test]
async fn create_has_no_pre_image() {
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_names(2);
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;
    let before = gateway.call_log().len();

    let result = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect("the create completes");

    assert_eq!(
        calls_after(&gateway, before),
        vec!["import:CapApp:false", "export:CapApp"],
        "no pre-image export; the verify export follows the import"
    );
    assert_eq!(result.action, ProjectAction::Create);
    assert_eq!(result.outcome, ProjectOutcome::Created);
    assert!(result.ok && result.record_written, "{result:?}");
    assert!(gateway.has_project(PROJECT));
    let run_dir = run_dir_of(&repo, &result);
    assert!(
        !run_dir.join("export").exists(),
        "a create has nothing to save"
    );
    assert_eq!(read_record(&repo).artifact_sha256, artifact_sha(&repo));
}

/// K15: another administrator edits the live project between the plan and the
/// run. The saved pre-image no longer matches the plan, so nothing is
/// imported, the other administrator's edit is still there, and
/// `result.json` carries the slug.
#[tokio::test]
async fn live_changed_since_plan_is_stale() {
    let (repo, gateway) = recorded_import(0).await;
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;
    gateway.change_live(PROJECT, "ignition/script-python/cap/code.py", OTHER_CODE);

    let err = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("a moved live project is stale");

    match &err {
        CoreError::PlanStale { detail, .. } => {
            assert_eq!(detail, "live project changed since the plan");
        }
        other => panic!("expected plan_stale, got {other:?}"),
    }
    assert_eq!(err.code(), "plan_stale");
    assert!(
        gateway.import_calls().is_empty(),
        "nothing was imported: {:?}",
        gateway.import_calls()
    );
    assert_eq!(
        gateway.stored_member(PROJECT, "ignition/script-python/cap/code.py"),
        Some(OTHER_CODE.to_vec()),
        "the other administrator's edit survives"
    );
    assert_eq!(
        read_record(&repo).release,
        "v0.9.0",
        "no release record was written over the earlier one"
    );

    let run_dir = only_run_dir(&repo);
    let result: serde_json::Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("result.json")).expect("result.json"))
            .expect("result.json is json");
    assert_eq!(result["slug"], "plan_stale");
    assert_eq!(result["ok"], false);
}

/// Research Pitfall 1: a gateway that answers an empty `changes` list but
/// adopted the artifact is a warning, and the verify decides the outcome.
#[tokio::test]
async fn empty_changes_warns() {
    let (repo, gateway) = recorded_import(0).await;
    gateway.answer_empty_changes();

    let result = apply(&repo, &gateway, &repo.input()).await;

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.ok && result.record_written, "{result:?}");
    assert_eq!(
        result.warnings,
        vec!["the gateway reported no changes for this import".to_string()]
    );
}

/// DPRJ-05: a full check of every kind of plan against a gateway that panics
/// on a write completes, and leaves no run directory and no record.
#[tokio::test]
async fn check_is_read_only() {
    // create: the project is absent
    let create = checkout(&base_members());
    let absent = ForbidProjectWrites(FakeProjects::new().with_names(2));
    assert_eq!(
        prepare_forbidden(&create, &absent).await.action,
        ProjectAction::Create
    );
    // import: the live project differs and a record exists
    let (import, differing) = recorded_import(1).await;
    let differing = ForbidProjectWrites(differing);
    assert_eq!(
        prepare_forbidden(&import, &differing).await.action,
        ProjectAction::Import
    );
    // record: equal and no record
    let members = base_members();
    let record = checkout(&members);
    let equal =
        ForbidProjectWrites(FakeProjects::new().with_project(PROJECT, None, false, &members));
    assert_eq!(
        prepare_forbidden(&record, &equal).await.action,
        ProjectAction::Record
    );
    // skip: equal and the same release recorded
    let skip = checkout(&members);
    put_matching_record(&skip, "v1.0.0");
    let equal =
        ForbidProjectWrites(FakeProjects::new().with_project(PROJECT, None, false, &members));
    assert_eq!(
        prepare_forbidden(&skip, &equal).await.action,
        ProjectAction::Skip
    );

    for (name, fixture) in [
        ("create", &create),
        ("import", &import),
        ("record", &record),
        ("skip", &skip),
    ] {
        assert!(
            !fixture.repo.join(".ign").exists(),
            "{name}: a check creates no run directory"
        );
    }
    assert!(!record_path(&create).exists(), "create: no record");
    assert!(!record_path(&record).exists(), "record: no record");
    // the import fixture holds only the record it was seeded with
    assert_eq!(read_record(&import).release, "v0.9.0");
    // skip: still the record it was seeded with, byte for byte
    assert_eq!(read_record(&skip).release, "v1.0.0");
}

async fn prepare_forbidden<T: ProjectGateway>(
    checkout: &Checkout,
    gateway: &ForbidProjectWrites<T>,
) -> DeployProjectsResult {
    let input = checkout.input();
    let (_, prepared) = plan(checkout, gateway, &input).await;
    prepared.into_check_result()
}

/// DPRJ-04: whatever the run does to `CapApp`, every other project in the
/// gateway is byte-identical afterwards, for the create, the import, the
/// record-only and the refused (stale) run. The fake has no delete method, so
/// the only write the verb can make is an import of its own project.
#[tokio::test]
async fn other_projects_untouched() {
    // import
    let (repo, gateway) = recorded_import(3).await;
    let before = gateway.snapshot_except(PROJECT);
    assert_eq!(before.len(), 3);
    apply(&repo, &gateway, &repo.input()).await;
    assert_eq!(gateway.snapshot_except(PROJECT), before, "import");
    assert_eq!(gateway.import_calls(), vec![(PROJECT.to_string(), true)]);

    // create
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_names(3);
    let before = gateway.snapshot_except(PROJECT);
    apply(&repo, &gateway, &repo.input()).await;
    assert_eq!(gateway.snapshot_except(PROJECT), before, "create");
    assert_eq!(gateway.import_calls(), vec![(PROJECT.to_string(), false)]);

    // record only
    let members = base_members();
    let repo = checkout(&members);
    let gateway = FakeProjects::new()
        .with_names(3)
        .with_project(PROJECT, None, false, &members);
    let before = gateway.snapshot_except(PROJECT);
    let result = apply(&repo, &gateway, &repo.input()).await;
    assert_eq!(result.outcome, ProjectOutcome::Recorded);
    assert_eq!(gateway.snapshot_except(PROJECT), before, "record");
    assert!(
        gateway.import_calls().is_empty(),
        "record-only imports nothing"
    );

    // refused as stale
    let (repo, gateway) = recorded_import(3).await;
    let before = gateway.snapshot_except(PROJECT);
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;
    gateway.change_live(PROJECT, "ignition/script-python/cap/code.py", OTHER_CODE);
    run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("stale");
    assert_eq!(gateway.snapshot_except(PROJECT), before, "stale");
}

/// A record that cannot be written after a verified import does not undo or
/// fail the deploy: a warning with the error code only, `record_written`
/// false.
#[tokio::test]
async fn record_write_failure_is_warning() {
    let (repo, gateway) = recorded_import(0).await;
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let result = run(
        &repo,
        &gateway,
        &input,
        &loaded,
        prepared,
        Some(&FailingRecordWriter),
    )
    .await
    .expect("a failed record write is not a failed deploy");

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.ok, "the import is verified");
    assert!(!result.record_written);
    assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
    let warning = &result.warnings[0];
    assert!(warning.contains("transport_failed"), "{warning}");
    assert!(
        !warning.contains("the record cannot be written"),
        "no error text beyond the code: {warning}"
    );
    assert_eq!(
        gateway.stored_member(PROJECT, "ignition/script-python/cap/code.py"),
        Some(CODE_V1.to_vec()),
        "the import stands"
    );
    // The seeded record of the earlier release is untouched.
    assert_eq!(read_record(&repo).release, "v0.9.0");
}

// ---- 25-14: the wrong-target and Designer pre-checks -----------------------

/// A session as the Designer list reports it: user, project, host.
fn session(user: &str) -> (String, String, String) {
    (user.to_string(), PROJECT.to_string(), "ws-1".to_string())
}

/// `--designers wait`, polling every millisecond for `timeout`.
fn wait_policy(timeout: Duration) -> DesignersPolicy {
    DesignersPolicy {
        mode: DesignersMode::Wait,
        timeout,
        interval: Duration::from_millis(1),
    }
}

/// The only failure of a `deploy_preflight_failed` refusal, as (check, detail).
fn preflight_failure(err: &CoreError) -> (String, String) {
    let CoreError::DeployPreflightFailed { failures } = err else {
        panic!("expected deploy_preflight_failed, got {err:?}");
    };
    assert_eq!(failures.len(), 1, "{failures:?}");
    (failures[0].check.clone(), failures[0].detail.clone())
}

/// A refused run wrote nothing anywhere: no run directory, no import, and
/// no release record beyond the one `recorded_import` seeded (`v0.9.0`).
fn assert_nothing_written(repo: &Checkout, gateway: &FakeProjects) {
    assert!(!repo.repo.join(".ign").exists(), "no run directory");
    assert!(gateway.import_calls().is_empty(), "no import");
    if record_path(repo).exists() {
        assert_eq!(
            read_record(repo).release,
            "v0.9.0",
            "the record is untouched"
        );
    }
}

/// T-25-44: the target names `gw-a`, the gateway answers `gw-b` (the host
/// agrees with the gateway): refused `wrong_target`, nothing written.
#[tokio::test]
async fn wrong_target_name_mismatch_refuses() {
    let (repo, gateway) = recorded_import(0).await;
    repo.declare_gateway_name("gw-a");
    repo.set_host_system_name("gw-b");
    let gateway = gateway.with_gateway_name("gw-b");
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;
    let exports = gateway.export_calls().len();

    let err = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("a gateway that is not the declared one is refused");

    let (check, detail) = preflight_failure(&err);
    assert_eq!(check, "wrong_target");
    assert!(detail.contains("nodes.master.gateway_name"), "{detail}");
    assert!(
        !detail.contains("gw-a") && !detail.contains("gw-b"),
        "{detail}"
    );
    assert_eq!(gateway.export_calls().len(), exports, "no pre-image export");
    assert_nothing_written(&repo, &gateway);
}

/// T-25-44: the REST name is not the host's `systemName`, read through the
/// local node's read half: the same refusal.
#[tokio::test]
async fn wrong_target_host_name_mismatch_refuses() {
    let (repo, gateway) = recorded_import(0).await;
    repo.set_host_system_name("some-other-gateway");
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let err = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("a host that is not the gateway is refused");

    let (check, detail) = preflight_failure(&err);
    assert_eq!(check, "wrong_target");
    assert!(detail.contains("host's systemName"), "{detail}");
    assert!(!detail.contains("some-other-gateway"), "{detail}");
    assert_nothing_written(&repo, &gateway);
}

/// A gateway that reports no name cannot be proven to be the target.
#[tokio::test]
async fn wrong_target_gateway_without_a_name_refuses() {
    let (repo, gateway) = recorded_import(0).await;
    let gateway = gateway.with_no_gateway_name();
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let err = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("an unnamed gateway is refused");

    let (check, detail) = preflight_failure(&err);
    assert_eq!(check, "wrong_target");
    assert!(detail.contains("no name over REST"), "{detail}");
    assert_nothing_written(&repo, &gateway);
}

/// The release record is a write to the host too: a `record` plan against
/// the wrong gateway refuses before the record is written.
#[tokio::test]
async fn wrong_target_applies_to_record_only() {
    let members = base_members();
    let repo = checkout(&members);
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members)
        .with_gateway_name("gw-b");
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;
    assert!(
        matches!(&prepared, ProjectsPrepared::Ready { plan, .. } if plan.action == ProjectAction::Record),
        "the gateway already holds the artifact, so only the record is planned"
    );

    let err = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("the record is not written to the wrong gateway");

    let (check, _) = preflight_failure(&err);
    assert_eq!(check, "wrong_target");
    assert_nothing_written(&repo, &gateway);
    assert_eq!(
        gateway.designers_reads(),
        0,
        "wrong_target is checked first"
    );
}

/// A matching REST name, host name and declared name pass: the import runs.
#[tokio::test]
async fn wrong_target_all_three_names_agree_proceeds() {
    let (repo, gateway) = recorded_import(0).await;
    repo.declare_gateway_name("capgw");
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let result = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect("a gateway that is the declared one is deployed to");

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert_eq!(gateway.import_calls().len(), 1);
}

/// D-15: one session, then none. `--designers wait` polls once more and the
/// import runs.
#[tokio::test]
async fn designers_wait_clears() {
    let (repo, gateway) = recorded_import(0).await;
    let gateway = gateway.with_designers_script(vec![vec![session("alice")], vec![]]);
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let result = run_with_policy(
        &repo,
        &gateway,
        &input,
        &loaded,
        prepared,
        None,
        wait_policy(Duration::from_secs(30)),
    )
    .await
    .expect("the import runs once the Designer is closed");

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert_eq!(gateway.designers_reads(), 2, "one session, then none");
    assert_eq!(gateway.import_calls().len(), 1);
}

/// D-15: sessions stay open past the timeout: refused `designers`, nothing
/// written.
#[tokio::test]
async fn designers_wait_times_out() {
    let (repo, gateway) = recorded_import(0).await;
    let gateway = gateway.with_designers_script(vec![vec![session("alice")]]);
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let err = run_with_policy(
        &repo,
        &gateway,
        &input,
        &loaded,
        prepared,
        None,
        wait_policy(Duration::from_secs(1)),
    )
    .await
    .expect_err("a Designer that never closes refuses the import");

    let (check, detail) = preflight_failure(&err);
    assert_eq!(check, "designers");
    assert!(detail.contains("alice"), "{detail}");
    assert!(gateway.designers_reads() > 1, "it polled before it gave up");
    assert_nothing_written(&repo, &gateway);
}

/// An unanswerable Designer read is its own error: `network_error`, never
/// folded into `deploy_preflight_failed`.
#[tokio::test]
async fn preflight_read_error_is_not_preflight_failed() {
    let (repo, gateway) = recorded_import(0).await;
    let gateway = gateway.with_designers_error();
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let err = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect_err("an unreadable Designer list is an error");

    assert!(matches!(err, CoreError::Network { .. }), "{err:?}");
    assert_eq!(err.code(), "network_error");
    assert_nothing_written(&repo, &gateway);
}

/// A `record` plan does not touch the project, so open Designer sessions do
/// not refuse it and the Designer list is not even read.
#[tokio::test]
async fn record_only_skips_the_designers_check() {
    let members = base_members();
    let repo = checkout(&members);
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members)
        .with_designers_script(vec![vec![session("alice")]]);
    let input = repo.input();
    let (loaded, prepared) = plan(&repo, &gateway, &input).await;

    let result = run(&repo, &gateway, &input, &loaded, prepared, None)
        .await
        .expect("a record-only run ignores open Designer sessions");

    assert_eq!(result.outcome, ProjectOutcome::Recorded);
    assert_eq!(gateway.designers_reads(), 0);
    assert!(record_path(&repo).is_file());
}

// ---- 25-15: the children of a parent project -------------------------------

const CHILD_PROJECT_JSON: &str = r#"{"title":"Child","enabled":true,"parent":"CapApp"}"#;

/// The members of an inheriting project: a descriptor and one script.
fn child_members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("project.json", CHILD_PROJECT_JSON.as_bytes().to_vec()),
        (
            "ignition/script-python/child/resource.json",
            SCRIPT_RESOURCE.as_bytes().to_vec(),
        ),
        ("ignition/script-python/child/code.py", CODE_V1.to_vec()),
    ]
}

/// The canonical digest of what the gateway now holds for `name`. Reads it
/// through the trait, so it shows in the fake's counts and call log.
async fn digest_of(gateway: &FakeProjects, name: &str) -> String {
    let zip = gateway
        .export_bytes(name)
        .await
        .expect("the fake exports its project");
    ignition_core::deploy::project_artifact::canonical_digest(&zip)
        .expect("a project export")
        .sha256
}

/// Write the release record `ign` left for the child `name` on the master
/// node.
fn put_child_record(checkout: &Checkout, name: &str, release: &str, artifact_sha256: &str) {
    let record = ReleaseRecord {
        schema_version: 1,
        gateway: "capgw".to_string(),
        project: name.to_string(),
        release: release.to_string(),
        artifact_sha256: artifact_sha256.to_string(),
        run_id: "20261020T101500Z-cd34".to_string(),
        deployed_at: "2026-10-20T10:15:00Z".to_string(),
    };
    put(
        &checkout.data,
        &format!("ign-deploy/capgw/projects/{name}/release.json"),
        &record.to_bytes(),
    );
}

/// A recorded overwrite of `CapApp` whose gateway also holds `ChildA`, with a
/// record naming exactly what the child holds now.
async fn parent_with_recorded_child() -> (Checkout, FakeProjects, String) {
    let (repo, gateway) = recorded_import(0).await;
    let gateway = gateway.with_child("ChildA", PROJECT, &child_members());
    let digest = digest_of(&gateway, "ChildA").await;
    put_child_record(&repo, "ChildA", "v0.4.0", &digest);
    (repo, gateway, digest)
}

fn warnings_naming<'a>(result: &'a DeployProjectsResult, child: &str) -> Vec<&'a String> {
    result
        .warnings
        .iter()
        .filter(|warning| warning.contains(child))
        .collect()
}

/// K7: a child that is still changing after the parent import is read again
/// until two reads in a row agree, and the status is the one for the stable
/// read.
#[tokio::test]
async fn children_settle_waits_for_stable_exports() {
    let (repo, gateway, recorded) = parent_with_recorded_child().await;
    gateway.mutate_child_after_import("ChildA", 2);
    let before = gateway.export_count("ChildA");

    let result = apply(&repo, &gateway, &repo.input()).await;

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert_eq!(
        gateway.export_count("ChildA") - before,
        3,
        "two reads that changed, then one that agreed with the one before"
    );
    let stable = digest_of(&gateway, "ChildA").await;
    assert_ne!(stable, recorded, "the child did change");
    assert_eq!(result.children.len(), 1, "{:?}", result.children);
    let child = &result.children[0];
    assert_eq!(child.name, "ChildA");
    assert_eq!(child.status, ChildStatus::Mismatch);
    assert_eq!(child.live_sha256.as_deref(), Some(stable.as_str()));
    assert_eq!(child.recorded_sha256.as_deref(), Some(recorded.as_str()));
    assert_eq!(child.release.as_deref(), Some("v0.4.0"));
    assert_eq!(
        warnings_naming(&result, "ChildA").len(),
        1,
        "the mismatch only, no did-not-settle warning: {:?}",
        result.warnings
    );
}

/// T-25-48: a child that changes on every read does not hold the run up past
/// `children_settle`; it is classified on the last read, with a warning.
#[tokio::test]
async fn children_settle_is_bounded() {
    let (repo, gateway, _recorded) = parent_with_recorded_child().await;
    gateway.mutate_child_after_import("ChildA", usize::MAX);

    let started = Instant::now();
    let result = apply(&repo, &gateway, &repo.input()).await;
    let took = started.elapsed();

    assert!(
        took >= CHILDREN_SETTLE && took < CHILDREN_SETTLE * 10,
        "the sweep stopped at the bound: {took:?}"
    );
    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.ok && result.record_written, "{result:?}");
    assert_eq!(result.children.len(), 1);
    assert_eq!(result.children[0].status, ChildStatus::Mismatch);
    assert!(
        result
            .warnings
            .contains(&"child project ChildA did not settle within 1 s".to_string()),
        "{:?}",
        result.warnings
    );
}

/// D-06: a child this tool never deployed has nothing to compare with.
#[tokio::test]
async fn child_without_record_is_unverified() {
    let (repo, gateway) = recorded_import(0).await;
    let gateway = gateway.with_child("ChildA", PROJECT, &child_members());
    let live = digest_of(&gateway, "ChildA").await;

    let result = apply(&repo, &gateway, &repo.input()).await;

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert_eq!(result.children.len(), 1);
    let child = &result.children[0];
    assert_eq!(child.status, ChildStatus::Unverified);
    assert_eq!(child.release, None);
    assert_eq!(child.recorded_sha256, None);
    assert_eq!(child.live_sha256.as_deref(), Some(live.as_str()));
    assert!(
        warnings_naming(&result, "ChildA").is_empty(),
        "a child never deployed by this tool is not a warning: {:?}",
        result.warnings
    );
}

/// D-06: a child whose export cannot be read is unverified, the warning
/// carries the code only, and the deploy stands as it was.
#[tokio::test]
async fn child_read_error_is_unverified() {
    let (repo, gateway, _recorded) = parent_with_recorded_child().await;
    let gateway = gateway.with_child("ChildB", PROJECT, &child_members());
    let b_digest = digest_of(&gateway, "ChildB").await;
    put_child_record(&repo, "ChildB", "v0.5.0", &b_digest);
    gateway.fail_export_of("ChildB");

    let result = apply(&repo, &gateway, &repo.input()).await;

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.ok && result.record_written, "{result:?}");
    assert_eq!(result.failure_exit_code(), None);
    assert_eq!(result.slug, None);
    let by_name: Vec<(&str, ChildStatus)> = result
        .children
        .iter()
        .map(|child| (child.name.as_str(), child.status))
        .collect();
    assert_eq!(
        by_name,
        vec![
            ("ChildA", ChildStatus::Ok),
            ("ChildB", ChildStatus::Unverified)
        ]
    );
    let b = &result.children[1];
    assert_eq!(b.live_sha256, None);
    assert_eq!(b.recorded_sha256.as_deref(), Some(b_digest.as_str()));
    assert_eq!(
        result.warnings,
        vec!["child project ChildB: its export could not be read (network_error)".to_string()]
    );
}

/// A [`ProjectHost`] over the checkout's data directory that remembers which
/// of its two methods were called, and for what.
struct RecordingHost {
    inner: ignition_core::deploy::transport::local::LocalFsTransport,
    calls: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl ProjectHost for RecordingHost {
    async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("git_entry:{project}"));
        self.inner.git_entry(project).await
    }

    async fn read_release_record(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .push(format!("read_release_record:{project}"));
        self.inner.read_release_record(gateway, project).await
    }
}

/// A [`ReleaseRecordWriter`] that remembers which projects it wrote.
struct RecordingWriter {
    inner: ignition_core::deploy::transport::local::LocalFsTransport,
    written: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl ReleaseRecordWriter for RecordingWriter {
    async fn write_release_record(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        self.written
            .lock()
            .expect("written")
            .push(project.to_string());
        self.inner
            .write_release_record(gateway, project, bytes)
            .await
    }
}

/// T-25-47, DPRJ-02: after the parent is replaced the sweep makes two kinds
/// of gateway call (the listing and the children's exports) and one kind of
/// host call (the children's records). No import, no write, nothing about a
/// child changes.
#[tokio::test]
async fn sweep_only_reads() {
    let (repo, gateway, _recorded) = parent_with_recorded_child().await;
    let gateway = gateway.with_child("ChildB", PROJECT, &child_members());
    let host = RecordingHost {
        inner: repo.host(),
        calls: Mutex::new(Vec::new()),
    };
    let writer = RecordingWriter {
        inner: repo.host(),
        written: Mutex::new(Vec::new()),
    };
    let children_before = gateway.snapshot_except(PROJECT);
    let records_before = std::fs::read(
        repo.data
            .join("ign-deploy/capgw/projects/ChildA/release.json"),
    )
    .expect("the child's record");

    let input = repo.input();
    let loaded = load_project(&input).expect("the checkout loads");
    let surfaces = ProjectsSurfaces {
        gateway: &gateway,
        host: &host,
    };
    let prepared = deploy_projects_prepare(&loaded, &input, &surfaces)
        .await
        .expect("the check completes");
    let gateway_before = gateway.call_log().len();
    let host_before = host.calls.lock().expect("calls").len();
    let result = deploy_projects_run(
        prepared,
        &loaded,
        &input,
        &surfaces,
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: Some(&writer),
            designers: DesignersPolicy::default(),
            children_settle: CHILDREN_SETTLE,
            children_poll: CHILDREN_POLL,
        },
        &NoProjectsObserver,
    )
    .await
    .expect("the apply completes");

    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert_eq!(result.children.len(), 2);
    let after_verify = calls_after(&gateway, gateway_before);
    let sweep_from = after_verify
        .iter()
        .position(|call| call == "list")
        .expect("the sweep lists the projects");
    let sweep = &after_verify[sweep_from..];
    assert!(
        sweep
            .iter()
            .all(|call| call == "list" || call == "export:ChildA" || call == "export:ChildB"),
        "the sweep reads the listing and the children's exports only: {sweep:?}"
    );
    assert_eq!(
        after_verify[..sweep_from],
        ["export:CapApp", "import:CapApp:true", "export:CapApp"],
        "everything before the sweep is the deploy of the parent"
    );
    assert_eq!(gateway.import_calls(), vec![(PROJECT.to_string(), true)]);
    let host_calls = host.calls.lock().expect("calls").split_off(host_before);
    assert_eq!(
        host_calls,
        vec![
            "read_release_record:ChildA".to_string(),
            "read_release_record:ChildB".to_string()
        ],
        "the run's only host calls are the children's record reads"
    );
    assert_eq!(
        *writer.written.lock().expect("written"),
        vec![PROJECT.to_string()],
        "the one record written is the parent's"
    );
    assert_eq!(gateway.snapshot_except(PROJECT), children_before);
    assert_eq!(
        std::fs::read(
            repo.data
                .join("ign-deploy/capgw/projects/ChildA/release.json")
        )
        .expect("the child's record"),
        records_before
    );
    assert!(
        !repo.data.join("ign-deploy/capgw/projects/ChildB").exists(),
        "no record was made for a child"
    );
}

/// D-06: only an `imported` or `imported_unverified` parent is followed by a
/// sweep, and a project with no children has nothing to sweep. In every case
/// `children` is the empty list and no child export was read.
#[tokio::test]
async fn no_sweep_without_children_or_import() {
    // An import of a project nothing inherits from: the listing is read, no
    // export but the parent's.
    let (repo, gateway) = recorded_import(2).await;
    let result = apply(&repo, &gateway, &repo.input()).await;
    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.children.is_empty(), "{:?}", result.children);
    assert!(
        gateway.export_calls().iter().all(|name| name == PROJECT),
        "{:?}",
        gateway.export_calls()
    );

    // created: the gateway lacks the parent, but lists a project naming it.
    let repo = checkout(&base_members());
    let gateway = FakeProjects::new().with_child("ChildA", PROJECT, &child_members());
    let result = apply(&repo, &gateway, &repo.input()).await;
    assert_eq!(result.outcome, ProjectOutcome::Created);
    assert!(result.children.is_empty(), "{:?}", result.children);
    assert_eq!(gateway.export_count("ChildA"), 0, "created");

    // recorded: a new release of unchanged content.
    let members = base_members();
    let repo = checkout(&members);
    put_matching_record(&repo, "v1.0.0");
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members)
        .with_child("ChildA", PROJECT, &child_members());
    let mut input = repo.input();
    input.release = "v1.1.0".to_string();
    let result = apply(&repo, &gateway, &input).await;
    assert_eq!(result.outcome, ProjectOutcome::Recorded);
    assert!(result.children.is_empty(), "{:?}", result.children);
    assert_eq!(gateway.export_count("ChildA"), 0, "recorded");

    // skipped: the same release and equal content.
    let repo = checkout(&members);
    put_matching_record(&repo, "v1.0.0");
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members)
        .with_child("ChildA", PROJECT, &child_members());
    let result = apply(&repo, &gateway, &repo.input()).await;
    assert_eq!(result.outcome, ProjectOutcome::Skipped);
    assert!(result.children.is_empty(), "{:?}", result.children);
    assert_eq!(gateway.export_count("ChildA"), 0, "skipped");

    // --check of an import that would have children.
    let (repo, gateway, _recorded) = parent_with_recorded_child().await;
    let before = gateway.export_count("ChildA");
    let result = prepare(&repo, &gateway).await;
    assert_eq!(result.outcome, ProjectOutcome::Checked);
    assert!(result.children.is_empty(), "{:?}", result.children);
    assert_eq!(gateway.export_count("ChildA"), before, "check");
}

/// D-06: a parent whose read-back differs from the artifact is still swept;
/// the exit code comes from the verdict alone.
#[tokio::test]
async fn unverified_parent_still_sweeps() {
    let (repo, gateway, recorded) = parent_with_recorded_child().await;
    // The first read of the parent after the import finds it edited, so the
    // read-back differs from the artifact.
    gateway.mutate_child_after_import(PROJECT, 1);

    let result = apply(&repo, &gateway, &repo.input()).await;

    assert_eq!(result.outcome, ProjectOutcome::ImportedUnverified);
    assert!(!result.ok && !result.record_written);
    assert_eq!(result.failure_exit_code(), Some(6));
    assert_eq!(result.children.len(), 1, "{:?}", result.children);
    let child = &result.children[0];
    assert_eq!(child.name, "ChildA");
    assert_eq!(child.status, ChildStatus::Ok);
    assert_eq!(child.live_sha256.as_deref(), Some(recorded.as_str()));
    assert!(
        warnings_naming(&result, "ChildA").is_empty(),
        "{:?}",
        result.warnings
    );
}

// ---- 25-17: the secret canary (SC-5, DSURF-03, DPRJ-03) --------------------

/// Scripts can embed credentials. This string sits in a script of the
/// checkout and in the live project the fake serves; the only file of a run
/// that may hold it afterwards is the pre-image, named by
/// [`PROJECTS_CANARY_EXEMPT`].
const CANARY: &str = "CANARY-PROJECT-25";
const CANARY_CHECKOUT: &[u8] = b"PASSWORD = 'CANARY-PROJECT-25'\n";
const CANARY_LIVE: &[u8] = b"PASSWORD = 'CANARY-PROJECT-25'\nOLD = 1\n";
/// What another administrator leaves the live project holding.
const CANARY_MOVED: &[u8] = b"PASSWORD = 'CANARY-PROJECT-25'\nMOVED = 2\n";
const CODE_PATH: &str = "ignition/script-python/cap/code.py";
const CHILD_CODE_PATH: &str = "ignition/script-python/child/code.py";

fn holds_canary(bytes: &[u8]) -> bool {
    bytes.windows(CANARY.len()).any(|w| w == CANARY.as_bytes())
}

/// Whether any member of the zip `bytes` holds the canary. The members are
/// deflated, so the canary is not a literal run of bytes in the archive.
fn zip_holds_canary(bytes: &[u8]) -> bool {
    use std::io::Read as _;
    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return false;
    };
    (0..archive.len()).any(|i| {
        let mut member = Vec::new();
        archive
            .by_index(i)
            .expect("member")
            .read_to_end(&mut member)
            .expect("read member");
        holds_canary(&member)
    })
}

/// Every file below `dir`, relative to it, sorted.
fn files_below(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                found.push(path.strip_prefix(root).expect("below root").to_path_buf());
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// Whether `rel` (below a run directory) is named by
/// [`PROJECTS_CANARY_EXEMPT`]: a bare name matches the top-level entry and
/// everything below it.
fn is_canary_exempt(rel: &Path) -> bool {
    let first = rel
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    PROJECTS_CANARY_EXEMPT.contains(&first.as_str())
}

/// The files below `dir` that hold the canary, as bytes or inside a zip,
/// leaving out the entries [`PROJECTS_CANARY_EXEMPT`] names when
/// `honour_exempt`.
fn canary_files(dir: &Path, honour_exempt: bool) -> Vec<PathBuf> {
    files_below(dir)
        .into_iter()
        .filter(|rel| !(honour_exempt && is_canary_exempt(rel)))
        .filter(|rel| {
            let bytes = std::fs::read(dir.join(rel)).expect("read file");
            holds_canary(&bytes) || zip_holds_canary(&bytes)
        })
        .collect()
}

/// The run directories below `.ign/deploy/capgw`, if any.
fn run_dirs(checkout: &Checkout) -> Vec<PathBuf> {
    match std::fs::read_dir(checkout.repo.join(".ign/deploy/capgw")) {
        Ok(entries) => entries.map(|e| e.expect("entry").path()).collect(),
        Err(_) => Vec::new(),
    }
}

/// The canary is in no scanned file of any run directory and not in the
/// release records. The scan is never vacuous: each run directory must hold
/// the four audit files and the walk must have found them. Returns the run
/// directories scanned.
fn assert_run_files_canary_free(checkout: &Checkout, what: &str) -> Vec<PathBuf> {
    let dirs = run_dirs(checkout);
    for dir in &dirs {
        let walked = files_below(dir);
        for file in ["run.json", "plan.json", "events.ndjson", "result.json"] {
            assert!(
                walked.iter().any(|rel| rel == Path::new(file)),
                "{file} was scanned in {dir:?}: {what}"
            );
        }
        let leaks = canary_files(dir, true);
        assert!(leaks.is_empty(), "canary in {leaks:?} of {dir:?}: {what}");
    }
    let records = checkout.data.join("ign-deploy");
    if records.exists() {
        let leaks = canary_files(&records, false);
        assert!(leaks.is_empty(), "canary in the record {leaks:?}: {what}");
    }
    dirs
}

/// The one exempt file exists, holds the canary, is the only exempt file and
/// is owner-only inside owner-only directories: the exemption hides nothing.
fn assert_exempt_pre_image(run_dir: &Path, what: &str) {
    let exempt: Vec<PathBuf> = files_below(run_dir)
        .into_iter()
        .filter(|rel| is_canary_exempt(rel))
        .collect();
    assert_eq!(
        exempt,
        [PathBuf::from("export/CapApp.zip")],
        "the pre-image is the only exempt file: {what}"
    );
    let pre = run_dir.join("export/CapApp.zip");
    assert!(
        zip_holds_canary(&std::fs::read(&pre).expect("pre-image")),
        "the exempt file holds the canary (the exemption is not vacuous): {what}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode(&pre), 0o600, "the pre-image is 0600: {what}");
        assert_eq!(mode(&run_dir.join("export")), 0o700, "{what}");
        assert_eq!(mode(run_dir), 0o700, "{what}");
    }
}

/// An error says nothing of the project: not its display, debug form, hint,
/// endpoint or code.
fn assert_error_canary_free(err: &CoreError, what: &str) {
    for (field, text) in [
        ("display", err.to_string()),
        ("debug", format!("{err:?}")),
        ("hint", err.hint().unwrap_or_default()),
        ("endpoint", err.endpoint().unwrap_or_default()),
        ("code", err.code().to_string()),
    ] {
        assert!(!text.contains(CANARY), "canary in the {field}: {what}");
    }
}

/// No run directory exists and the gateway was never written to.
fn assert_no_run(checkout: &Checkout, gateway: &FakeProjects, what: &str) {
    assert!(
        !checkout.repo.join(".ign").exists(),
        "no run directory: {what}"
    );
    assert!(gateway.import_calls().is_empty(), "no import: {what}");
}

/// The members of the checkout: the base project with the canary in a script.
fn canary_checkout_members() -> Vec<(&'static str, Vec<u8>)> {
    replace(&base_members(), CODE_PATH, CANARY_CHECKOUT)
}

/// A checkout and a gateway that both hold the canary, the gateway holding a
/// different project (so an import has something to replace). With `release`,
/// the record of that release names exactly what the gateway holds.
async fn canary_import(release: Option<&str>) -> (Checkout, FakeProjects) {
    let repo = checkout(&canary_checkout_members());
    let live = replace(&base_members(), CODE_PATH, CANARY_LIVE);
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &live);
    if let Some(release) = release {
        let digest = digest_of(&gateway, PROJECT).await;
        put_record(&repo, release, &digest);
    }
    (repo, gateway)
}

/// The seeds are real: the checkout's artifact and the gateway's project both
/// hold the canary, so a clean scan means something.
fn assert_seeded(checkout: &Checkout, gateway: &FakeProjects) {
    let loaded = load_project(&checkout.input()).expect("the checkout loads");
    assert!(
        zip_holds_canary(&loaded.artifact.zip),
        "the artifact holds the canary"
    );
    assert!(
        holds_canary(
            &gateway
                .stored_member(PROJECT, CODE_PATH)
                .expect("the live script")
        ),
        "the live project holds the canary"
    );
}

/// Records every event the run reports, in debug form.
#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl ProjectsObserver for Recorder {
    fn event(&self, event: &ProjectsEvent) {
        self.0.lock().expect("events").push(format!("{event:?}"));
    }
}

impl Recorder {
    /// The observer heard the run (so a clean scan of it means something).
    fn assert_heard(&self, what: &str) {
        assert!(
            !self.0.lock().expect("events").is_empty(),
            "the observer heard the run: {what}"
        );
    }

    fn assert_canary_free(&self, what: &str) {
        for event in self.0.lock().expect("events").iter() {
            assert!(!event.contains(CANARY), "canary in event {event}: {what}");
        }
    }
}

/// A [`ProjectGateway`] that forwards every read and answers the import with
/// the gateway's own denial (`import_denied`).
struct DenyImport<'a>(&'a FakeProjects);

#[async_trait::async_trait]
impl ProjectGateway for DenyImport<'_> {
    async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError> {
        self.0.projects().await
    }

    async fn project_find(&self, name: &str) -> Result<Option<ProjectRecord>, CoreError> {
        self.0.project_find(name).await
    }

    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, CoreError> {
        self.0.export_bytes(name).await
    }

    async fn designers(&self) -> Result<Vec<DesignerSeen>, CoreError> {
        self.0.designers().await
    }

    async fn gateway_name(&self) -> Result<Option<String>, CoreError> {
        self.0.gateway_name().await
    }

    async fn project_import(
        &self,
        name: &str,
        _zip: Vec<u8>,
        _overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        Err(CoreError::ImportDenied {
            project: name.to_string(),
            problem: "the gateway refused the import".to_string(),
            endpoint: Some("http://fake-gateway.invalid/data/api/v1/projects/import".to_string()),
        })
    }
}

/// Plan and run `input` against `gateway`, hearing every event.
async fn run_heard(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
    input: &ProjectsInput,
    designers: DesignersPolicy,
) -> (Result<DeployProjectsResult, CoreError>, Recorder) {
    let (loaded, prepared) = plan(repo, gateway, input).await;
    let recorder = Recorder::default();
    let result = run_observed(
        repo, gateway, input, &loaded, prepared, None, designers, &recorder,
    )
    .await;
    (result, recorder)
}

/// DPRJ-03, SC-5: every refusal that stops a run before it writes anything
/// says nothing of the project, and leaves no run directory and no import.
/// The checkout and the live project both hold the canary throughout.
#[tokio::test]
async fn canary_refusals_clean() {
    // project_hash_mismatch: refused at load, before any gateway or host call.
    {
        let (repo, gateway) = canary_import(Some("v0.9.0")).await;
        assert_seeded(&repo, &gateway);
        let mut input = repo.input();
        input.artifact_sha256 = Some("0".repeat(64));
        let err = load_project(&input).expect_err("a wrong reviewed hash refuses");
        assert_eq!(err.code(), "project_hash_mismatch");
        assert_error_canary_free(&err, "project_hash_mismatch");
        assert_no_run(&repo, &gateway, "project_hash_mismatch");
    }

    // project_git_managed: a `.git` entry under the project's directory.
    {
        let (repo, gateway) = canary_import(Some("v0.9.0")).await;
        assert_seeded(&repo, &gateway);
        std::fs::create_dir_all(repo.data.join("projects").join(PROJECT).join(".git"))
            .expect("git dir");
        let calls = gateway.call_log().len();
        let err = prepare_err(&repo, &gateway).await;
        assert_eq!(err.code(), "project_git_managed");
        assert_error_canary_free(&err, "project_git_managed");
        assert_eq!(gateway.call_log().len(), calls, "the gateway was not asked");
        assert_no_run(&repo, &gateway, "project_git_managed");
    }

    // project_parent_invalid: the named parent is not on the gateway.
    {
        let repo = checkout(&replace(
            &members_with_parent("Missing"),
            CODE_PATH,
            CANARY_CHECKOUT,
        ));
        let live = replace(&base_members(), CODE_PATH, CANARY_LIVE);
        let gateway = FakeProjects::new().with_project(PROJECT, None, false, &live);
        assert_seeded(&repo, &gateway);
        let err = prepare_err(&repo, &gateway).await;
        assert_eq!(err.code(), "project_parent_invalid");
        assert_error_canary_free(&err, "project_parent_invalid");
        assert_no_run(&repo, &gateway, "project_parent_invalid");
    }

    // destructive_change_refused, downgrade: the record is newer than the tag.
    {
        let (repo, gateway) = canary_import(Some("v2.0.0")).await;
        assert_seeded(&repo, &gateway);
        let err = prepare_err(&repo, &gateway).await;
        match &err {
            CoreError::DestructiveChangeRefused { gate, .. } => assert_eq!(gate, "downgrade"),
            other => panic!("expected destructive_change_refused, got {other:?}"),
        }
        assert_error_canary_free(&err, "downgrade");
        assert_no_run(&repo, &gateway, "downgrade");
    }

    // destructive_change_refused, first deploy: no record, the live project
    // differs from the artifact.
    {
        let (repo, gateway) = canary_import(None).await;
        assert_seeded(&repo, &gateway);
        let err = prepare_err(&repo, &gateway).await;
        match &err {
            CoreError::DestructiveChangeRefused { gate, .. } => assert_eq!(gate, "first_deploy"),
            other => panic!("expected destructive_change_refused, got {other:?}"),
        }
        assert_error_canary_free(&err, "first_deploy");
        assert_no_run(&repo, &gateway, "first_deploy");
    }

    // deploy_preflight_failed, designers: a session is open.
    {
        let (repo, gateway) = canary_import(Some("v0.9.0")).await;
        let gateway = gateway.with_designers_script(vec![vec![session("alice")]]);
        assert_seeded(&repo, &gateway);
        let (result, recorder) =
            run_heard(&repo, &gateway, &repo.input(), DesignersPolicy::default()).await;
        let err = result.expect_err("an open Designer refuses the import");
        assert_eq!(preflight_failure(&err).0, "designers");
        assert_error_canary_free(&err, "designers");
        recorder.assert_canary_free("designers");
        assert_no_run(&repo, &gateway, "designers");
    }

    // deploy_preflight_failed, wrong target: the gateway is not the one the
    // target names.
    {
        let (repo, gateway) = canary_import(Some("v0.9.0")).await;
        repo.declare_gateway_name("gw-a");
        repo.set_host_system_name("gw-b");
        let gateway = gateway.with_gateway_name("gw-b");
        assert_seeded(&repo, &gateway);
        let (result, recorder) =
            run_heard(&repo, &gateway, &repo.input(), DesignersPolicy::default()).await;
        let err = result.expect_err("a wrong target refuses the import");
        assert_eq!(preflight_failure(&err).0, "wrong_target");
        assert_error_canary_free(&err, "wrong_target");
        recorder.assert_canary_free("wrong_target");
        assert_no_run(&repo, &gateway, "wrong_target");
    }
}

/// DPRJ-03, SC-5: a run that stops after the pre-image was saved (`plan_stale`
/// after the live project moved, `import_denied` from the gateway) leaves a
/// run directory whose audit files, events and error say nothing of the
/// project; only `export/CapApp.zip` holds it, and it does.
#[tokio::test]
async fn canary_aborted_runs_clean() {
    // plan_stale: another administrator edits the live project between the
    // plan and the run (K15).
    {
        let (repo, gateway) = canary_import(Some("v0.9.0")).await;
        assert_seeded(&repo, &gateway);
        let input = repo.input();
        let (loaded, prepared) = plan(&repo, &gateway, &input).await;
        gateway.change_live(PROJECT, CODE_PATH, CANARY_MOVED);
        let recorder = Recorder::default();
        let err = run_observed(
            &repo,
            &gateway,
            &input,
            &loaded,
            prepared,
            None,
            DesignersPolicy::default(),
            &recorder,
        )
        .await
        .expect_err("a moved live project is stale");
        assert_eq!(err.code(), "plan_stale");
        assert_error_canary_free(&err, "plan_stale");
        recorder.assert_heard("plan_stale");
        recorder.assert_canary_free("plan_stale");
        assert!(gateway.import_calls().is_empty(), "nothing was imported");

        let dirs = assert_run_files_canary_free(&repo, "plan_stale");
        assert_eq!(dirs.len(), 1, "plan_stale");
        assert_exempt_pre_image(&dirs[0], "plan_stale");
        let result: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dirs[0].join("result.json")).expect("result.json"),
        )
        .expect("result.json is json");
        assert_eq!(result["slug"], "plan_stale");
        assert_eq!(result["ok"], false);
    }

    // import_denied: the gateway refuses the import after the pre-image was
    // saved and the import was sent.
    {
        let (repo, gateway) = canary_import(Some("v0.9.0")).await;
        assert_seeded(&repo, &gateway);
        let denying = DenyImport(&gateway);
        let (result, recorder) =
            run_heard(&repo, &denying, &repo.input(), DesignersPolicy::default()).await;
        let err = result.expect_err("a denied import fails the run");
        assert_eq!(err.code(), "import_denied");
        assert_error_canary_free(&err, "import_denied");
        recorder.assert_heard("import_denied");
        recorder.assert_canary_free("import_denied");

        let dirs = assert_run_files_canary_free(&repo, "import_denied");
        assert_eq!(dirs.len(), 1, "import_denied");
        assert_exempt_pre_image(&dirs[0], "import_denied");
        let events = std::fs::read_to_string(dirs[0].join("events.ndjson")).expect("events");
        assert!(events.contains("import_sent"), "the import was sent");
        let result: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dirs[0].join("result.json")).expect("result.json"),
        )
        .expect("result.json is json");
        assert_eq!(result["slug"], "import_denied");
        assert_eq!(
            read_record(&repo).release,
            "v0.9.0",
            "no record for a denied import"
        );
    }
}

/// The members of an inheriting project whose script holds the canary.
fn canary_child_members() -> Vec<(&'static str, Vec<u8>)> {
    replace(&child_members(), CHILD_CODE_PATH, CANARY_CHECKOUT)
}

/// DPRJ-03, D-06: the children sweep reads other projects' exports. Their
/// content never reaches the report, the warnings, the events or a run file:
/// a child that no longer matches its record, one with no record and one
/// whose export cannot be read each report names, statuses and hashes only.
#[tokio::test]
async fn canary_children_clean() {
    let (repo, gateway) = canary_import(Some("v0.9.0")).await;
    let children = canary_child_members();
    let gateway = gateway
        .with_child("ChildA", PROJECT, &children)
        .with_child("ChildB", PROJECT, &children)
        .with_child("ChildC", PROJECT, &children);
    gateway.fail_export_of("ChildC");
    // ChildA: a record that names other content (a mismatch).
    put_child_record(&repo, "ChildA", "v0.4.0", &"0".repeat(64));
    assert_seeded(&repo, &gateway);
    for child in ["ChildA", "ChildB", "ChildC"] {
        assert!(
            holds_canary(
                &gateway
                    .stored_member(child, CHILD_CODE_PATH)
                    .expect("the child's script")
            ),
            "{child} holds the canary"
        );
    }

    let (result, recorder) =
        run_heard(&repo, &gateway, &repo.input(), DesignersPolicy::default()).await;
    let result = result.expect("the parent import completes");
    assert_eq!(result.outcome, ProjectOutcome::Imported);

    let status = |name: &str| {
        result
            .children
            .iter()
            .find(|child| child.name == name)
            .map(|child| child.status)
    };
    assert_eq!(status("ChildA"), Some(ChildStatus::Mismatch));
    assert_eq!(status("ChildB"), Some(ChildStatus::Unverified));
    assert_eq!(status("ChildC"), Some(ChildStatus::Unverified));
    assert!(
        !warnings_naming(&result, "ChildA").is_empty()
            && !warnings_naming(&result, "ChildC").is_empty(),
        "the sweep warned: {:?}",
        result.warnings
    );

    let serialized = serde_json::to_string(&result).expect("the result serializes");
    assert!(!serialized.contains(CANARY), "canary in the result");
    assert!(
        !format!("{:?}", result.children).contains(CANARY),
        "canary in the children report"
    );
    for warning in &result.warnings {
        assert!(!warning.contains(CANARY), "canary in warning {warning}");
    }
    recorder.assert_heard("children");
    recorder.assert_canary_free("children");
    let dirs = assert_run_files_canary_free(&repo, "children");
    assert_eq!(dirs.len(), 1);
    assert_exempt_pre_image(&dirs[0], "children");
}
