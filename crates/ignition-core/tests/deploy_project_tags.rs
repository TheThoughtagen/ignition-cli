//! Engine suite for project-owned tags (Phase 26, 26-03 tracer): a project
//! repo whose `deploy/project.yaml` lists `tag_providers` deploys the tags in
//! `tags/<P>/` and the project in one `ign deploy projects` run, tags first,
//! against `FakeProjects`. No network.
//!
//! The fake copies 26-LIVE-CAPTURES.md: an `Abort` import as C1 recorded, and
//! a created AtomicTag gaining `defaultValue` equal to the sent `value` as C12
//! recorded. Names are neutral (`PlantA`, `Line1`, `CapApp`).

mod common;

use std::path::{Path, PathBuf};

use common::fake_projects::{FakeProjects, ForbidProjectWrites};
use ignition_core::actions::deploy_projects::{
    DeployProjectsResult, LoadedProject, NoProjectsObserver, ProjectGateway, ProjectOutcome,
    ProjectsInput, ProjectsPrepared, ProjectsRunOptions, ProjectsSurfaces, deploy_projects_prepare,
    deploy_projects_run, load_project,
};
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::error::CoreError;
use serde_json::{Value, json};
use std::time::Duration;

const PROJECT: &str = "CapApp";
const PROVIDER: &str = "PlantA";

const PROJECT_JSON: &str = r#"{"title":"Cap App","enabled":true}"#;
const SCRIPT_RESOURCE: &str =
    r#"{"scope":"A","version":1,"files":["code.py"],"attributes":{"enabled":true}}"#;
const CODE_V1: &[u8] = b"def f():\n    return 1\n";
const CODE_V2: &[u8] = b"def f():\n    return 2\n";

/// The new memory tag the tracer deploys: it carries `value` and no
/// `defaultValue`, as the hand-made files of the project export do.
const SPEED: &str = r#"{"dataType":"Int4","tagType":"AtomicTag","value":3,"valueSource":"memory"}"#;

fn members(code: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("project.json", PROJECT_JSON.as_bytes().to_vec()),
        (
            "ignition/script-python/cap/resource.json",
            SCRIPT_RESOURCE.as_bytes().to_vec(),
        ),
        ("ignition/script-python/cap/code.py", code.to_vec()),
    ]
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

struct Checkout {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
    data: PathBuf,
}

impl Checkout {
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

    /// A tag file of the owned provider, at its provider-relative path.
    fn tag(&self, path: &str, json: &str) {
        put(
            &self.repo,
            &format!("tags/{PROVIDER}/{path}.json"),
            json.as_bytes(),
        );
    }

    /// The release record of `release` naming `artifact_sha256`.
    fn record(&self, release: &str, artifact_sha256: &str) {
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
            &self.data,
            "ign-deploy/capgw/projects/CapApp/release.json",
            &record.to_bytes(),
        );
    }
}

/// A project repo holding the base project, `deploy/project.yaml` with
/// `tag_providers` when `owned` is not empty, and a targets root.
fn checkout(owned: &[&str]) -> Checkout {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = scratch.path().join("repo");
    let targets = scratch.path().join("targets-root");
    let data = scratch.path().join("data");
    std::fs::create_dir_all(data.join("projects")).expect("data dir");
    for (rel, bytes) in members(CODE_V1) {
        put(&repo, rel, &bytes);
    }
    let tag_line = if owned.is_empty() {
        String::new()
    } else {
        format!("tag_providers: [{}]\n", owned.join(", "))
    };
    put(
        &repo,
        "deploy/project.yaml",
        format!("schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\n{tag_line}")
            .as_bytes(),
    );
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

/// The owned provider's live tree: the structural `_types_` folder and an
/// empty `Line1` folder.
fn live_tree() -> Value {
    json!({"name":"","tagType":"Provider","tags":[
        {"name":"_types_","tagType":"Folder"},
        {"name":"Line1","tagType":"Folder","tags":[]}
    ]})
}

/// A gateway whose `CapApp` is the repo's project with older code, recorded
/// at v0.9.0, so the plan imports it; it owns `PlantA` as `live_tree`.
async fn importing(repo: &Checkout, gateway: FakeProjects) -> FakeProjects {
    let gateway = gateway
        .with_project(PROJECT, None, false, &members(CODE_V2))
        .with_tag_tree(PROVIDER, live_tree());
    let live = gateway.export_bytes(PROJECT).await.expect("export");
    let digest =
        ignition_core::deploy::project_artifact::canonical_digest(&live).expect("a project export");
    repo.record("v0.9.0", &digest.sha256);
    gateway
}

async fn plan(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
) -> Result<(LoadedProject, ProjectsPrepared), CoreError> {
    let input = repo.input();
    let loaded = load_project(&input)?;
    let host = repo.host();
    let prepared = deploy_projects_prepare(
        &loaded,
        &input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
    )
    .await?;
    Ok((loaded, prepared))
}

async fn check(repo: &Checkout, gateway: &dyn ProjectGateway) -> DeployProjectsResult {
    plan(repo, gateway)
        .await
        .expect("the check completes")
        .1
        .into_check_result()
}

async fn run(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
    loaded: &LoadedProject,
    prepared: ProjectsPrepared,
) -> Result<DeployProjectsResult, CoreError> {
    let input = repo.input();
    let host = repo.host();
    deploy_projects_run(
        prepared,
        loaded,
        &input,
        &ProjectsSurfaces {
            gateway,
            host: &host,
        },
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: None,
            designers: DesignersPolicy::default(),
            children_settle: Duration::from_millis(50),
            children_poll: Duration::from_millis(5),
        },
        &NoProjectsObserver,
    )
    .await
}

async fn apply(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
) -> Result<DeployProjectsResult, CoreError> {
    let (loaded, prepared) = plan(repo, gateway).await.expect("the plan completes");
    run(repo, gateway, &loaded, prepared).await
}

/// The provider's live node at `path`, from a fresh export.
async fn live_node(gateway: &FakeProjects, path: &str) -> Option<Value> {
    let bytes = gateway.tag_export(PROVIDER).await.expect("tag export");
    ignition_core::deploy::tag_canon::flatten_export(&bytes, PROVIDER)
        .expect("a tag tree")
        .remove(path)
}

fn event_names(run_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(run_dir.join("events.ndjson"))
        .expect("events")
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).expect("an event line")["event"]
                .as_str()
                .expect("an event name")
                .to_string()
        })
        .collect()
}

fn position(log: &[String], wanted: &str) -> usize {
    log.iter()
        .position(|call| call == wanted)
        .unwrap_or_else(|| panic!("{wanted} is not in {log:?}"))
}

/// The tracer: one new owned tag and the project, one run, tags first.
#[tokio::test]
async fn tracer_one_new_tag() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;

    // The check reports the tag, reading only.
    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.added, ["[PlantA]Line1/Speed"]);
    assert_eq!(tags.added_count, 1);
    assert_eq!(tags.changed_count, 0);
    assert_eq!(tags.drift_count, 0);
    assert_eq!(tags.providers.len(), 1);
    assert_eq!(
        (tags.providers[0].added, tags.providers[0].same),
        (1, 1),
        "Line1 is the same folder on both sides"
    );
    assert_eq!(tags.tags_sha256.len(), 64);
    assert_eq!(tags.live_sha256.len(), 64);
    assert!(checked.would_change);
    assert_eq!(checked.action, ProjectAction::Import);
    assert!(gateway.tag_import_calls().is_empty());

    let before = gateway.call_log().len();
    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    assert!(result.ok && result.record_written, "{result:?}");
    assert_eq!(
        gateway.tag_import_calls(),
        [(
            PROVIDER.to_string(),
            "Line1".to_string(),
            "Abort".to_string()
        )],
        "one Abort import at the deepest existing parent"
    );

    // Every tag import before the project import, and the tag verify read
    // between them.
    let log = gateway.call_log().split_off(before);
    let tag_import = position(&log, "tag-import:PlantA:Line1:Abort");
    let project_import = position(&log, "import:CapApp:true");
    assert!(tag_import < project_import, "{log:?}");
    let verify_read = log[tag_import..]
        .iter()
        .position(|c| c == "tag-export:PlantA")
        .map(|at| at + tag_import)
        .expect("a verify export after the tag import");
    assert!(verify_read < project_import, "{log:?}");

    // The tag is on the gateway, with the defaultValue C12 recorded.
    let node = live_node(&gateway, "Line1/Speed")
        .await
        .expect("the tag is live");
    assert_eq!(node["defaultValue"], 3);
    assert_eq!(node["dataType"], "Int4");

    // The pre-image is private and holds the export as the run read it.
    let run_dir = repo
        .repo
        .join(result.run_dir.as_deref().expect("a run dir"));
    let pre = run_dir.join("export/tags/PlantA.json");
    let saved: Value =
        serde_json::from_slice(&std::fs::read(&pre).expect("the tag pre-image")).expect("json");
    assert_eq!(saved, live_tree());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode(&pre), 0o600);
        assert_eq!(mode(&run_dir.join("export/tags")), 0o700);
    }
    let names = event_names(&run_dir);
    let expected = [
        "run_started",
        "pre_image_exported",
        "tags_pre_image_exported",
        "tag_import_sent",
        "tag_import_answered",
        "tags_verified",
        "import_sent",
        "import_answered",
        "verified",
        "record_written",
        "finished",
    ];
    assert_eq!(names, expected);
    let plan_json: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("plan.json")).expect("plan.json"))
            .expect("json");
    assert_eq!(plan_json["tags"]["added"], json!(["[PlantA]Line1/Speed"]));
}

/// A project that is already this release, with one new owned tag: the tag
/// is deployed and the project is left alone.
#[tokio::test]
async fn tags_only_change_leaves_the_project_alone() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V1))
        .with_tag_tree(PROVIDER, live_tree());
    let sha = load_project(&repo.input())
        .expect("loads")
        .artifact
        .digest
        .sha256;
    repo.record("v1.0.0", &sha);

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.action, ProjectAction::Skip);
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    assert!(result.ok);
    assert!(
        gateway.import_calls().is_empty(),
        "the project is not imported"
    );
    assert_eq!(gateway.tag_import_calls().len(), 1);

    // Run again: nothing left to do.
    let again = apply(&repo, &gateway).await.expect("the second run");
    assert_eq!(again.outcome, ProjectOutcome::Skipped);
    assert!(again.run_id.is_none());
    assert_eq!(gateway.tag_import_calls().len(), 1);
}

/// R3-DV seed check: a gateway-added defaultValue that is not the repo's
/// value is a verify mismatch naming the node and the property, and the
/// project is never imported.
#[tokio::test]
async fn created_default_value_seed_checked() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(
        &repo,
        FakeProjects::new().with_created_default_value(json!("seed-sentinel")),
    )
    .await;

    let err = apply(&repo, &gateway).await.expect_err("a red tag verify");
    assert_eq!(err.code(), "rollback_failed");
    let text = err.to_string();
    assert!(
        text.contains("[PlantA]Line1/Speed (defaultValue)"),
        "{text}"
    );
    assert!(!text.contains("seed-sentinel"), "never a value: {text}");
    assert!(
        gateway.import_calls().is_empty(),
        "the project is untouched"
    );
}

/// R3-VK: a created tag whose value changed at runtime before the verify is
/// still equal: values are never compared.
#[tokio::test]
async fn created_value_not_compared() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new().with_created_value(json!(99))).await;

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    let node = live_node(&gateway, "Line1/Speed").await.expect("live");
    assert_eq!(node["value"], 99, "the stand-in runtime write is there");
}

/// `--check` makes no tag import and no project import and creates no
/// `.ign/` directory.
#[tokio::test]
async fn check_writes_nothing() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = ForbidProjectWrites(importing(&repo, FakeProjects::new()).await);

    let checked = check(&repo, &gateway).await;
    assert_eq!(checked.outcome, ProjectOutcome::Checked);
    assert_eq!(checked.tags.as_ref().expect("data.tags").added_count, 1);
    assert!(gateway.0.tag_import_calls().is_empty());
    assert!(gateway.0.import_calls().is_empty());
    assert!(!repo.repo.join(".ign").exists());
    assert_eq!(
        gateway.0.tag_export_calls(),
        [PROVIDER],
        "the owned provider is exported once, for the plan and the missing-tag check"
    );
}

/// A project without `tag_providers` has `data.tags: null` and the outcomes
/// it always had, and no tag read is made for it.
#[tokio::test]
async fn no_tag_providers_unchanged() {
    let repo = checkout(&[]);
    let gateway = importing(&repo, FakeProjects::new()).await;

    let checked = check(&repo, &gateway).await;
    let data = serde_json::to_value(&checked).expect("json");
    assert!(data.get("tags").is_some_and(Value::is_null), "{data}");
    assert_eq!(data["outcome"], "checked");

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Imported);
    assert!(result.tags.is_none());
    assert!(gateway.tag_export_calls().is_empty());
    assert!(gateway.tag_import_calls().is_empty());
}

/// An owned provider with no `tags/<P>/` refuses, naming it; it is never an
/// empty provider.
#[tokio::test]
async fn missing_provider_dir() {
    let repo = checkout(&[PROVIDER]);
    let err = load_project(&repo.input()).expect_err("no tags/PlantA");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(err.to_string().contains("tags/PlantA"), "{err}");
}

/// An instance whose only difference from live is a `parameters` value is
/// changed: runtime value keys are stripped from AtomicTag objects only. A
/// run refuses the change before anything is written (26-08 adds the batches).
#[tokio::test]
async fn instance_parameters_are_config() {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "Line1/Pump",
        r#"{"parameters":{"P1":{"dataType":"Integer","value":7}},"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag","value":1}],"typeId":"Ty"}"#,
    );
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V1))
        .with_tag_tree(
            PROVIDER,
            json!({"name":"","tagType":"Provider","tags":[
                {"name":"_types_","tagType":"Folder"},
                {"name":"Line1","tagType":"Folder","tags":[
                    {"name":"Pump","tagType":"UdtInstance","typeId":"Ty",
                     "parameters":{"P1":{"dataType":"Integer","value":8}},
                     "tags":[{"name":"M1","tagType":"AtomicTag","value":5}]}
                ]}
            ]}),
        );
    let sha = load_project(&repo.input())
        .expect("loads")
        .artifact
        .digest
        .sha256;
    repo.record("v1.0.0", &sha);

    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.changed_count, 1);
    assert_eq!(tags.changed[0].path, "[PlantA]Line1/Pump");
    assert_eq!(tags.changed[0].properties, ["parameters"]);
    assert_eq!(tags.added_count, 0);

    let err = apply(&repo, &gateway)
        .await
        .expect_err("changed is refused");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(gateway.tag_import_calls().is_empty());
    assert!(
        !repo.repo.join(".ign").exists(),
        "refused before any run directory"
    );
}

/// A member value that differs only at runtime is not a change (R3-VK), and
/// a live-only tag is drift, kept (D-01).
#[tokio::test]
async fn member_values_are_runtime_and_live_only_tags_are_drift() {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "Line1/Pump",
        r#"{"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag","value":1}],"typeId":"Ty"}"#,
    );
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V1))
        .with_tag_tree(
            PROVIDER,
            json!({"name":"","tagType":"Provider","tags":[
                {"name":"_types_","tagType":"Folder"},
                {"name":"Line1","tagType":"Folder","tags":[
                    {"name":"Pump","tagType":"UdtInstance","typeId":"Ty",
                     "tags":[{"name":"M1","tagType":"AtomicTag","value":5}]},
                    {"name":"Outbox","tagType":"AtomicTag","value":2}
                ]}
            ]}),
        );
    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.changed_count, 0, "{tags:?}");
    assert_eq!(tags.drift, ["[PlantA]Line1/Outbox"]);
    assert_eq!(tags.providers[0].same, 2);
}

/// A provider the target excludes (`System` always) cannot be owned, and a
/// provider the gateway does not list is refused before it is planned.
#[tokio::test]
async fn excluded_or_absent_owned_providers_are_refused() {
    let repo = checkout(&["System"]);
    put(
        &repo.repo,
        "tags/System/A.json",
        br#"{"tagType":"AtomicTag"}"#,
    );
    let err = load_project(&repo.input()).expect_err("System is excluded");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(err.to_string().contains("tag_providers[0]"), "{err}");

    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V1))
        .with_tag_tree("planta", live_tree());
    let err = plan(&repo, &gateway)
        .await
        .expect_err("not listed under that name");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(err.to_string().contains("letter case"), "{err}");
    assert!(gateway.tag_export_calls().is_empty());
}
