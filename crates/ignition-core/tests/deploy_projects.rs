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

use common::fake_projects::FakeProjects;
use ignition_core::actions::deploy_projects::{
    DeployProjectsResult, ProjectsInput, ProjectsSurfaces, deploy_projects_prepare, load_project,
};
use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::error::CoreError;

const PROJECT: &str = "CapApp";

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
