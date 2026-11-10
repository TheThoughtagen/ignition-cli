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
use std::sync::Mutex;

use common::fake_projects::{FakeProjects, ForbidProjectWrites};
use ignition_core::actions::deploy_projects::{
    DeployProjectsResult, LoadedProject, NoProjectsObserver, ProjectGateway, ProjectOutcome,
    ProjectsInput, ProjectsPrepared, ProjectsRunOptions, ProjectsSurfaces, deploy_projects_prepare,
    deploy_projects_run, load_project,
};
use ignition_core::client::DeployApi;
use ignition_core::deploy::apply::host::ApplyHost;
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::pair::BackupPolicy;
use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};
use ignition_core::deploy::tag_converge::BackupTags;
use ignition_core::deploy::tag_record::TagRecord;
use ignition_core::deploy::transport::TreeReader;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::{GitEntry, ProjectHost};
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

/// An owned provider's settings file (`deploy/tag-providers/<P>.json`).
const SETTINGS: &str = r#"{"schema_version": 1, "enabled": true, "config": {}}"#;

/// The gateway-config manifest path of the suite's gateway, below the
/// targets root.
const MANIFEST_REL: &str = "gateways/capgw/manifest.yaml";

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
            tags_sha256: None,
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

    /// The gateway-config manifest at `gateways/capgw/manifest.yaml` below
    /// the targets root.
    fn manifest(&self, yaml: &str) {
        put(&self.targets, MANIFEST_REL, yaml.as_bytes());
    }

    /// Append `extra` to `targets/dev.yaml`.
    fn target_extra(&self, extra: &str) {
        let path = self.targets.join("targets/dev.yaml");
        let mut yaml = std::fs::read_to_string(&path).expect("target");
        yaml.push_str(extra);
        std::fs::write(path, yaml).expect("write target");
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

    /// `project`'s tag record of the owned provider listing `nodes`, from a
    /// previous deploy (26-06), on the master node's host.
    fn tag_record(&self, project: &str, nodes: &[&str]) {
        let mut nodes: Vec<String> = nodes.iter().map(|n| (*n).to_string()).collect();
        nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let record = TagRecord {
            schema_version: 1,
            gateway: "capgw".to_string(),
            provider: PROVIDER.to_string(),
            project: project.to_string(),
            release: "v0.9.0".to_string(),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
            tags_sha256: "ab".repeat(32),
            nodes,
            groups: Vec::new(),
        };
        put(&self.data, TAG_RECORD_REL, &record.to_bytes());
    }
}

/// The owned provider's tag record below the master node's data directory.
const TAG_RECORD_REL: &str = "ign-deploy/capgw/tag-providers/PlantA/record.json";

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
    // Each owned provider's settings (26-05, D-09).
    for provider in owned {
        put(
            &repo,
            &format!("deploy/tag-providers/{provider}.json"),
            SETTINGS.as_bytes(),
        );
    }
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
/// at v0.9.0, so the plan imports it; it owns `PlantA` as `live_tree`, and an
/// earlier deploy of this project recorded the provider's `Line1` folder.
async fn importing(repo: &Checkout, gateway: FakeProjects) -> FakeProjects {
    let gateway = adopting(repo, gateway).await;
    repo.tag_record(PROJECT, &["Line1"]);
    gateway
}

/// [`importing`] without the tag record: the first `ign` deploy of the
/// provider (adoption).
async fn adopting(repo: &Checkout, gateway: FakeProjects) -> FakeProjects {
    let gateway = gateway
        .with_project(PROJECT, None, false, &members(CODE_V2))
        .with_tag_tree(PROVIDER, live_tree());
    let live = gateway.export_bytes(PROJECT).await.expect("export");
    let digest =
        ignition_core::deploy::project_artifact::canonical_digest(&live).expect("a project export");
    repo.record("v0.9.0", &digest.sha256);
    gateway
}

/// The optional seams of [`ProjectsSurfaces`] a tag-group run needs (26-10):
/// the master's tree reader, its apply host and its deploy API.
#[derive(Default, Clone, Copy)]
struct Seams<'a> {
    tree: Option<&'a dyn TreeReader>,
    apply_host: Option<&'a dyn ApplyHost>,
    deploy: Option<&'a dyn DeployApi>,
}

fn surfaces<'a>(
    gateway: &'a dyn ProjectGateway,
    host: &'a dyn ProjectHost,
    seams: Seams<'a>,
) -> ProjectsSurfaces<'a> {
    ProjectsSurfaces {
        gateway,
        host,
        tree: seams.tree,
        apply_host: seams.apply_host,
        deploy: seams.deploy,
    }
}

async fn plan(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
) -> Result<(LoadedProject, ProjectsPrepared), CoreError> {
    plan_with(repo, &repo.input(), gateway, Seams::default()).await
}

async fn plan_with(
    repo: &Checkout,
    input: &ProjectsInput,
    gateway: &dyn ProjectGateway,
    seams: Seams<'_>,
) -> Result<(LoadedProject, ProjectsPrepared), CoreError> {
    let loaded = load_project(input)?;
    let host = repo.host();
    let prepared =
        deploy_projects_prepare(&loaded, input, &surfaces(gateway, &host, seams)).await?;
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
    run_with(
        repo,
        &repo.input(),
        gateway,
        Seams::default(),
        loaded,
        prepared,
    )
    .await
}

async fn run_with(
    repo: &Checkout,
    input: &ProjectsInput,
    gateway: &dyn ProjectGateway,
    seams: Seams<'_>,
    loaded: &LoadedProject,
    prepared: ProjectsPrepared,
) -> Result<DeployProjectsResult, CoreError> {
    run_paired(repo, input, gateway, seams, loaded, prepared, None).await
}

/// [`run_with`] with a backup node read through `backup` (26-13), waited
/// for with [`fast_backup`].
async fn run_paired(
    repo: &Checkout,
    input: &ProjectsInput,
    gateway: &dyn ProjectGateway,
    seams: Seams<'_>,
    loaded: &LoadedProject,
    prepared: ProjectsPrepared,
    backup: Option<(&dyn BackupTags, BackupPolicy)>,
) -> Result<DeployProjectsResult, CoreError> {
    let host = repo.host();
    deploy_projects_run(
        prepared,
        loaded,
        input,
        &surfaces(gateway, &host, seams),
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: None,
            tag_record_writer: None,
            run_writer: None,
            designers: DesignersPolicy::default(),
            children_settle: Duration::from_millis(50),
            children_poll: Duration::from_millis(5),
            tag_verify_settle: Duration::from_millis(500),
            tag_verify_poll: Duration::from_millis(5),
            backup: backup.map(|(node, _)| node),
            backup_policy: backup.map(|(_, policy)| policy).unwrap_or_default(),
        },
        &NoProjectsObserver,
    )
    .await
}

async fn apply_with(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
    seams: Seams<'_>,
) -> Result<DeployProjectsResult, CoreError> {
    let input = repo.input();
    let (loaded, prepared) = plan_with(repo, &input, gateway, seams)
        .await
        .expect("the plan completes");
    run_with(repo, &input, gateway, seams, &loaded, prepared).await
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
        // 26-12: the host copy before the first gateway write, cleared last
        "host_pre_image_written",
        "journal_written",
        "tag_import_sent",
        "tag_import_answered",
        "tags_verified",
        "import_sent",
        "import_answered",
        "verified",
        "record_written",
        "tag_record_written",
        "journal_cleared",
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
    repo.tag_record(PROJECT, &["Line1"]);

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
/// changed: runtime value keys are stripped from AtomicTag objects only. The
/// run applies it as a stripped `MergeOverwrite` (26-08, R3-POL): the new
/// parameter lands and the member keeps its live value (C7 I3).
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
    repo.tag_record(PROJECT, &["Line1", "Line1/Pump"]);

    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.changed_count, 1);
    assert_eq!(tags.changed[0].path, "[PlantA]Line1/Pump");
    assert_eq!(tags.changed[0].properties, ["parameters.P1.value"]);
    assert_eq!(tags.added_count, 0);

    let result = apply(&repo, &gateway).await.expect("the change applies");
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    assert_eq!(
        gateway.tag_import_calls(),
        [(
            PROVIDER.to_string(),
            "Line1".to_string(),
            "MergeOverwrite".to_string()
        )]
    );
    let pump = live_node(&gateway, "Line1/Pump").await.expect("live");
    assert_eq!(pump["parameters"]["P1"]["value"], 7);
    assert_eq!(
        pump["tags"][0]["value"], 5,
        "the member keeps its live value"
    );
}

/// A member value that differs only at runtime is not a change (R3-VK): the
/// node is `kept_values` (D-03); a live-only tag is drift, kept (D-01).
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
    assert_eq!(tags.kept_values, ["[PlantA]Line1/Pump"]);
    assert_eq!(tags.providers[0].same, 1);
    assert_eq!(tags.providers[0].kept_values, 1);
    assert!(
        tags.providers[0].record_only,
        "no record and nothing to add or change"
    );
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

// ---------------------------------------------------------------------------
// 26-05: provider ownership (D-04, D-05, D-06, D-09).
// ---------------------------------------------------------------------------

/// A [`ProjectHost`] over the checkout's data directory that counts its
/// calls.
struct RecordingHost {
    inner: LocalFsTransport,
    calls: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl ProjectHost for RecordingHost {
    async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError> {
        self.calls.lock().expect("calls").push("git_entry".into());
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
            .push("read_release_record".into());
        self.inner.read_release_record(gateway, project).await
    }

    async fn read_tag_record(
        &self,
        gateway: &str,
        provider: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .push("read_tag_record".into());
        self.inner.read_tag_record(gateway, provider).await
    }

    async fn read_journal(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .push("read_journal".into());
        ProjectHost::read_journal(&self.inner, gateway, project).await
    }

    async fn read_pre_image(
        &self,
        gateway: &str,
        project: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .push("read_pre_image".into());
        self.inner.read_pre_image(gateway, project, name).await
    }
}

/// Plan `repo` against a gateway holding the project and the owned provider,
/// expecting a refusal; asserts that neither the gateway nor the host was
/// called, and that no run directory exists.
async fn refused_before_contact(repo: &Checkout) -> CoreError {
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V2))
        .with_tag_tree(PROVIDER, live_tree());
    let host = RecordingHost {
        inner: repo.host(),
        calls: Mutex::new(Vec::new()),
    };
    let input = repo.input();
    let err = match load_project(&input) {
        Err(err) => err,
        Ok(loaded) => match deploy_projects_prepare(
            &loaded,
            &input,
            &surfaces(&gateway, &host, Seams::default()),
        )
        .await
        {
            Err(err) => err,
            Ok(_) => panic!("refused before contact"),
        },
    };
    assert!(gateway.call_log().is_empty(), "{:?}", gateway.call_log());
    assert_eq!(gateway.tag_list_count(), 0);
    assert!(gateway.tag_export_calls().is_empty());
    assert!(host.calls.lock().expect("calls").is_empty(), "no host call");
    assert!(!repo.repo.join(".ign").exists());
    err
}

/// D-05: a provider the gateway-config manifest still lists in
/// `tag_providers` (in any letter case) is refused `tag_provider_claimed`,
/// exit 6, before any gateway or host call.
#[tokio::test]
async fn manifest_claim_refused_before_any_contact() {
    for listed in [PROVIDER, "planta"] {
        let repo = checkout(&[PROVIDER]);
        repo.tag("Line1/Speed", SPEED);
        repo.manifest(&format!(
            "schema_version: 1\ntypes: []\ntag_providers: [default, {listed}]\n"
        ));
        let err = refused_before_contact(&repo).await;
        assert_eq!(err.code(), "tag_provider_claimed", "{err}");
        assert_eq!(err.exit_code(), 6);
        let text = err.to_string();
        assert!(text.contains(PROVIDER), "{text}");
        assert!(text.contains(MANIFEST_REL), "{text}");
    }
}

/// D-06: a provider on the target's `read_only_tag_providers` cannot be
/// owned; refused before any call.
#[tokio::test]
async fn read_only_on_target_refused_before_any_contact() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    repo.target_extra("read_only_tag_providers: [planta]\n");
    let err = refused_before_contact(&repo).await;
    assert_eq!(err.code(), "deploy_tree_invalid");
    let text = err.to_string();
    assert!(text.contains("deploy/project.yaml"), "{text}");
    assert!(
        text.contains("read_only_tag_providers of targets/dev.yaml"),
        "{text}"
    );
}

/// D-06 from the manifest's list, and a manifest that is present but
/// invalid is its own `deploy_tree_invalid`, both before any call.
#[tokio::test]
async fn manifest_read_only_and_invalid_manifest_refused_before_any_contact() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    repo.manifest("schema_version: 1\ntypes: []\nread_only_tag_providers: [PlantA]\n");
    let err = refused_before_contact(&repo).await;
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(
        err.to_string()
            .contains(&format!("read_only_tag_providers of {MANIFEST_REL}")),
        "{err}"
    );

    repo.manifest("schema_version: 1\ntypes: []\nunknown_key: 1\n");
    let err = refused_before_contact(&repo).await;
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(err.to_string().contains(MANIFEST_REL), "{err}");
}

/// D-09: every owned provider has its settings file; a missing one refuses
/// before any call, naming it.
#[tokio::test]
async fn missing_provider_settings_refused_before_any_contact() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    std::fs::remove_file(repo.repo.join("deploy/tag-providers/PlantA.json")).expect("rm");
    let err = refused_before_contact(&repo).await;
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(
        err.to_string().contains("deploy/tag-providers/PlantA.json"),
        "{err}"
    );
}

/// The tag plan's exclusion set is manifest-aware for a tag-owning project:
/// the manifest's exclude and read-only lists join the target's.
#[tokio::test]
async fn tag_exclusions_include_the_manifest_lists() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    repo.target_extra("read_only_tag_providers: [\"Broker Transmission\"]\n");
    repo.manifest(
        "schema_version: 1\ntypes: []\nexclude_tag_providers: [Sparkplug]\nread_only_tag_providers: [EdgeGateway01]\n",
    );
    let loaded = load_project(&repo.input()).expect("loads");
    assert_eq!(
        loaded.tag_exclusions,
        [
            "Broker Transmission",
            "EdgeGateway01",
            "Sparkplug",
            "System"
        ]
    );
    assert!(loaded.ownership_warnings.is_empty());

    // The run still plans and reports the owned provider.
    let gateway = importing(&repo, FakeProjects::new()).await;
    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.added_count, 1);
    assert!(tags.warnings.is_empty(), "{:?}", tags.warnings);
}

/// Without a manifest the ownership check passes and says so in
/// `data.tags.warnings`, naming the manifest path it looked for.
#[tokio::test]
async fn no_manifest_is_a_tags_warning() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.warnings.len(), 1, "{:?}", tags.warnings);
    assert!(
        tags.warnings[0].contains(MANIFEST_REL),
        "{:?}",
        tags.warnings
    );
    let data = serde_json::to_value(&checked).expect("json");
    assert_eq!(data["tags"]["warnings"], json!(tags.warnings));
}

/// A project without `tag_providers` never reads the manifest: one that
/// would not even parse is ignored, and the exclusion set is the target's.
#[tokio::test]
async fn no_tag_providers_never_reads_the_manifest() {
    let repo = checkout(&[]);
    repo.manifest("not: [valid\n");
    let loaded = load_project(&repo.input()).expect("the manifest is not read");
    assert_eq!(loaded.tag_exclusions, ["System"]);
    assert!(loaded.ownership_warnings.is_empty());
}

// ---------------------------------------------------------------------------
// 26-05: the reviewed tags hash and the plan-stale re-export (TAGDEP-10).
// ---------------------------------------------------------------------------

/// `--tags-sha256`: the reviewed hash passes; a different one is
/// `project_hash_mismatch` naming `tags_sha256` before any gateway or host
/// call; one given for a project that owns no provider is `invalid_input`.
#[tokio::test]
async fn tags_hash_gate() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    let reviewed = check(&repo, &gateway)
        .await
        .tags
        .expect("data.tags")
        .tags_sha256;
    let mut input = repo.input();
    input.tags_sha256 = Some(reviewed.clone());
    load_project(&input).expect("the reviewed hash passes");

    // The repo's tag content changes after the review.
    repo.tag(
        "Line1/Speed",
        r#"{"dataType":"Int4","tagType":"AtomicTag","value":4,"valueSource":"memory"}"#,
    );
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V2))
        .with_tag_tree(PROVIDER, live_tree());
    let err = load_project(&input).expect_err("a different repo tags hash");
    assert_eq!(err.code(), "project_hash_mismatch");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(text.contains("tags_sha256"), "{text}");
    assert!(text.contains(&reviewed), "{text}");
    assert!(gateway.call_log().is_empty());
    assert!(!repo.repo.join(".ign").exists());

    let repo = checkout(&[]);
    let mut input = repo.input();
    input.tags_sha256 = Some("a".repeat(64));
    let err = load_project(&input).expect_err("nothing owned to compare");
    assert_eq!(err.code(), "invalid_input");
    assert!(err.to_string().contains("--tags-sha256"), "{err}");
}

/// A tags hash that is not 64 lowercase hex characters is `invalid_input`
/// before anything is read: here the repo does not even exist.
#[tokio::test]
async fn tags_hash_format_refused_before_anything_is_read() {
    let repo = checkout(&[PROVIDER]);
    let mut input = repo.input();
    input.repo = repo.repo.join("does-not-exist");
    let upper = "A".repeat(64);
    for bad in [upper.as_str(), "abc", &"g".repeat(64)] {
        input.tags_sha256 = Some(bad.to_string());
        let err = load_project(&input).expect_err(bad);
        assert_eq!(err.code(), "invalid_input", "{bad}: {err}");
        assert!(err.to_string().contains("--tags-sha256"), "{err}");
    }
}

/// The owned provider's live tree with an existing memory tag
/// `Line1/Count` whose `value` and `defaultValue` are given.
fn live_with_count(value: i64, default_value: i64) -> Value {
    json!({"name":"","tagType":"Provider","tags":[
        {"name":"_types_","tagType":"Folder"},
        {"name":"Line1","tagType":"Folder","tags":[
            {"name":"Count","tagType":"AtomicTag","dataType":"Int4",
             "valueSource":"memory","value":value,"defaultValue":default_value}
        ]}
    ]})
}

/// The repo's copy of `Line1/Count`, without `defaultValue` (so the plan
/// absorbs the live one under R3-DV), and the new tag.
fn count_checkout() -> Checkout {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "Line1/Count",
        r#"{"dataType":"Int4","tagType":"AtomicTag","value":1,"valueSource":"memory"}"#,
    );
    repo.tag("Line1/Speed", SPEED);
    repo
}

/// Pitfall 12: a configuration change of an owned provider between the plan
/// and the first write refuses `plan_stale` with no tag import and no
/// project import.
#[tokio::test]
async fn plan_stale_before_first_write() {
    let repo = count_checkout();
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_with_count(1, 1));
    let (loaded, prepared) = plan(&repo, &gateway).await.expect("plans");

    // Someone adds a tag to the provider after the review.
    let mut changed = live_with_count(1, 1);
    changed["tags"][1]["tags"]
        .as_array_mut()
        .expect("Line1 children")
        .push(json!({"name":"Other","tagType":"AtomicTag","dataType":"Int4"}));
    gateway.set_tag_tree(PROVIDER, changed);

    let err = run(&repo, &gateway, &loaded, prepared)
        .await
        .expect_err("stale");
    assert_eq!(err.code(), "plan_stale", "{err}");
    assert_eq!(err.exit_code(), 6);
    assert!(err.to_string().contains("owned tag providers"), "{err}");
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
}

/// R3-VK: a runtime write of an existing tag's `value` between the plan and
/// the run is not staleness.
#[tokio::test]
async fn plan_stale_ignores_runtime_values() {
    let repo = count_checkout();
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_with_count(1, 1));
    let (loaded, prepared) = plan(&repo, &gateway).await.expect("plans");

    gateway.set_tag_tree(PROVIDER, live_with_count(7, 1));
    let result = run(&repo, &gateway, &loaded, prepared)
        .await
        .expect("a value write is not a change");
    assert_eq!(result.outcome, ProjectOutcome::Applied);
    assert_eq!(gateway.tag_import_calls().len(), 1);
}

/// R3-DV (5): `live_sha256` compares one export with another, so the live
/// `defaultValue` is configuration there, even where the repo node has none
/// and the plan diff absorbs it: a change of it between the plan and the run
/// is staleness.
#[tokio::test]
async fn plan_stale_compares_live_default_value() {
    let repo = count_checkout();
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_with_count(1, 1));
    let (loaded, prepared) = plan(&repo, &gateway).await.expect("plans");
    let tags = match &prepared {
        ProjectsPrepared::Ready { plan, .. } => plan.tags.as_ref().expect("data.tags"),
        ProjectsPrepared::Unchanged(_) => panic!("the plan adds Line1/Speed"),
    };
    assert_eq!(tags.changed_count, 0, "R3-DV absorbs the live defaultValue");

    gateway.set_tag_tree(PROVIDER, live_with_count(1, 2));
    let err = run(&repo, &gateway, &loaded, prepared)
        .await
        .expect_err("a defaultValue change is a change");
    assert_eq!(err.code(), "plan_stale", "{err}");
    assert!(err.to_string().contains("owned tag providers"), "{err}");
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
}

// ---------------------------------------------------------------------------
// 26-07: the full plan, the host record, the gates and the secret rule.
// ---------------------------------------------------------------------------

/// The owned provider's live tree: `Line1` holding the given children.
fn live_line1(children: Value) -> Value {
    json!({"name":"","tagType":"Provider","tags":[
        {"name":"_types_","tagType":"Folder"},
        {"name":"Line1","tagType":"Folder","tags":children}
    ]})
}

/// A memory tag as the repo and the gateway hold it.
const KEEP: &str = r#"{"dataType":"Int4","tagType":"AtomicTag","value":1,"valueSource":"memory"}"#;

fn keep_live(name: &str) -> Value {
    json!({"name":name,"dataType":"Int4","tagType":"AtomicTag","value":1,"valueSource":"memory"})
}

/// Every file below `root`, relative, with its bytes.
fn files_below(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("below")
                    .to_string_lossy()
                    .into_owned();
                out.push((rel, std::fs::read(&path).expect("read")));
            }
        }
    }
    out.sort();
    out
}

/// D-02: a node this project's record lists, gone from the repo and live, is
/// planned as `removed` and listed by `--check`; the run refuses it with gate
/// `tag_removal` before anything is contacted or written (R3-DEL).
#[tokio::test]
async fn removal_planned_from_the_record_and_refused() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Keep", KEEP);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_line1(json!([
            keep_live("Keep"),
            keep_live("Old"),
            keep_live("Stray")
        ])),
    );
    repo.tag_record(PROJECT, &["Line1", "Line1/Keep", "Line1/Old", "Line1/Gone"]);

    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.removed, ["[PlantA]Line1/Old"]);
    assert_eq!(tags.removed_count, 1);
    assert_eq!(tags.already_absent, ["[PlantA]Line1/Gone"]);
    assert_eq!(tags.drift, ["[PlantA]Line1/Stray"], "unrecorded: kept");
    let summary = &tags.providers[0];
    assert_eq!((summary.removed, summary.already_absent), (1, 1));
    assert_eq!(
        summary.record_run_id.as_deref(),
        Some("20261020T101500Z-ab12")
    );
    assert_eq!(tags.plan_sha256.len(), 64);
    let data = serde_json::to_value(&checked).expect("json");
    for key in [
        "plan_sha256",
        "recreated",
        "kept_values",
        "removed",
        "removed_count",
        "already_absent",
        "non_canonical",
    ] {
        assert!(data["tags"].get(key).is_some(), "data.tags.{key}");
    }

    let before = gateway.call_log().len();
    let err = apply(&repo, &gateway).await.expect_err("a removal");
    match &err {
        CoreError::DestructiveChangeRefused { gate, keys, count } => {
            assert_eq!(gate, "tag_removal");
            assert_eq!(keys, &["[PlantA]Line1/Old"]);
            assert_eq!(*count, 1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(err.exit_code(), 2);
    assert!(err.to_string().contains("[PlantA]Line1/Old"), "{err}");
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
    assert!(
        gateway.call_log()[before..]
            .iter()
            .all(|c| !c.starts_with("tag-import") && !c.starts_with("import")),
        "{:?}",
        gateway.call_log()
    );
    assert!(!repo.repo.join(".ign").exists(), "no run directory");
}

/// TAGDEP-11: with no record (the first deploy of the provider, adoption)
/// nothing is removed; an added node is a first change, refused without
/// `--allow-first-change`; a provider whose nodes all compare equal plans
/// only the record.
#[tokio::test]
async fn adoption_without_a_record() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Keep", KEEP);
    repo.tag("Line1/Speed", SPEED);
    let gateway = adopting(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_line1(json!([keep_live("Keep"), keep_live("LiveOnly")])),
    );

    let err = plan(&repo, &gateway).await.expect_err("a first change");
    match &err {
        CoreError::DestructiveChangeRefused { gate, keys, .. } => {
            assert_eq!(gate, "first_deploy");
            assert_eq!(keys, &["tag provider PlantA"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(gateway.tag_import_calls().is_empty());

    // With a stated reason the plan shows the adoption: nothing removed, the
    // live-only tag kept.
    let mut input = repo.input();
    input.allow_first_change = Some("adopt the provider".to_string());
    let loaded = load_project(&input).expect("loads");
    let host = repo.host();
    let checked = deploy_projects_prepare(
        &loaded,
        &input,
        &surfaces(&gateway, &host, Seams::default()),
    )
    .await
    .expect("the reason passes the gate")
    .into_check_result();
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.added, ["[PlantA]Line1/Speed"]);
    assert_eq!(tags.removed_count, 0);
    assert_eq!(tags.drift, ["[PlantA]Line1/LiveOnly"]);
    assert!(tags.providers[0].first_change);
    assert!(tags.providers[0].record_run_id.is_none());

    // All equal: only the record, no refusal, nothing removed.
    std::fs::remove_file(repo.repo.join("tags/PlantA/Line1/Speed.json")).expect("rm");
    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert!(tags.providers[0].record_only, "{tags:?}");
    assert!(!tags.providers[0].first_change);
    assert_eq!(
        (tags.added_count, tags.changed_count, tags.removed_count),
        (0, 0, 0)
    );
}

/// D-05 on the host: a tag record another project wrote claims the provider:
/// `tag_provider_claimed` (exit 6), before any gateway call.
#[tokio::test]
async fn another_projects_record_claims_the_provider() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    repo.tag_record("OtherApp", &["Line1"]);
    let before = gateway.call_log().len();

    let err = plan(&repo, &gateway).await.expect_err("claimed");
    assert_eq!(err.code(), "tag_provider_claimed", "{err}");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(text.contains("project OtherApp (host record)"), "{text}");
    assert!(text.contains(PROVIDER), "{text}");
    assert_eq!(gateway.call_log().len(), before, "no gateway call");
    assert!(gateway.tag_export_calls().is_empty());
}

/// A tag record that cannot be trusted refuses `deploy_run_invalid` naming
/// its location; it is never read as absent (which would be an adoption).
#[tokio::test]
async fn a_defective_record_refuses() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    put(&repo.data, TAG_RECORD_REL, b"{\"schema_version\": 1");

    let err = plan(&repo, &gateway).await.expect_err("a defective record");
    assert_eq!(err.code(), "deploy_run_invalid", "{err}");
    assert!(err.to_string().contains(TAG_RECORD_REL), "{err}");
    assert!(gateway.tag_export_calls().is_empty());
}

/// Pitfall 2: a plan that removes every node the record lists is refused
/// `provider_emptied`, with no flag that passes it.
#[tokio::test]
async fn provider_emptied_is_refused() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/New", KEEP);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_line1(json!([keep_live("Old1"), keep_live("Old2")])),
    );
    repo.tag_record(PROJECT, &["Line1/Old1", "Line1/Old2"]);

    let mut input = repo.input();
    input.allow_first_change = Some("not a pass for this gate".to_string());
    let loaded = load_project(&input).expect("loads");
    let host = repo.host();
    let err = deploy_projects_prepare(
        &loaded,
        &input,
        &surfaces(&gateway, &host, Seams::default()),
    )
    .await
    .expect_err("emptied");
    match &err {
        CoreError::DestructiveChangeRefused { gate, keys, .. } => {
            assert_eq!(gate, "provider_emptied");
            assert_eq!(keys, &["tag provider PlantA"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(err.to_string().contains("no flag permits"), "{err}");
    assert!(gateway.tag_import_calls().is_empty());
}

/// An owned provider whose repo directory holds no node refuses
/// `deploy_tree_invalid` offline (Pitfall 2).
#[tokio::test]
async fn a_provider_with_no_node_is_refused() {
    let repo = checkout(&[PROVIDER]);
    std::fs::create_dir_all(repo.repo.join("tags/PlantA")).expect("mkdir");
    let err = load_project(&repo.input()).expect_err("empty provider");
    assert_eq!(err.code(), "deploy_tree_invalid");
    assert!(err.to_string().contains("holds no tag node"), "{err}");
}

/// D-14 with R3-SECRET: a credential-like value on a tag whose value is
/// kept (never sent) is a warning naming the location, and the plan stands.
#[tokio::test]
async fn a_credential_the_run_keeps_is_a_warning() {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "Line1/ServiceToken",
        r#"{"dataType":"String","tagType":"AtomicTag","value":"fake-repo-sentinel","valueSource":"memory"}"#,
    );
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_line1(
            json!([{"name":"ServiceToken","dataType":"String","tagType":"AtomicTag",
            "value":"fake-live-sentinel","valueSource":"memory"}]),
        ),
    );

    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.kept_values, ["[PlantA]Line1/ServiceToken"]);
    let warning = tags
        .warnings
        .iter()
        .find(|w| w.contains("ServiceToken"))
        .unwrap_or_else(|| panic!("{:?}", tags.warnings));
    assert!(
        warning.starts_with("tags/PlantA/Line1/ServiceToken.json: value (credential value)"),
        "{warning}"
    );
    let data = serde_json::to_string(&checked).expect("json");
    assert!(!data.contains("sentinel"), "never a value: {data}");
}

/// D-14 with R3-SECRET: a credential-like `defaultValue` (a Document holding
/// a string) on a tag the run would create refuses `embedded_secret` naming
/// the file and the key, never the value.
#[tokio::test]
async fn a_credential_the_run_would_send_refuses() {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "Line1/ServiceToken",
        r#"{"dataType":"Document","tagType":"AtomicTag","value":{"n":1},"defaultValue":{"jwt":"fake.jwt.sentinel"},"valueSource":"memory"}"#,
    );
    let gateway = importing(&repo, FakeProjects::new()).await;

    let err = plan(&repo, &gateway).await.expect_err("a credential sent");
    assert_eq!(err.code(), "embedded_secret", "{err}");
    let text = err.to_string();
    assert!(
        text.contains("tags/PlantA/Line1/ServiceToken.json: defaultValue (credential value)"),
        "{text}"
    );
    assert!(!text.contains("sentinel"), "{text}");
    assert!(gateway.tag_import_calls().is_empty());
}

/// TAGDEP-12: a repo file the git module would rewrite is reported in
/// `data.tags.non_canonical` with its reason; a file in the module's own form
/// is not.
#[tokio::test]
async fn a_non_canonical_file_is_reported() {
    let repo = checkout(&[PROVIDER]);
    // A memory tag with value and defaultValue: the module's own form.
    let node = json!({"dataType":"Int4","defaultValue":3,"tagType":"AtomicTag","value":3,"valueSource":"memory"});
    let canonical = ignition_core::deploy::tag_canon::canonical_file_bytes(&node);
    put(&repo.repo, "tags/PlantA/Line1/Speed.json", &canonical);
    let mut trailing = canonical.clone();
    trailing.push(b'\n');
    put(&repo.repo, "tags/PlantA/Line1/Rate.json", &trailing);
    let gateway = importing(&repo, FakeProjects::new()).await;

    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.non_canonical_count, 1, "{:?}", tags.non_canonical);
    assert_eq!(tags.non_canonical[0].file, "tags/PlantA/Line1/Rate.json");
    assert_eq!(tags.non_canonical[0].reason, "trailing newline");
    assert_eq!(tags.providers[0].non_canonical, 1);
    let data = serde_json::to_value(&checked).expect("json");
    assert_eq!(
        data["tags"]["non_canonical"],
        json!([{"file":"tags/PlantA/Line1/Rate.json","reason":"trailing newline"}])
    );
}

/// `--check` reads only: with a record and a planned removal it makes no tag
/// import, no project import and no record write, and the host's records are
/// byte-identical afterwards.
#[tokio::test]
async fn check_with_a_record_writes_nothing() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Keep", KEEP);
    repo.tag("Line1/Speed", SPEED);
    let inner = importing(&repo, FakeProjects::new()).await;
    inner.set_tag_tree(
        PROVIDER,
        live_line1(json!([keep_live("Keep"), keep_live("Old")])),
    );
    repo.tag_record(PROJECT, &["Line1", "Line1/Keep", "Line1/Old"]);
    let gateway = ForbidProjectWrites(inner);
    let host_before = files_below(&repo.data);

    let checked = check(&repo, &gateway).await;
    assert_eq!(checked.outcome, ProjectOutcome::Checked);
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.removed_count, 1);
    assert_eq!(tags.added_count, 1);
    assert!(gateway.0.tag_import_calls().is_empty(), "zero tag_import");
    assert!(gateway.0.import_calls().is_empty(), "zero project_import");
    assert_eq!(files_below(&repo.data), host_before, "zero record writes");
    assert!(!repo.repo.join(".ign").exists());
}

/// 26-06 open point 1: an owned provider name the ssh tag-record reader and
/// writer refuse is refused when the deploy is loaded, before any contact.
#[tokio::test]
async fn a_provider_name_the_tag_record_cannot_carry_is_refused_at_load() {
    let bad = "Plant[1]";
    let quoted = format!("'{bad}'");
    let repo = checkout(&[quoted.as_str()]);
    put(
        &repo.repo,
        &format!("deploy/tag-providers/{bad}.json"),
        SETTINGS.as_bytes(),
    );
    put(
        &repo.repo,
        &format!("tags/{bad}/Speed.json"),
        SPEED.as_bytes(),
    );
    put(
        &repo.targets,
        "targets/dev.yaml",
        b"schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n      known_hosts: kh\n",
    );
    let err = load_project(&repo.input()).expect_err(bad);
    assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
    let text = err.to_string();
    assert!(text.contains("tag_providers[0]"), "{text}");
    assert!(text.contains("tag record"), "{text}");
}

// ---------------------------------------------------------------------------
// 26-08: the batches, the verify and the tag record.
// ---------------------------------------------------------------------------

/// The owned provider's tag record on the master's host, as the run left it.
fn host_tag_record(repo: &Checkout) -> TagRecord {
    let bytes = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("the tag record");
    TagRecord::parse(&bytes, TAG_RECORD_REL, "capgw", PROVIDER).expect("a valid tag record")
}

/// The run's `data.tags`, as JSON.
fn tags_json(result: &DeployProjectsResult) -> Value {
    serde_json::to_value(result).expect("json")["tags"].clone()
}

/// The repo's `Line1/Count`, changed against `live_count(..)`: new
/// documentation, the same configuration otherwise.
const COUNT_DOC: &str = r#"{"dataType":"Int4","documentation":"new","tagType":"AtomicTag","value":1,"valueSource":"memory"}"#;

/// `Line1/Count` live: value 9 (a runtime write), `defaultValue` 1 and the
/// old documentation, beside `others`.
fn live_count(others: Value) -> Value {
    let mut children = vec![
        json!({"name":"Count","tagType":"AtomicTag","dataType":"Int4",
        "valueSource":"memory","value":9,"defaultValue":1,"documentation":"old"}),
    ];
    children.extend(others.as_array().cloned().unwrap_or_default());
    live_line1(Value::Array(children))
}

/// D-03, R3-POL: a changed existing node goes `MergeOverwrite` without its
/// runtime keys: the configuration changes and the live value stays.
#[tokio::test]
async fn changed_node_keeps_live_value() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));

    let checked = check(&repo, &gateway).await;
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.changed[0].path, "[PlantA]Line1/Count");
    assert_eq!(tags.changed[0].properties, ["documentation"]);

    let result = apply(&repo, &gateway).await.expect("the change applies");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    assert_eq!(
        gateway.tag_import_calls(),
        [(
            PROVIDER.to_string(),
            "Line1".to_string(),
            "MergeOverwrite".to_string()
        )]
    );
    let sent = &gateway.tag_import_bodies()[0]["tags"][0];
    assert!(sent.get("value").is_none(), "stripped: {sent}");
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["value"], 9, "the live value is kept");
    assert_eq!(count["documentation"], "new");
    assert_eq!(count["defaultValue"], 1);
    let data = tags_json(&result);
    assert_eq!(data["verify"]["equal"], true, "{data}");
    assert_eq!(data["carried"], json!([]));
    assert_eq!(data["carried_count"], 0);
}

/// The repo's two types, the base `Zeta` used by `Alpha`, an instance of
/// `Alpha` and a plain tag.
fn typed_checkout() -> Checkout {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "_types_/Zeta",
        r#"{"tagType":"UdtType","tags":[{"name":"M","tagType":"AtomicTag","value":0,"valueSource":"memory"}]}"#,
    );
    repo.tag(
        "_types_/Alpha",
        r#"{"tagType":"UdtType","tags":[{"name":"Z","tagType":"UdtInstance","typeId":"Zeta"}]}"#,
    );
    repo.tag(
        "Line1/Pump",
        r#"{"tagType":"UdtInstance","typeId":"Alpha"}"#,
    );
    repo.tag("Line1/Speed", SPEED);
    repo
}

/// Research Q1: types go first, one per import at the provider root wrapped
/// in `_types_`, a type after the types it uses, then the tags.
#[tokio::test]
async fn types_before_tags_in_dependency_order() {
    let repo = typed_checkout();
    let gateway = importing(&repo, FakeProjects::new()).await;

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    let calls: Vec<(String, String)> = gateway
        .tag_import_calls()
        .into_iter()
        .map(|(_, path, policy)| (path, policy))
        .collect();
    assert_eq!(
        calls,
        [
            ("".to_string(), "Abort".to_string()),
            ("".to_string(), "Abort".to_string()),
            ("Line1".to_string(), "Abort".to_string()),
        ]
    );
    let bodies = gateway.tag_import_bodies();
    let type_in = |i: usize| {
        let wrapper = &bodies[i]["tags"][0];
        assert_eq!(wrapper["name"], "_types_");
        wrapper["tags"][0]["name"]
            .as_str()
            .expect("a type")
            .to_string()
    };
    assert_eq!((type_in(0), type_in(1)), ("Zeta".into(), "Alpha".into()));
    let names: Vec<&str> = bodies[2]["tags"]
        .as_array()
        .expect("tags")
        .iter()
        .map(|n| n["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["Pump", "Speed"]);
    assert!(live_node(&gateway, "_types_/Alpha").await.is_some());
}

/// Pitfall 5: an answer with a failure count stops the tag step: no later
/// batch, no project import. The rollback (26-11) cannot delete the type the
/// first batch created (R3-DEL), so the run ends `rollback_failed` naming it
/// and what was sent.
#[tokio::test]
async fn quality_failure_stops_the_tag_step() {
    let repo = typed_checkout();
    let gateway = importing(&repo, FakeProjects::new().with_tag_import_failure(2)).await;

    let err = apply(&repo, &gateway).await.expect_err("a failed batch");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    let text = err.to_string();
    assert!(text.contains("[PlantA]_types_/Alpha"), "{text}");
    assert!(text.contains("2 tag write(s)"), "{text}");
    assert!(
        text.contains("[PlantA]_types_/Zeta (created by this run"),
        "{text}"
    );
    assert_eq!(gateway.tag_import_calls().len(), 2, "no later batch");
    assert!(gateway.import_calls().is_empty(), "no project import");
    assert!(
        !repo.data.join(TAG_RECORD_REL).exists()
            || host_tag_record(&repo).run_id == "20261020T101500Z-ab12",
        "the old record stands"
    );
}

/// The verify reads until two reads agree: reads that differ (a settling
/// gateway) are read again, and child order never counts (R3-KEY, F-P1).
#[tokio::test]
async fn verify_waits_for_two_equal_reads() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(
        &repo,
        FakeProjects::new()
            .with_tag_churn(3)
            .with_tag_child_order_flips(),
    )
    .await;

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    let verify = &tags_json(&result)["verify"];
    assert_eq!(verify["equal"], true, "{verify}");
    assert_eq!(verify["settled"], true, "{verify}");
    // Three churned reads, the first settled one, and the one that agrees.
    assert_eq!(verify["reads"], 5, "{verify}");
    assert!(verify["elapsed_s"].as_f64().is_some());
    assert_eq!(verify["mismatches_count"], 0);
}

/// R3-VK: values are never compared, on a created node (written at runtime
/// before the verify) or on a changed one.
#[tokio::test]
async fn verify_ignores_values_on_created_nodes() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new().with_created_value(json!(99))).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    assert_eq!(tags_json(&result)["verify"]["equal"], true);
    let speed = live_node(&gateway, "Line1/Speed").await.expect("created");
    assert_eq!(speed["value"], 99, "the runtime write stands");
    assert_eq!(speed["defaultValue"], 3, "the seed is the repo value");
}

/// D-01: a live-only node is never in a body and is as it was after the run.
#[tokio::test]
async fn drift_never_sent() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_count(json!([
            {"name":"Stray","tagType":"AtomicTag","valueSource":"memory","value":4},
            {"name":"Old","tagType":"Folder","tags":[
                {"name":"T","tagType":"AtomicTag","valueSource":"memory","value":5}
            ]}
        ])),
    );
    let stray = live_node(&gateway, "Line1/Stray").await.expect("live");

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    let tags = result.tags.as_ref().expect("data.tags");
    assert_eq!(
        tags.drift,
        [
            "[PlantA]Line1/Old",
            "[PlantA]Line1/Old/T",
            "[PlantA]Line1/Stray"
        ]
    );
    for body in gateway.tag_import_bodies() {
        let text = body.to_string();
        assert!(
            !text.contains("Stray") && !text.contains("\"Old\""),
            "{text}"
        );
    }
    assert_eq!(gateway.tag_import_bodies().len(), 2);
    assert_eq!(live_node(&gateway, "Line1/Stray").await, Some(stray));
    assert!(live_node(&gateway, "Line1/Old/T").await.is_some());
}

/// Choice (c) `overwrite-carry`: a repo change that drops a property goes
/// `Overwrite` with the live value carried from an export read right before
/// that batch, so a runtime write between the plan and the send survives,
/// and the path is listed in `data.tags.carried`.
#[tokio::test]
async fn removed_property_follows_choice_c() {
    let repo = checkout(&[PROVIDER]);
    // No tooltip in the repo: the live tooltip is to go.
    repo.tag(
        "Line1/Count",
        r#"{"dataType":"Int4","tagType":"AtomicTag","value":1,"valueSource":"memory"}"#,
    );
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(
        &repo,
        FakeProjects::new().with_value_write_after_first_import(PROVIDER, "Line1/Count", json!(77)),
    )
    .await;
    gateway.set_tag_tree(
        PROVIDER,
        live_line1(
            json!([{"name":"Count","tagType":"AtomicTag","dataType":"Int4",
            "valueSource":"memory","value":9,"defaultValue":1,"tooltip":"tip"}]),
        ),
    );
    let checked = check(&repo, &gateway).await;
    assert_eq!(
        checked.tags.as_ref().expect("data.tags").changed[0].properties,
        ["tooltip"]
    );

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    let policies: Vec<String> = gateway
        .tag_import_calls()
        .into_iter()
        .map(|(_, _, policy)| policy)
        .collect();
    assert_eq!(policies, ["Abort", "Overwrite"]);
    assert_eq!(
        gateway.tag_import_bodies()[1]["tags"][0]["value"],
        77,
        "carried from the fresh export, not the pre-image's 9"
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert!(
        count.get("tooltip").is_none(),
        "the property is gone: {count}"
    );
    assert_eq!(count["value"], 77, "the runtime write survives");
    let data = tags_json(&result);
    assert_eq!(data["carried"], json!(["[PlantA]Line1/Count"]));
    assert_eq!(data["carried_count"], 1);
    let run_dir = repo
        .repo
        .join(result.run_dir.as_deref().expect("a run dir"));
    let names = event_names(&run_dir);
    let carried = position(&names, "tag_values_carried");
    assert!(
        carried > position(&names, "tag_import_answered"),
        "{names:?}"
    );
    let log = std::fs::read_to_string(run_dir.join("events.ndjson")).expect("events");
    for line in log.lines() {
        let event: Value = serde_json::from_str(line).expect("an event line");
        assert!(
            event.get("value").is_none() && !line.contains("\"value\""),
            "never a value in the events: {line}"
        );
    }
}

/// 26-08 open point 3, KL-5 (26-11 carry guard): a release that drops a
/// property from a memory tag whose live node holds a value and names no
/// `valueSource` would need a carried `Overwrite` that cannot carry the value
/// (it would reset it), so the run refuses `deploy_tree_invalid` naming the
/// tag before any write.
#[tokio::test]
async fn carry_guard_refuses_value_without_value_source() {
    let repo = checkout(&[PROVIDER]);
    repo.tag(
        "Line1/Count",
        r#"{"dataType":"Int4","tagType":"AtomicTag","value":1}"#,
    );
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_line1(
            json!([{"name":"Count","tagType":"AtomicTag","dataType":"Int4",
            "value":9,"defaultValue":1,"tooltip":"tip"}]),
        ),
    );
    let checked = check(&repo, &gateway).await;
    assert_eq!(
        checked.tags.as_ref().expect("data.tags").changed[0].properties,
        ["tooltip"]
    );

    let before = gateway.call_log().len();
    let err = apply(&repo, &gateway)
        .await
        .expect_err("the value cannot be carried");
    assert_eq!(err.code(), "deploy_tree_invalid", "{err}");
    let text = err.to_string();
    assert!(text.contains("Line1/Count"), "{text}");
    assert!(text.contains("valueSource"), "{text}");
    assert!(gateway.tag_import_calls().is_empty(), "zero tag imports");
    assert!(gateway.import_calls().is_empty(), "no project import");
    let log = gateway.call_log().split_off(before);
    assert!(
        !log.iter()
            .any(|c| c.starts_with("tag-provider-create") || c.starts_with("tag-provider-modify")),
        "{log:?}"
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["value"], 9, "untouched");
    assert_eq!(count["tooltip"], "tip", "untouched");
}

/// TAGDEP-09: after the project is verified the run writes the owned
/// provider's tag record: the repo's nodes and groups, under the run id and
/// release of the release record.
#[tokio::test]
async fn record_written_after_project_verify() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    put(
        &repo.repo,
        "tags/PlantA/.tag-groups.json",
        br#"{"tagGroups":[{"name":"Fast","rate":100},{"name":"Default","rate":1000}]}"#,
    );
    let gateway = importing(&repo, FakeProjects::new()).await;
    // The summary's groups are new on the gateway (26-10): they go through
    // the master's file engine, which the run builds from the target.
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let deploy = fake.client("cap");
    let seams = Seams {
        tree: Some(&reader),
        deploy: Some(&deploy),
        ..Seams::default()
    };
    let result = apply_with(&repo, &gateway, seams)
        .await
        .expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    assert!(repo.mode_unit("Fast", "config.json").exists());
    let run_id = result.run_id.clone().expect("a run id");
    let record = host_tag_record(&repo);
    assert_eq!(record.project, PROJECT);
    assert_eq!(record.release, "v1.0.0");
    assert_eq!(record.run_id, run_id);
    assert_eq!(record.nodes, ["Line1", "Line1/Speed"]);
    assert_eq!(record.groups, ["Default", "Fast"]);
    assert_eq!(
        record.tags_sha256,
        result.tags.as_ref().expect("data.tags").tags_sha256
    );
    let release: Value = serde_json::from_slice(
        &std::fs::read(
            repo.data
                .join("ign-deploy/capgw/projects/CapApp/release.json"),
        )
        .expect("the release record"),
    )
    .expect("json");
    assert_eq!(release["run_id"], run_id.as_str());
    let names = event_names(&repo.repo.join(result.run_dir.as_deref().expect("run dir")));
    assert!(position(&names, "verified") < position(&names, "tag_record_written"));

    // The next plan reads it: nothing left to do.
    let again = apply_with(&repo, &gateway, seams)
        .await
        .expect("the second run");
    assert_eq!(again.outcome, ProjectOutcome::Skipped);
}

/// A project that is not verified leaves no tag record either. With a tag
/// the run created, the rollback (26-11) cannot finish (R3-DEL): the run ends
/// `rollback_failed` and the old record stands.
#[tokio::test]
async fn record_not_written_when_project_unverified() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.mutate_child_after_import(PROJECT, 1);
    let before = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("the old record");

    let err = apply(&repo, &gateway)
        .await
        .expect_err("the created tag cannot be removed");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    assert!(err.to_string().contains("project_unverified"), "{err}");
    assert_eq!(
        std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("the record"),
        before,
        "the old record stands"
    );
    let names = event_names(&only_run_dir(&repo));
    assert!(
        !names.iter().any(|n| n == "tag_record_written"),
        "{names:?}"
    );
}

/// TAGDEP-11, adoption: a provider whose nodes all equal the repo and that
/// has no record plans only the record; that is a change to run, and the run
/// writes the record without a single tag import.
#[tokio::test]
async fn adoption_record_only_writes_record() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Keep", KEEP);
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members(CODE_V1))
        .with_tag_tree(PROVIDER, live_line1(json!([keep_live("Keep")])));
    let sha = load_project(&repo.input())
        .expect("loads")
        .artifact
        .digest
        .sha256;
    repo.record("v1.0.0", &sha);

    let checked = check(&repo, &gateway).await;
    assert!(checked.tags.as_ref().expect("data.tags").providers[0].record_only);
    assert!(checked.would_change, "writing the record is a change");
    assert_eq!(checked.action, ProjectAction::Skip);

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    assert!(gateway.tag_import_calls().is_empty(), "no tag import");
    assert!(gateway.import_calls().is_empty(), "no project import");
    let record = host_tag_record(&repo);
    assert_eq!(record.nodes, ["Line1", "Line1/Keep"]);
    assert_eq!(record.project, PROJECT);

    let again = apply(&repo, &gateway).await.expect("the second run");
    assert_eq!(again.outcome, ProjectOutcome::Skipped);
}

// ---- 26-10: provider settings (D-09, D-12, C11) ----

/// The owned provider's settings with a description the live provider lacks.
const SETTINGS_DESCRIBED: &str =
    r#"{"schema_version": 1, "enabled": true, "description": "Plant A", "config": {}}"#;

/// `data.tags.provider_settings` as JSON.
fn provider_settings_json(result: &DeployProjectsResult) -> Value {
    tags_json(result)["provider_settings"].clone()
}

/// A gateway holding the repo's project with older code, recorded at
/// v0.9.0, and no owned tag provider at all.
async fn without_provider(repo: &Checkout) -> FakeProjects {
    let gateway = FakeProjects::new().with_project(PROJECT, None, false, &members(CODE_V2));
    let live = gateway.export_bytes(PROJECT).await.expect("export");
    let digest =
        ignition_core::deploy::project_artifact::canonical_digest(&live).expect("a project export");
    repo.record("v0.9.0", &digest.sha256);
    gateway
}

/// Settings equal to the live record plan `same` and write nothing.
#[tokio::test]
async fn provider_settings_same_writes_nothing() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;

    let checked = check(&repo, &gateway).await;
    assert_eq!(
        provider_settings_json(&checked),
        json!([{"provider": "PlantA", "action": "same", "properties": []}])
    );
    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert!(result.ok, "{result:?}");
    assert!(
        !gateway
            .call_log()
            .iter()
            .any(|c| c.starts_with("tag-provider-create") || c.starts_with("tag-provider-modify")),
        "{:?}",
        gateway.call_log()
    );
}

/// Settings that differ are modified with the live signature before the
/// first tag import, the live record is saved as the pre-image (0600), and
/// the record is read again to verify.
#[tokio::test]
async fn provider_settings_modify_before_the_first_tag_import() {
    let repo = checkout(&[PROVIDER]);
    put(
        &repo.repo,
        "deploy/tag-providers/PlantA.json",
        SETTINGS_DESCRIBED.as_bytes(),
    );
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    let signature_before = gateway
        .provider_record(PROVIDER)
        .expect("listed")
        .signature
        .expect("a signature");

    let checked = check(&repo, &gateway).await;
    assert_eq!(
        provider_settings_json(&checked),
        json!([{"provider": "PlantA", "action": "modify", "properties": ["description"]}])
    );
    assert!(checked.would_change);

    let before = gateway.call_log().len();
    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert!(result.ok, "{result:?}");
    let log = gateway.call_log().split_off(before);
    let modify = position(&log, "tag-provider-modify:PlantA");
    let import = position(&log, "tag-import:PlantA:Line1:Abort");
    assert!(modify < import, "{log:?}");
    let reread = log[modify..]
        .iter()
        .position(|c| c == "tag-provider-find:PlantA")
        .map(|at| at + modify)
        .expect("the record is read again after the modify");
    assert!(reread < import, "{log:?}");
    assert_eq!(
        gateway.provider_modify_signatures(),
        std::slice::from_ref(&signature_before),
        "the modify carries the signature of a fresh find"
    );

    let live = gateway.provider_record(PROVIDER).expect("listed");
    assert_eq!(live.description(), Some("Plant A"));
    assert_ne!(live.signature.as_deref(), Some(signature_before.as_str()));

    let run_dir = repo
        .repo
        .join(result.run_dir.as_deref().expect("a run dir"));
    let pre = run_dir.join("export/tags/PlantA.provider.json");
    let saved: Value =
        serde_json::from_slice(&std::fs::read(&pre).expect("the provider pre-image"))
            .expect("json");
    assert_eq!(saved["name"], "PlantA");
    assert!(saved["description"].is_null(), "{saved}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&pre).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let names = event_names(&run_dir);
    let applied = position(&names, "tag_provider_settings_applied");
    assert!(
        applied > position(&names, "tags_pre_image_exported"),
        "{names:?}"
    );
    assert!(applied < position(&names, "tag_import_sent"), "{names:?}");
}

/// A provider the gateway lacks is created from the repo's settings, before
/// any tag import into it; it has no pre-image.
#[tokio::test]
async fn provider_settings_create_when_the_gateway_lacks_the_provider() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = without_provider(&repo).await;
    let mut input = repo.input();
    input.allow_first_change = Some("a new provider".to_string());

    let (loaded, prepared) = plan_with(&repo, &input, &gateway, Seams::default())
        .await
        .expect("the plan completes");
    let ProjectsPrepared::Ready { plan: checked, .. } = &prepared else {
        panic!("a ready plan");
    };
    assert_eq!(
        provider_settings_json(checked),
        json!([{"provider": "PlantA", "action": "create", "properties": []}])
    );
    let tags = checked.tags.as_ref().expect("data.tags");
    assert_eq!(tags.added, ["[PlantA]Line1", "[PlantA]Line1/Speed"]);
    assert!(gateway.provider_record(PROVIDER).is_none());

    let before = gateway.call_log().len();
    let result = run_with(&repo, &input, &gateway, Seams::default(), &loaded, prepared)
        .await
        .expect("the run completes");
    assert!(result.ok, "{result:?}");
    let log = gateway.call_log().split_off(before);
    let create = position(&log, "tag-provider-create:PlantA");
    let first_import = log
        .iter()
        .position(|c| c.starts_with("tag-import:PlantA:"))
        .expect("a tag import");
    assert!(create < first_import, "{log:?}");
    let live = gateway.provider_record(PROVIDER).expect("created");
    assert!(live.enabled);
    assert!(live_node(&gateway, "Line1/Speed").await.is_some());
    let run_dir = repo
        .repo
        .join(result.run_dir.as_deref().expect("a run dir"));
    assert!(!run_dir.join("export/tags/PlantA.provider.json").exists());
}

/// A gateway that answers the modify but keeps the old settings fails the
/// re-read verify: the run stops before any tag import and before the
/// project import.
#[tokio::test]
async fn provider_settings_verify_mismatch_stops_before_any_tag_write() {
    let repo = checkout(&[PROVIDER]);
    put(
        &repo.repo,
        "deploy/tag-providers/PlantA.json",
        SETTINGS_DESCRIBED.as_bytes(),
    );
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new().with_provider_writes_ignored()).await;

    let result = apply(&repo, &gateway)
        .await
        .expect("a verdict: the settings did not land and nothing else was written");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["cause"], "tag_step_failed");
    assert!(
        rollback["detail"]
            .as_str()
            .expect("detail")
            .contains("PlantA"),
        "{rollback}"
    );
    assert_eq!(
        rollback["provider_restored"],
        json!([]),
        "the settings never changed, so nothing was put back"
    );
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
}

// ---- 26-10: tag groups through the file engine (D-12, R3-GRP, R3-MODE) ----

use common::fake_deploy::FakeGateway;
use common::faulty_deploy::{DeployFaults, FaultyDeploy};
use ignition_core::deploy::apply::host::{BatchReport, BatchSpec, Prepared, ProbeReport};
use ignition_core::deploy::apply::record::{RecordState, RunRecord};
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::tree::RenderedTree;

/// The gateway's own `Default` group unit (C14), in `core`.
const DEFAULT_GROUP: &str = r#"{
  "config": {
    "mode": "Direct",
    "oneShot": false,
    "opcDataMode": "Subscribed",
    "optWriteTimeout": 60000,
    "optWrites": false,
    "rate": 1000,
    "readAfterWrite": false
  }
}"#;
const GROUP_RESOURCE: &str = r#"{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{}}"#;

/// A memory tag in tag group `group`.
fn tag_in(group: &str) -> String {
    format!(
        r#"{{"dataType":"Int4","tagType":"AtomicTag","value":3,"valueSource":"memory","tagGroup":"{group}"}}"#
    )
}

impl Checkout {
    /// `tags/PlantA/.tag-groups.json` holding `groups`.
    fn groups(&self, groups: Value) {
        put(
            &self.repo,
            "tags/PlantA/.tag-groups.json",
            serde_json::to_string_pretty(&json!({ "tagGroups": groups }))
                .expect("json")
                .as_bytes(),
        );
    }

    /// A tag-group unit of `PlantA` in `collection` on the master.
    fn group_unit(&self, collection: &str, group: &str, config: &str) {
        let dir = format!("config/resources/{collection}/ignition/tag-group/PlantA/{group}");
        put(&self.data, &format!("{dir}/config.json"), config.as_bytes());
        put(
            &self.data,
            &format!("{dir}/resource.json"),
            GROUP_RESOURCE.as_bytes(),
        );
    }

    /// The master's unit file of `group` in the `dev` mode collection.
    fn mode_unit(&self, group: &str, file: &str) -> PathBuf {
        self.data.join(format!(
            "config/resources/dev/ignition/tag-group/PlantA/{group}/{file}"
        ))
    }

    /// The tag record of the owned provider listing `nodes` and `groups`.
    fn tag_record_with_groups(&self, nodes: &[&str], groups: &[&str]) {
        let mut record = TagRecord {
            schema_version: 1,
            gateway: "capgw".to_string(),
            provider: PROVIDER.to_string(),
            project: PROJECT.to_string(),
            release: "v0.9.0".to_string(),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
            tags_sha256: "ab".repeat(32),
            nodes: nodes.iter().map(|n| (*n).to_string()).collect(),
            groups: groups.iter().map(|g| (*g).to_string()).collect(),
        };
        record.nodes.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        record.groups.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        put(&self.data, TAG_RECORD_REL, &record.to_bytes());
    }

    /// The master's local tree reader.
    fn reader(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.data.clone(), "nodes.master")
    }
}

/// An [`ApplyHost`] over the master's local host that keeps every spec it
/// staged and where the gateway's call log stood when it committed.
struct SpyHost<'a> {
    inner: LocalFsTransport,
    gateway: &'a FakeProjects,
    specs: Mutex<Vec<BatchSpec>>,
    commit_at: Mutex<Vec<usize>>,
}

impl<'a> SpyHost<'a> {
    fn new(repo: &Checkout, gateway: &'a FakeProjects) -> Self {
        SpyHost {
            inner: LocalFsTransport::new(repo.data.clone(), "nodes.master")
                .with_apply("capgw", "dev")
                .expect("an apply host"),
            gateway,
            specs: Mutex::new(Vec::new()),
            commit_at: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl ApplyHost for SpyHost<'_> {
    async fn probe(&self) -> Result<ProbeReport, CoreError> {
        self.inner.probe().await
    }
    async fn probe_paths(&self, touched: &[String]) -> Result<ProbeReport, CoreError> {
        self.inner.probe_paths(touched).await
    }
    async fn prepare(&self, spec: &BatchSpec, tree: &RenderedTree) -> Result<Prepared, CoreError> {
        self.specs.lock().expect("specs").push(spec.clone());
        self.inner.prepare(spec, tree).await
    }
    async fn commit(&self, prepared: &Prepared) -> Result<BatchReport, CoreError> {
        self.commit_at
            .lock()
            .expect("commit")
            .push(self.gateway.call_log().len());
        self.inner.commit(prepared).await
    }
    async fn restore(&self, record: &RunRecord) -> Result<BatchReport, CoreError> {
        self.inner.restore(record).await
    }
    async fn fingerprint(
        &self,
        paths: &[String],
    ) -> Result<std::collections::BTreeMap<String, Option<String>>, CoreError> {
        self.inner.fingerprint(paths).await
    }
    async fn finalize(&self, record: &RunRecord) -> Result<(), CoreError> {
        self.inner.finalize(record).await
    }
    async fn read_record(&self, run_id: &RunId) -> Result<Option<RunRecord>, CoreError> {
        self.inner.read_record(run_id).await
    }
    async fn fetch_snapshot(
        &self,
        run_id: &RunId,
        dest: &Path,
    ) -> Result<std::collections::BTreeMap<String, String>, CoreError> {
        self.inner.fetch_snapshot(run_id, dest).await
    }
    async fn discard_interrupted(&self, run_id: &RunId) -> Result<(), CoreError> {
        self.inner.discard_interrupted(run_id).await
    }
}

/// A repo whose `Line1/Speed` names group `Fast`, with a summary adding
/// `Fast` beside the gateway's `Default`, on a gateway that imports the
/// project; `Default` lives in `core`.
async fn fast_group_rig(repo: &Checkout) -> FakeProjects {
    repo.tag("Line1/Speed", &tag_in("Fast"));
    repo.groups(json!([
        {"name": "Default", "rate": 1000, "mode": "Direct", "oneShot": false},
        {"name": "Fast", "rate": 250}
    ]));
    repo.group_unit("core", "Default", DEFAULT_GROUP);
    importing(repo, FakeProjects::new()).await
}

fn groups_json(result: &DeployProjectsResult) -> Value {
    tags_json(result)["groups"].clone()
}

/// The group units are committed through the file engine before the first
/// tag import, the run record lands at `applied`, and the tag record lists
/// the repo's groups.
#[tokio::test]
async fn groups_written_before_the_first_tag_import() {
    let repo = checkout(&[PROVIDER]);
    let gateway = fast_group_rig(&repo).await;
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let deploy = fake.client("cap");
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };

    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("the plan completes");
    let ProjectsPrepared::Ready { plan: planned, .. } = &prepared else {
        panic!("a ready plan");
    };
    assert_eq!(
        groups_json(planned),
        json!([{"provider": "PlantA", "added": ["Fast"], "changed": [], "removed": [],
                "drift": [], "same": 1}])
    );

    let before = gateway.call_log().len();
    let result = run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect("the run completes");
    assert!(result.ok, "{result:?}");
    let commit_at = host.commit_at.lock().expect("commit").clone();
    assert_eq!(commit_at.len(), 1, "one commit");
    let log = gateway.call_log();
    let import = before + position(&log[before..], "tag-import:PlantA:Line1:Abort");
    assert!(commit_at[0] <= import, "{log:?} commit at {}", commit_at[0]);
    assert_eq!(fake.scan_count(), 1, "one config scan");

    let config: Value = serde_json::from_slice(
        &std::fs::read(repo.mode_unit("Fast", "config.json")).expect("unit"),
    )
    .expect("json");
    assert_eq!(config["config"]["rate"], 250);
    assert!(repo.mode_unit("Fast", "resource.json").exists());
    assert!(
        !repo.mode_unit("Default", "config.json").exists(),
        "an equal group is not copied into the mode collection"
    );

    let run_id = RunId::parse(result.run_id.as_deref().expect("a run id")).expect("run id");
    let record = host
        .read_record(&run_id)
        .await
        .expect("readable")
        .expect("the run record");
    assert_eq!(record.state, RecordState::Applied);
    assert_eq!(host_tag_record(&repo).groups, ["Default", "Fast"]);
    let run_dir = repo
        .repo
        .join(result.run_dir.as_deref().expect("a run dir"));
    let names = event_names(&run_dir);
    assert!(
        position(&names, "tag_groups_written") < position(&names, "tag_import_sent"),
        "{names:?}"
    );
}

/// The group batch never carries the gateway-config ledger, and the ledger
/// on the box is left byte for byte.
#[tokio::test]
async fn groups_never_touch_the_ledger() {
    let repo = checkout(&[PROVIDER]);
    let gateway = fast_group_rig(&repo).await;
    let ledger = repo
        .data
        .join(ignition_core::deploy::ledger::ledger_rel("capgw", "dev"));
    put(
        &repo.data,
        &ignition_core::deploy::ledger::ledger_rel("capgw", "dev"),
        b"{\"keep\":true}",
    );
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let deploy = fake.client("cap");
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };
    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("plans");
    run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect("the run completes");
    let specs = host.specs.lock().expect("specs");
    assert_eq!(specs.len(), 1);
    assert!(specs[0].ledger_new.is_none() && specs[0].ledger_pre.is_none());
    assert!(
        specs[0]
            .touched_paths()
            .iter()
            .all(|p| p.starts_with("ignition/tag-group/PlantA/")),
        "{:?}",
        specs[0].touched_paths()
    );
    assert_eq!(std::fs::read(&ledger).expect("ledger"), b"{\"keep\":true}");
}

/// An apply journal on the box (a gateway-config run) refuses the run
/// before anything is written.
#[tokio::test]
async fn groups_journal_present_refuses_with_no_write() {
    let repo = checkout(&[PROVIDER]);
    let gateway = fast_group_rig(&repo).await;
    let journal = ignition_core::deploy::apply::journal::Journal {
        schema_version: 1,
        run_id: "20261101T101500Z-cd34".to_string(),
        gateway: "capgw".to_string(),
        mode: "dev".to_string(),
        phase: ignition_core::deploy::apply::journal::JournalPhase::Writing,
        pre_fingerprint: "c".repeat(64),
        touched: vec!["ignition/x/y/config.json".to_string()],
    };
    put(
        &repo.data,
        &ignition_core::deploy::ledger::journal_rel("capgw", "dev"),
        &journal.to_bytes(),
    );
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let deploy = fake.client("cap");
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };
    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("plans");
    let err = run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect_err("a journal is on the box");
    assert_eq!(err.code(), "deploy_journal_present", "{err:?}");
    assert!(err.to_string().contains("20261101T101500Z-cd34"), "{err}");
    assert!(
        host.specs.lock().expect("specs").is_empty(),
        "nothing staged"
    );
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
    assert!(!repo.mode_unit("Fast", "config.json").exists());
    assert_eq!(fake.lock_acquires(), 0);
}

/// A config scan that fails after the commit restores the group units from
/// the snapshot; no tag import and no project import follow.
#[tokio::test]
async fn groups_scan_failure_restores_the_units() {
    let repo = checkout(&[PROVIDER]);
    let gateway = fast_group_rig(&repo).await;
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let faults = DeployFaults::default().fail("scan_request", 1);
    faults.arm();
    let deploy = FaultyDeploy::new(fake.client("cap"), faults.clone());
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };
    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("plans");
    let result = run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect("a verdict: the scan failed and the units were restored");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["groups_restored"], true, "{rollback}");
    assert!(
        rollback["detail"]
            .as_str()
            .expect("detail")
            .contains("restored"),
        "{rollback}"
    );
    assert_eq!(faults.failed("scan_request"), 1);
    assert!(!repo.mode_unit("Fast", "config.json").exists(), "restored");
    assert!(
        !repo.mode_unit("Fast", "resource.json").exists(),
        "restored"
    );
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
    let run_id = host.specs.lock().expect("specs")[0].run_id.clone();
    let record = host
        .read_record(&run_id)
        .await
        .expect("readable")
        .expect("the run record");
    assert_eq!(record.state, RecordState::RolledBack);
}

/// A group this project recorded and the repo dropped is deleted from the
/// mode collection; one a repo tag still names is kept as drift.
#[tokio::test]
async fn groups_removed_only_when_recorded_and_unreferenced() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", &tag_in("Kept"));
    repo.groups(json!([
        {"name": "Default", "rate": 1000, "mode": "Direct", "oneShot": false}
    ]));
    repo.group_unit("core", "Default", DEFAULT_GROUP);
    repo.group_unit("dev", "Old", DEFAULT_GROUP);
    repo.group_unit("dev", "Kept", DEFAULT_GROUP);
    let gateway = importing(&repo, FakeProjects::new()).await;
    repo.tag_record_with_groups(&["Line1"], &["Old", "Kept"]);
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let deploy = fake.client("cap");
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };
    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("plans");
    let ProjectsPrepared::Ready { plan: planned, .. } = &prepared else {
        panic!("a ready plan");
    };
    assert_eq!(
        groups_json(planned),
        json!([{"provider": "PlantA", "added": [], "changed": [], "removed": ["Old"],
                "drift": ["Kept"], "same": 1}])
    );
    let result = run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect("the run completes");
    assert!(result.ok, "{result:?}");
    assert!(!repo.mode_unit("Old", "config.json").exists(), "deleted");
    assert!(!repo.mode_unit("Old", "resource.json").exists(), "deleted");
    assert!(repo.mode_unit("Kept", "config.json").exists(), "kept");
}

/// R3-MODE on a pair: a tag definition unit of the owned provider in the
/// mode collection refuses the deploy before any write, in `--check` too.
#[tokio::test]
async fn groups_pair_mode_tag_units_refused() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let backup = repo.data.parent().expect("scratch").join("data-b");
    std::fs::create_dir_all(backup.join("projects")).expect("backup data");
    repo.target_extra(&format!(
        "  backup:\n    profile: devb\n    local:\n      data_dir: '{}'\n",
        backup.display()
    ));
    put(
        &repo.data,
        "config/resources/dev/ignition/tag-definition/PlantA/tags.json",
        b"[]",
    );
    let gateway = importing(&repo, FakeProjects::new()).await;
    let reader = repo.reader();
    let err = plan_with(
        &repo,
        &repo.input(),
        &gateway,
        Seams {
            tree: Some(&reader),
            ..Seams::default()
        },
    )
    .await
    .expect_err("mode-collection tag units on a pair");
    assert_eq!(err.code(), "deploy_tree_invalid", "{err:?}");
    let text = err.to_string();
    assert!(text.contains("ignition/tag-definition/PlantA"), "{text}");
    assert!(text.contains("C8"), "{text}");
    assert!(gateway.tag_import_calls().is_empty());
}

/// R3-MODE on a standalone target: the same unit is no refusal (A4 holds:
/// the import edits the effective mode copy).
#[tokio::test]
async fn groups_standalone_mode_tag_units_pass() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    put(
        &repo.data,
        "config/resources/dev/ignition/tag-definition/PlantA/tags.json",
        b"[]",
    );
    let gateway = importing(&repo, FakeProjects::new()).await;
    let reader = repo.reader();
    let (_, prepared) = plan_with(
        &repo,
        &repo.input(),
        &gateway,
        Seams {
            tree: Some(&reader),
            ..Seams::default()
        },
    )
    .await
    .expect("a standalone target is not refused");
    let checked = prepared.into_check_result();
    assert_eq!(checked.tags.as_ref().expect("data.tags").added_count, 1);
}

/// R3-GRP: a tag the run would send naming a group neither the summary nor
/// the gateway has is refused before any write, naming the file, the tag and
/// the group.
#[tokio::test]
async fn groups_missing_group_refused_before_any_write() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", &tag_in("Nope"));
    repo.group_unit("core", "Default", DEFAULT_GROUP);
    let gateway = importing(&repo, FakeProjects::new()).await;
    let reader = repo.reader();
    let err = plan_with(
        &repo,
        &repo.input(),
        &gateway,
        Seams {
            tree: Some(&reader),
            ..Seams::default()
        },
    )
    .await
    .expect_err("a missing group");
    assert_eq!(err.code(), "deploy_tree_invalid", "{err:?}");
    let text = err.to_string();
    for needle in ["tags/PlantA/Line1/Speed.json", "Line1/Speed", "Nope"] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    assert!(gateway.tag_import_calls().is_empty());

    // The gateway's own group passes.
    repo.tag("Line1/Speed", &tag_in("Default"));
    plan_with(
        &repo,
        &repo.input(),
        &gateway,
        Seams {
            tree: Some(&reader),
            ..Seams::default()
        },
    )
    .await
    .expect("Default is live");
}

/// A config scan-lock another client holds refuses the group commit with
/// `scan_lock_held` (T-26-38): nothing is written into a collection, no tag
/// import follows, and no journal stays on the box.
#[tokio::test]
async fn groups_held_scan_lock_refuses_with_no_write() {
    let repo = checkout(&[PROVIDER]);
    let gateway = fast_group_rig(&repo).await;
    let reader = repo.reader();
    let fake = FakeGateway::new();
    fake.client("other")
        .scan_lock_acquire(ignition_core::client::scan::ScanKind::Config, 1, 600)
        .await
        .expect("another client takes the lock");
    let deploy = fake.client("cap");
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };
    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("plans");
    let err = run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect_err("the lock is held");
    assert_eq!(err.code(), "scan_lock_held", "{err:?}");
    assert!(
        host.commit_at.lock().expect("commit").is_empty(),
        "no commit"
    );
    assert!(!repo.mode_unit("Fast", "config.json").exists());
    assert!(
        !repo
            .data
            .join(ignition_core::deploy::ledger::journal_rel("capgw", "dev"))
            .exists()
    );
    assert!(gateway.tag_import_calls().is_empty());
    assert!(gateway.import_calls().is_empty());
}

// ---- 26-11: the automatic rollback (D-07, TAGDEP-10, R3-RB, R3-DEL) ----

/// The release record below the master node's data directory.
const RELEASE_REL: &str = "ign-deploy/capgw/projects/CapApp/release.json";

/// The repo's `Line2/X`, changed against [`two_lines`]: new documentation.
const X_DOC: &str = r#"{"dataType":"Int4","documentation":"new","tagType":"AtomicTag","value":1,"valueSource":"memory"}"#;

/// The provider live: `Line1/Count` (value 9, `defaultValue` 1, old
/// documentation, plus `count_extra`) and `Line2/X` (value 4, old
/// documentation).
fn two_lines(count_extra: Value) -> Value {
    let mut count = json!({"name":"Count","tagType":"AtomicTag","dataType":"Int4",
        "valueSource":"memory","value":9,"defaultValue":1,"documentation":"old"});
    for (key, value) in count_extra.as_object().cloned().unwrap_or_default() {
        count[key.as_str()] = value;
    }
    json!({"name":"","tagType":"Provider","tags":[
        {"name":"_types_","tagType":"Folder"},
        {"name":"Line1","tagType":"Folder","tags":[count]},
        {"name":"Line2","tagType":"Folder","tags":[
            {"name":"X","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory",
             "value":4,"defaultValue":1,"documentation":"old"}
        ]}
    ]})
}

/// A repo whose `Line1/Count` is `count` and whose `Line2/X` changes its
/// documentation, on a gateway that imports the project and holds
/// [`two_lines`]: the run sends `Line1` then `Line2`, one batch each.
async fn changed_two(repo: &Checkout, count: &str, gateway: FakeProjects) -> FakeProjects {
    repo.tag("Line1/Count", count);
    repo.tag("Line2/X", X_DOC);
    let gateway = importing(repo, gateway).await;
    gateway.set_tag_tree(PROVIDER, two_lines(json!({})));
    gateway
}

/// The only run directory of the repo.
fn only_run_dir(repo: &Checkout) -> PathBuf {
    let root = repo.repo.join(".ign/deploy/capgw");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("the gateway's run directories")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    dirs.remove(0)
}

fn run_dir_of(repo: &Checkout, result: &DeployProjectsResult) -> PathBuf {
    repo.repo
        .join(result.run_dir.as_deref().expect("a run dir"))
}

/// `(path, policy)` of every tag import, in order.
fn import_paths(gateway: &FakeProjects) -> Vec<(String, String)> {
    gateway
        .tag_import_calls()
        .into_iter()
        .map(|(_, path, policy)| (path, policy))
        .collect()
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

/// A canonical digest of a project zip.
fn digest(zip: &[u8]) -> String {
    ignition_core::deploy::project_artifact::canonical_digest(zip)
        .expect("a project export")
        .sha256
}

/// D-07: an import that fails after an earlier batch changed a node is
/// rolled back: the changed node goes back by `MergeOverwrite` of its
/// pre-image (the failed batch applied nothing, so it is not sent again),
/// the project is never imported, and the run is the verdict
/// `tags_rolled_back` (exit 6) with no record written.
#[tokio::test]
async fn import_failure_rolls_back_tags() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        COUNT_DOC,
        FakeProjects::new().with_tag_import_failure(2),
    )
    .await;
    let tag_record = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("the old record");
    let release = std::fs::read(repo.data.join(RELEASE_REL)).expect("the release record");

    let result = apply(&repo, &gateway)
        .await
        .expect("a verdict, not an error");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert!(!result.ok);
    assert_eq!(result.slug.as_deref(), Some("deploy_rolled_back"));
    assert_eq!(result.failure_exit_code(), Some(6));
    assert!(!result.record_written);
    assert!(result.restore_command.is_none());
    assert!(gateway.import_calls().is_empty(), "no project import");
    assert_eq!(
        import_paths(&gateway),
        pairs(&[
            ("Line1", "MergeOverwrite"),
            ("Line2", "MergeOverwrite"),
            ("Line1", "MergeOverwrite"),
        ])
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old", "restored: {count}");
    assert_eq!(count["value"], 9, "the live value stays");
    let x = live_node(&gateway, "Line2/X").await.expect("live");
    assert_eq!(x["documentation"], "old", "never applied: {x}");

    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["cause"], "tag_step_failed");
    assert!(
        rollback["detail"]
            .as_str()
            .expect("detail")
            .contains("[PlantA]Line2/X"),
        "{rollback}"
    );
    assert_eq!(rollback["reimported"], json!(["[PlantA]Line1/Count"]));
    assert_eq!(rollback["reimported_count"], 1);
    assert_eq!(rollback["carried"], json!([]));
    assert_eq!(rollback["undeletable"], json!([]));
    assert_eq!(rollback["groups_restored"], false);
    assert_eq!(rollback["project_reimported"], false);
    assert_eq!(rollback["verify"]["equal"], true, "{rollback}");

    assert_eq!(
        std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record"),
        tag_record,
        "no tag record"
    );
    assert_eq!(
        std::fs::read(repo.data.join(RELEASE_REL)).expect("record"),
        release,
        "no release record"
    );
    let run_dir = run_dir_of(&repo, &result);
    let names = event_names(&run_dir);
    let started = position(&names, "rollback_started");
    assert!(
        started > position(&names, "tag_import_answered"),
        "{names:?}"
    );
    assert!(position(&names, "rollback_verified") > started, "{names:?}");
    assert_eq!(names.last().map(String::as_str), Some("finished"));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(run_dir.join("result.json")).expect("result.json"))
            .expect("json");
    assert_eq!(saved["outcome"], "tags_rolled_back");
    assert_eq!(saved["slug"], "deploy_rolled_back");
}

/// R3-RB, A1: the `MergeOverwrite` restore carries no value key, so a
/// runtime write made between the failure and the rollback survives it.
#[tokio::test]
async fn rollback_keeps_live_values() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        COUNT_DOC,
        FakeProjects::new()
            .with_tag_import_failure(2)
            .with_write_after_import(2, PROVIDER, "Line1/Count", "value", json!(55)),
    )
    .await;

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    let restore = &gateway.tag_import_bodies()[2]["tags"][0];
    assert_eq!(restore["name"], "Count");
    assert!(
        restore.get("value").is_none(),
        "no value is sent: {restore}"
    );
    assert_eq!(restore["defaultValue"], 1, "configuration is: {restore}");
    assert_eq!(restore["documentation"], "old");
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["value"], 55, "the runtime write survives");
    assert_eq!(count["documentation"], "old");
}

/// R3-RB, R3-CARRY: a change that added a property is undone by `Overwrite`
/// of the pre-image with the value carried from an export read right before
/// the restore, so the stand-in runtime write made after the failure
/// survives and the added property is gone.
#[tokio::test]
async fn rollback_carries_fresh_value() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        r#"{"dataType":"Int4","documentation":"old","tagType":"AtomicTag","tooltip":"tip","value":1,"valueSource":"memory"}"#,
        FakeProjects::new()
            .with_tag_import_failure(2)
            .with_write_after_import(2, PROVIDER, "Line1/Count", "value", json!(55)),
    )
    .await;
    assert_eq!(
        check(&repo, &gateway)
            .await
            .tags
            .expect("data.tags")
            .changed[0]
            .properties,
        ["tooltip"]
    );

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert_eq!(
        import_paths(&gateway),
        pairs(&[
            ("Line1", "MergeOverwrite"),
            ("Line2", "MergeOverwrite"),
            ("Line1", "Overwrite"),
        ])
    );
    let restore = &gateway.tag_import_bodies()[2]["tags"][0];
    assert_eq!(
        restore["value"], 55,
        "carried from the fresh export: {restore}"
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert!(
        count.get("tooltip").is_none(),
        "the property is gone: {count}"
    );
    assert_eq!(count["value"], 55, "the runtime write survives");
    assert_eq!(count["defaultValue"], 1);
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["carried"], json!(["[PlantA]Line1/Count"]));
    assert_eq!(rollback["carried_count"], 1);
    let run_dir = run_dir_of(&repo, &result);
    let names = event_names(&run_dir);
    let carried = names
        .iter()
        .rposition(|n| n == "tag_values_carried")
        .expect("a carry in the rollback");
    assert!(carried > position(&names, "rollback_started"), "{names:?}");
    let log = std::fs::read_to_string(run_dir.join("events.ndjson")).expect("events");
    assert!(!log.contains("\"value\""), "never a value in the events");
}

/// D-07: a red tag verify (another change landed on a node the run changed)
/// rolls the run back: the node goes back to its pre-image and the forward
/// verify stays in `data.tags.verify`.
#[tokio::test]
async fn verify_red_rolls_back() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(
        &repo,
        FakeProjects::new().with_write_after_import(
            1,
            PROVIDER,
            "Line1/Count",
            "documentation",
            json!("other"),
        ),
    )
    .await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert!(gateway.import_calls().is_empty());
    let data = tags_json(&result);
    assert_eq!(data["verify"]["equal"], false, "{data}");
    assert_eq!(data["rollback"]["cause"], "tag_verify_failed");
    assert_eq!(
        data["rollback"]["reimported"],
        json!(["[PlantA]Line1/Count"])
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old", "{count}");
    assert_eq!(count["value"], 9);
}

/// D-07: the tags were applied and the project reads back different: the
/// project is imported again from its pre-image and verified, the tags are
/// rolled back, and the run is `rolled_back` (exit 6) with no record and no
/// restore command.
#[tokio::test]
async fn project_unverified_rolls_back_both() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));
    gateway.mutate_child_after_import(PROJECT, 1);
    let release = std::fs::read(repo.data.join(RELEASE_REL)).expect("the release record");

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::RolledBack, "{result:?}");
    assert!(!result.ok);
    assert_eq!(result.slug.as_deref(), Some("deploy_rolled_back"));
    assert_eq!(result.failure_exit_code(), Some(6));
    assert!(result.restore_command.is_none());
    assert_eq!(
        gateway.import_calls(),
        [(PROJECT.to_string(), true), (PROJECT.to_string(), true)],
        "the artifact, then the pre-image"
    );
    let run_dir = run_dir_of(&repo, &result);
    let pre = std::fs::read(run_dir.join(format!("export/{PROJECT}.zip"))).expect("pre-image");
    let now = gateway.export_bytes(PROJECT).await.expect("export");
    assert_eq!(digest(&now), digest(&pre), "the project is its pre-image");
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old");
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["cause"], "project_unverified");
    assert_eq!(rollback["project_reimported"], true);
    assert_eq!(rollback["reimported"], json!(["[PlantA]Line1/Count"]));
    assert_eq!(
        std::fs::read(repo.data.join(RELEASE_REL)).expect("record"),
        release
    );
    let names = event_names(&run_dir);
    assert!(
        position(&names, "project_restored") > position(&names, "rollback_started"),
        "{names:?}"
    );
}

/// D-07: a refused project import after the tags were applied rolls the
/// tags back; the project did not change, so it is not imported again.
#[tokio::test]
async fn project_refused_rolls_back_tags() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(&repo, FakeProjects::new().with_project_import_denied()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::RolledBack, "{result:?}");
    assert_eq!(result.slug.as_deref(), Some("deploy_rolled_back"));
    assert_eq!(gateway.import_calls(), [(PROJECT.to_string(), true)]);
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["cause"], "project_import_failed");
    assert_eq!(rollback["project_reimported"], false);
    assert!(
        rollback["detail"]
            .as_str()
            .expect("detail")
            .contains("import_denied"),
        "{rollback}"
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old");
}

/// R3-DEL: a tag the run created in an existing provider cannot be removed.
/// Every other restore still runs, then the run ends `rollback_failed`
/// naming the created path and the run directory.
#[tokio::test]
async fn created_nodes_end_rollback_failed() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(
        &repo,
        FakeProjects::new().with_created_default_value(json!("seed-sentinel")),
    )
    .await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));

    let err = apply(&repo, &gateway)
        .await
        .expect_err("a created tag cannot be removed");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(
        text.contains("[PlantA]Line1/Speed (created by this run; this release has no tag delete)"),
        "{text}"
    );
    assert!(text.contains("tag_verify_failed"), "{text}");
    let run_dir = only_run_dir(&repo);
    let rel = run_dir
        .strip_prefix(&repo.repo)
        .expect("below the repo")
        .to_string_lossy()
        .into_owned();
    assert!(text.contains(&rel), "{text}");
    assert!(!text.contains("seed-sentinel"), "never a value: {text}");
    assert!(!text.contains("ign deploy rollback --run"), "{text}");
    assert!(gateway.import_calls().is_empty());
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old", "the other restore ran");
    assert!(
        live_node(&gateway, "Line1/Speed").await.is_some(),
        "it stays"
    );
    let names = event_names(&run_dir);
    assert!(names.iter().any(|n| n == "rollback_verified"), "{names:?}");
}

/// A provider the run created goes with the rollback, and the tags the run
/// put in it with it.
#[tokio::test]
async fn created_provider_rollback_deletes_provider() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = FakeProjects::new()
        .with_created_default_value(json!("seed-sentinel"))
        .with_project(PROJECT, None, false, &members(CODE_V2));
    let live = gateway.export_bytes(PROJECT).await.expect("export");
    repo.record("v0.9.0", &digest(&live));
    let mut input = repo.input();
    input.allow_first_change = Some("a new provider".to_string());
    let (loaded, prepared) = plan_with(&repo, &input, &gateway, Seams::default())
        .await
        .expect("the plan completes");

    let result = run_with(&repo, &input, &gateway, Seams::default(), &loaded, prepared)
        .await
        .expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert!(gateway.provider_record(PROVIDER).is_none(), "deleted");
    assert!(
        gateway
            .call_log()
            .iter()
            .any(|c| c == "tag-provider-delete:PlantA"),
        "{:?}",
        gateway.call_log()
    );
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["provider_deleted"], json!(["PlantA"]));
    assert_eq!(rollback["undeletable"], json!([]));
    assert!(rollback["verify"].is_null(), "no provider left to compare");
    assert!(gateway.import_calls().is_empty());
    let names = event_names(&run_dir_of(&repo, &result));
    assert!(
        names.iter().any(|n| n == "tag_provider_restored"),
        "{names:?}"
    );
}

/// T-26-42: a restore step that fails leaves the gateway half restored, so
/// the run ends `rollback_failed` naming what is left and the run directory.
#[tokio::test]
async fn rollback_failure_is_rollback_failed() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        COUNT_DOC,
        FakeProjects::new()
            .with_tag_import_failure(2)
            .with_tag_import_failure(3),
    )
    .await;

    let err = apply(&repo, &gateway)
        .await
        .expect_err("the restore failed");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    let text = err.to_string();
    assert!(text.contains("tag_step_failed"), "{text}");
    assert!(
        text.contains(
            "[PlantA]Line1 (MergeOverwrite, 1 node(s)): the restore import was not clean"
        ),
        "{text}"
    );
    assert!(
        text.contains("verify: [PlantA]Line1/Count (documentation)"),
        "{text}"
    );
    assert!(text.contains("export"), "the pre-images: {text}");
    assert_eq!(gateway.tag_import_calls().len(), 3);
    assert!(gateway.import_calls().is_empty());
}

/// D-13: a project the run created cannot be removed (the gateway surface has
/// no project delete): the tags are rolled back, a warning says the project
/// stays, and there is no restore command.
#[tokio::test]
async fn created_project_left_with_warning() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = FakeProjects::new().with_tag_tree(PROVIDER, live_count(json!([])));
    repo.tag_record(PROJECT, &["Line1"]);
    gateway.mutate_child_after_import(PROJECT, 1);

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.action, ProjectAction::Create);
    assert_eq!(result.outcome, ProjectOutcome::RolledBack, "{result:?}");
    assert!(result.restore_command.is_none());
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("was created by this run and stays")),
        "{:?}",
        result.warnings
    );
    assert_eq!(gateway.import_calls(), [(PROJECT.to_string(), false)]);
    assert!(gateway.has_project(PROJECT));
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old");
    assert_eq!(tags_json(&result)["rollback"]["project_reimported"], false);
}

/// K11 stays for a run that wrote no tag: the owned tags were already the
/// release, the project reads back different, and the run is
/// `imported_unverified` with its restore command and no automatic restore.
#[tokio::test]
async fn project_only_keeps_k11() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Keep", KEEP);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_line1(json!([keep_live("Keep")])));
    repo.tag_record(PROJECT, &["Line1", "Line1/Keep"]);
    gateway.mutate_child_after_import(PROJECT, 1);

    let result = apply(&repo, &gateway).await.expect("the run completes");
    assert_eq!(
        result.outcome,
        ProjectOutcome::ImportedUnverified,
        "{result:?}"
    );
    assert_eq!(result.slug.as_deref(), Some("deploy_project_unverified"));
    assert!(result.restore_command.is_some(), "{result:?}");
    assert_eq!(result.failure_exit_code(), Some(6));
    assert_eq!(gateway.import_calls(), [(PROJECT.to_string(), true)]);
    assert!(gateway.tag_import_calls().is_empty());
    assert!(tags_json(&result)["rollback"].is_null());
    let names = event_names(&run_dir_of(&repo, &result));
    assert!(!names.iter().any(|n| n == "rollback_started"), "{names:?}");
}

/// A batch that is not clean may have applied part of its nodes: the
/// rollback compares every node it named with the provider as it is now and
/// restores the ones that changed.
#[tokio::test]
async fn partial_batch_rolls_back_the_applied_nodes() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Other", X_DOC);
    let gateway = importing(&repo, FakeProjects::new().with_tag_import_partial(1, 1)).await;
    gateway.set_tag_tree(
        PROVIDER,
        live_count(
            json!([{"name":"Other","tagType":"AtomicTag","dataType":"Int4",
            "valueSource":"memory","value":4,"defaultValue":1,"documentation":"old"}]),
        ),
    );

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["reimported"], json!(["[PlantA]Line1/Count"]));
    for name in ["Count", "Other"] {
        let node = live_node(&gateway, &format!("Line1/{name}"))
            .await
            .expect("live");
        assert_eq!(node["documentation"], "old", "{name}: {node}");
    }
}

/// A new folder whose nested body is over the import cap: three tags of
/// about 200 KB below `Line1/Big`.
fn big_folder(repo: &Checkout) {
    for i in 0..3 {
        repo.tag(
            &format!("Line1/Big/T{i}"),
            &format!(
                r#"{{"dataType":"Int4","documentation":"{}","tagType":"AtomicTag","value":{i},"valueSource":"memory"}}"#,
                "d".repeat(200_000)
            ),
        );
    }
}

/// R3-BATCH (26-14 F1): the new folder goes alone at its parent path, then
/// its children are sent below it; no body is over the cap and the tags
/// verify.
#[tokio::test]
async fn a_folder_over_the_cap_is_sent_alone_then_filled() {
    let repo = checkout(&[PROVIDER]);
    big_folder(&repo);
    let gateway = importing(&repo, FakeProjects::new()).await;

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert!(result.ok, "{result:?}");
    assert_eq!(
        import_paths(&gateway),
        pairs(&[
            ("Line1", "Abort"),
            ("Line1/Big", "Abort"),
            ("Line1/Big", "Abort"),
        ])
    );
    let bodies = gateway.tag_import_bodies();
    assert_eq!(
        bodies[0],
        json!({"tags": [{"name": "Big", "tagType": "Folder"}]})
    );
    for body in &bodies {
        let bytes = serde_json::to_vec(body).expect("json").len();
        assert!(
            bytes <= ignition_core::deploy::tag_apply::IMPORT_BATCH_MAX_BYTES,
            "{bytes}"
        );
    }
    for i in 0..3 {
        let node = live_node(&gateway, &format!("Line1/Big/T{i}"))
            .await
            .expect("created");
        assert_eq!(node["value"], i, "{i}");
    }
    assert_eq!(tags_json(&result)["verify"]["equal"], true);
}

/// The first child batch of a split folder fails: the rollback names the
/// folder the run created, empty, and none of the children it never got.
#[tokio::test]
async fn a_split_folder_rollback_names_only_what_the_run_created() {
    let repo = checkout(&[PROVIDER]);
    big_folder(&repo);
    let gateway = importing(&repo, FakeProjects::new().with_tag_import_failure(2)).await;

    let err = apply(&repo, &gateway)
        .await
        .expect_err("a created folder cannot be removed");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    let text = err.to_string();
    // The failed import is named by its head; the rollback leaves exactly
    // the folder, none of the children it never got.
    assert!(
        text.contains("the tag import headed by [PlantA]Line1/Big/T0"),
        "{text}"
    );
    assert!(
        text.contains(
            "1 item(s) are left: [PlantA]Line1/Big (created by this run; this release has no \
             tag delete); the host journal"
        ),
        "{text}"
    );
    assert_eq!(
        import_paths(&gateway),
        pairs(&[("Line1", "Abort"), ("Line1/Big", "Abort")]),
        "nothing after the failure, and no restore import"
    );
    let folder = live_node(&gateway, "Line1/Big").await.expect("it stays");
    assert_eq!(folder["tagType"], "Folder");
    assert!(live_node(&gateway, "Line1/Big/T0").await.is_none());
}

/// 26-10 open point 4: a failure after the group commit puts the group units
/// back from the run's snapshot (the run record ends `rolled_back`), as
/// well as the tags.
#[tokio::test]
async fn groups_restored_after_a_later_failure() {
    let repo = checkout(&[PROVIDER]);
    let gateway = fast_group_rig(&repo).await;
    let gateway = gateway.with_tag_import_failure(1);
    let reader = repo.reader();
    let fake = FakeGateway::new();
    let deploy = fake.client("cap");
    let host = SpyHost::new(&repo, &gateway);
    let seams = Seams {
        tree: Some(&reader),
        apply_host: Some(&host),
        deploy: Some(&deploy),
    };
    let (loaded, prepared) = plan_with(&repo, &repo.input(), &gateway, seams)
        .await
        .expect("plans");

    let result = run_with(&repo, &repo.input(), &gateway, seams, &loaded, prepared)
        .await
        .expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["groups_restored"], true, "{rollback}");
    assert!(!repo.mode_unit("Fast", "config.json").exists(), "put back");
    assert!(
        !repo.mode_unit("Fast", "resource.json").exists(),
        "put back"
    );
    let run_id = host.specs.lock().expect("specs")[0].run_id.clone();
    let record = host
        .read_record(&run_id)
        .await
        .expect("readable")
        .expect("the run record");
    assert_eq!(record.state, RecordState::RolledBack);
    assert!(
        live_node(&gateway, "Line1/Speed").await.is_none(),
        "never applied"
    );
    let names = event_names(&run_dir_of(&repo, &result));
    assert!(
        position(&names, "tag_groups_restored") > position(&names, "rollback_started"),
        "{names:?}"
    );
    assert!(gateway.import_calls().is_empty());
}

/// 26-10 open point 4: a failure after a provider's settings were modified
/// puts them back to the saved record, with the signature of a fresh read.
#[tokio::test]
async fn provider_settings_restored_after_a_later_failure() {
    let repo = checkout(&[PROVIDER]);
    put(
        &repo.repo,
        "deploy/tag-providers/PlantA.json",
        SETTINGS_DESCRIBED.as_bytes(),
    );
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(&repo, FakeProjects::new().with_tag_import_failure(1)).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));
    assert_eq!(
        gateway
            .provider_record(PROVIDER)
            .expect("listed")
            .description(),
        None
    );

    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    let live = gateway.provider_record(PROVIDER).expect("listed");
    assert_eq!(live.description(), None, "put back");
    let rollback = &tags_json(&result)["rollback"];
    assert_eq!(rollback["provider_restored"], json!(["PlantA"]));
    assert_eq!(
        gateway.provider_modify_signatures().len(),
        2,
        "forward and back"
    );
    let names = event_names(&run_dir_of(&repo, &result));
    assert!(
        names.iter().any(|n| n == "tag_provider_restored"),
        "{names:?}"
    );
}

/// A run that is to create the owned provider (absent from `gateway`) and
/// add `Line1/Speed` to it, whose tag verify goes red (the seed check), so
/// the run rolls back. Returns the verdict or the error.
async fn creating_run_that_rolls_back(
    repo: &Checkout,
    gateway: FakeProjects,
) -> (FakeProjects, Result<DeployProjectsResult, CoreError>) {
    repo.tag("Line1/Speed", SPEED);
    let gateway = gateway
        .with_created_default_value(json!("seed-sentinel"))
        .with_project(PROJECT, None, false, &members(CODE_V2));
    let live = gateway.export_bytes(PROJECT).await.expect("export");
    repo.record("v0.9.0", &digest(&live));
    let mut input = repo.input();
    input.allow_first_change = Some("a new provider".to_string());
    let (loaded, prepared) = plan_with(repo, &input, &gateway, Seams::default())
        .await
        .expect("the plan completes");
    let outcome = run_with(repo, &input, &gateway, Seams::default(), &loaded, prepared).await;
    (gateway, outcome)
}

fn deleted(gateway: &FakeProjects) -> bool {
    gateway
        .call_log()
        .iter()
        .any(|c| c.starts_with("tag-provider-delete:"))
}

/// Security review (26-11): a create refused because another client created
/// the provider first never makes it this run's: the rollback leaves it.
#[tokio::test]
async fn provider_created_by_another_client_is_left_alone() {
    let repo = checkout(&[PROVIDER]);
    let (gateway, outcome) =
        creating_run_that_rolls_back(&repo, FakeProjects::new().with_provider_create_conflict())
            .await;
    let before = gateway
        .provider_record(PROVIDER)
        .expect("the other client's");
    let result = outcome.expect("a verdict: the run changed nothing");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert!(!deleted(&gateway), "{:?}", gateway.call_log());
    let after = gateway.provider_record(PROVIDER).expect("still there");
    assert_eq!(after.signature, before.signature, "untouched");
    assert_eq!(after.description(), Some("another client's provider"));
    assert_eq!(
        tags_json(&result)["rollback"]["provider_deleted"],
        json!([])
    );
    assert!(gateway.tag_import_calls().is_empty());
}

/// Security review (26-11): a create whose answer was lost is never deleted
/// on a fresh read; a provider whose settings are not what the run sent is
/// left, and the run ends `rollback_failed` naming it.
#[tokio::test]
async fn ambiguous_create_with_other_settings_is_not_deleted() {
    let repo = checkout(&[PROVIDER]);
    let (gateway, outcome) = creating_run_that_rolls_back(
        &repo,
        FakeProjects::new().with_provider_create_lost_answer(true),
    )
    .await;
    let err = outcome.expect_err("it cannot be proven to be this run's");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    let text = err.to_string();
    assert!(
        text.contains("[PlantA] the provider this run created"),
        "{text}"
    );
    assert!(!deleted(&gateway), "{:?}", gateway.call_log());
    assert!(gateway.provider_record(PROVIDER).is_some(), "left in place");
}

/// A create whose answer was lost but whose provider holds exactly what the
/// run sent, under the signature read right after it, is this run's: the
/// rollback deletes it with that signature.
#[tokio::test]
async fn ambiguous_create_with_the_sent_settings_is_deleted() {
    let repo = checkout(&[PROVIDER]);
    let (gateway, outcome) = creating_run_that_rolls_back(
        &repo,
        FakeProjects::new().with_provider_create_lost_answer(false),
    )
    .await;
    let result = outcome.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert!(gateway.provider_record(PROVIDER).is_none(), "deleted");
    assert_eq!(
        tags_json(&result)["rollback"]["provider_deleted"],
        json!(["PlantA"])
    );
}

/// Security review (26-11): the delete uses the signature read right after
/// the run's own create. A provider another client changed since is left,
/// and the run ends `rollback_failed` naming it.
#[tokio::test]
async fn created_provider_changed_by_another_client_is_not_deleted() {
    let repo = checkout(&[PROVIDER]);
    let (gateway, outcome) = creating_run_that_rolls_back(
        &repo,
        FakeProjects::new().with_provider_resigned_after_import(1, PROVIDER),
    )
    .await;
    let err = outcome.expect_err("the provider changed since the create");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    assert!(
        err.to_string()
            .contains("[PlantA] the provider this run created"),
        "{err}"
    );
    assert!(!deleted(&gateway), "{:?}", gateway.call_log());
    assert!(gateway.provider_record(PROVIDER).is_some(), "left in place");
}

// ---------------------------------------------------------------------------
// 26-12: the host journal and pre-image of a tag-owning run (TAGDEP-10).
// ---------------------------------------------------------------------------

use ignition_core::deploy::project_run::{JournalState, PreImageRun, ProjectJournal};
use ignition_core::deploy::transport::projects::{
    ProjectRunWriter, ReleaseRecordWriter, TagRecordWriter,
};

const JOURNAL_REL: &str = "ign-deploy/capgw/projects/CapApp/journal.json";
const PRE_IMAGE_REL: &str = "ign-deploy/capgw/projects/CapApp/pre-image";
const OLD_RUN: &str = "20261101T000000Z-dead";

/// A host journal of run `run_id` in `state` on the master's host.
fn put_journal(repo: &Checkout, run_id: &str, state: JournalState) {
    let journal = ProjectJournal {
        schema_version: 1,
        gateway: "capgw".to_string(),
        project: PROJECT.to_string(),
        run_id: run_id.to_string(),
        state,
        started_at: "2026-11-01T00:00:00Z".to_string(),
        providers: vec![PROVIDER.to_string()],
    };
    put(&repo.data, JOURNAL_REL, &journal.to_bytes());
}

/// The host journal on the master's host, parsed.
fn host_journal(repo: &Checkout) -> Option<ProjectJournal> {
    let bytes = std::fs::read(repo.data.join(JOURNAL_REL)).ok()?;
    Some(ProjectJournal::parse(&bytes, JOURNAL_REL, "capgw", PROJECT).expect("a journal"))
}

/// Whether a fake gateway call is a write.
fn is_write(call: &str) -> bool {
    [
        "tag-import:",
        "import:",
        "tag-provider-create",
        "tag-provider-modify",
        "tag-provider-delete",
    ]
    .iter()
    .any(|w| call.starts_with(w))
}

/// Every event of a run as `(name, line)`.
fn event_lines(run_dir: &Path) -> Vec<(String, Value)> {
    std::fs::read_to_string(run_dir.join("events.ndjson"))
        .expect("events")
        .lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).expect("an event line");
            (v["event"].as_str().expect("name").to_string(), v)
        })
        .collect()
}

/// The host writers of a run over the checkout's data directory, logging
/// every host write with how many gateway calls had been made before it. With
/// `fail_pre_image` every pre-image write fails.
struct HostLog<'a> {
    inner: LocalFsTransport,
    gateway: &'a FakeProjects,
    log: Mutex<Vec<(String, usize)>>,
    fail_pre_image: bool,
}

impl<'a> HostLog<'a> {
    fn new(repo: &Checkout, gateway: &'a FakeProjects, fail_pre_image: bool) -> Self {
        HostLog {
            inner: repo.host(),
            gateway,
            log: Mutex::new(Vec::new()),
            fail_pre_image,
        }
    }

    fn note(&self, what: String) {
        let calls = self.gateway.call_log().len();
        self.log.lock().expect("log").push((what, calls));
    }

    fn entries(&self) -> Vec<(String, usize)> {
        self.log.lock().expect("log").clone()
    }
}

#[async_trait::async_trait]
impl ProjectRunWriter for HostLog<'_> {
    async fn write_journal(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        self.note("journal".to_string());
        self.inner.write_journal(gateway, project, bytes).await
    }

    async fn clear_journal(&self, gateway: &str, project: &str) -> Result<(), CoreError> {
        self.note("clear".to_string());
        self.inner.clear_journal(gateway, project).await
    }

    async fn write_pre_image(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        if self.fail_pre_image {
            return Err(CoreError::TransportFailed {
                operation: "write".to_string(),
                node: "nodes.master".to_string(),
                detail: "the host refused the pre-image".to_string(),
            });
        }
        self.note(format!("pre:{name}"));
        self.inner
            .write_pre_image(gateway, project, run_id, name, bytes)
            .await
    }
}

#[async_trait::async_trait]
impl ReleaseRecordWriter for HostLog<'_> {
    async fn write_release_record(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        self.note("release-record".to_string());
        self.inner
            .write_release_record(gateway, project, bytes)
            .await
    }
}

#[async_trait::async_trait]
impl TagRecordWriter for HostLog<'_> {
    async fn write_tag_record(
        &self,
        gateway: &str,
        provider: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        self.note(format!("tag-record:{provider}"));
        self.inner.write_tag_record(gateway, provider, bytes).await
    }
}

/// Plan and run `repo` with every host write going through `log`.
async fn run_logged(
    repo: &Checkout,
    gateway: &FakeProjects,
    log: &HostLog<'_>,
) -> Result<DeployProjectsResult, CoreError> {
    let input = repo.input();
    let (loaded, prepared) = plan(repo, gateway).await.expect("the plan completes");
    let host = repo.host();
    deploy_projects_run(
        prepared,
        &loaded,
        &input,
        &surfaces(gateway, &host, Seams::default()),
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: Some(log),
            tag_record_writer: Some(log),
            run_writer: Some(log),
            designers: DesignersPolicy::default(),
            children_settle: Duration::from_millis(50),
            children_poll: Duration::from_millis(5),
            tag_verify_settle: Duration::from_millis(500),
            tag_verify_poll: Duration::from_millis(5),
            backup: None,
            backup_policy: BackupPolicy::default(),
        },
        &NoProjectsObserver,
    )
    .await
}

/// T-26-44: a host journal of an earlier run refuses the apply before any
/// write, naming the run and the exact command that undoes it; no run
/// directory is made and the journal stays.
#[tokio::test]
async fn journal_present_refuses_the_apply_with_zero_writes() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    put_journal(&repo, OLD_RUN, JournalState::Applying);
    let before = std::fs::read(repo.data.join(JOURNAL_REL)).expect("journal");

    let err = apply(&repo, &gateway).await.expect_err("refused");
    assert_eq!(err.code(), "deploy_journal_present", "{err}");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(
        text.contains(&format!(
            "ign deploy projects --env dev --rollback-run {OLD_RUN}"
        )),
        "{text}"
    );
    assert!(text.contains(JOURNAL_REL), "{text}");
    assert!(!text.contains("ign deploy rollback --run"), "{text}");
    assert!(
        gateway.call_log().iter().all(|c| !is_write(c)),
        "{:?}",
        gateway.call_log()
    );
    assert!(!repo.repo.join(".ign").exists(), "no run directory");
    assert_eq!(
        std::fs::read(repo.data.join(JOURNAL_REL)).expect("journal"),
        before,
        "the journal stays as it was"
    );
}

/// The check reports the journal in `data.tags.journal` with a warning
/// naming the command, and writes nothing.
#[tokio::test]
async fn journal_present_is_reported_by_the_check() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    put_journal(&repo, OLD_RUN, JournalState::RollingBack);

    let checked = check(&repo, &gateway).await;
    let journal = tags_json(&checked)["journal"].clone();
    assert_eq!(
        journal,
        json!({"run_id": OLD_RUN, "state": "rolling_back", "started_at": "2026-11-01T00:00:00Z"})
    );
    assert!(
        checked.warnings.iter().any(|w| w.contains(&format!(
            "ign deploy projects --env dev --rollback-run {OLD_RUN}"
        ))),
        "{:?}",
        checked.warnings
    );
    assert!(gateway.call_log().iter().all(|c| !is_write(c)));
    assert!(!repo.repo.join(".ign").exists());
    assert!(host_journal(&repo).is_some(), "the check leaves it");

    // without a journal the key is null
    std::fs::remove_file(repo.data.join(JOURNAL_REL)).expect("rm");
    assert!(tags_json(&check(&repo, &gateway).await)["journal"].is_null());
}

/// A green run writes the pre-image files, then `run.json`, then the
/// journal, all before its first gateway write; after every gateway write
/// the records, then the journal clear. `run.json` names the run and hashes
/// each file of the set.
#[tokio::test]
async fn journal_green_run_writes_the_host_copy_first_and_clears_it_last() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    let log = HostLog::new(&repo, &gateway, false);

    let before = gateway.call_log().len();
    let result = run_logged(&repo, &gateway, &log).await.expect("green");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    let entries = log.entries();
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    let run_json = position_of(&names, "pre:run.json");
    let journal = position_of(&names, "journal");
    assert!(
        names[..run_json].iter().all(|n| n.starts_with("pre:")),
        "{names:?}"
    );
    for file in [
        "pre:project.zip",
        "pre:tags-PlantA.json",
        "pre:record-PlantA.json",
    ] {
        assert!(names[..run_json].contains(&file), "{file}: {names:?}");
    }
    assert_eq!(journal, run_json + 1, "{names:?}");
    assert_eq!(
        &names[journal + 1..],
        ["release-record", "tag-record:PlantA", "clear"],
        "{names:?}"
    );
    let log_calls = gateway.call_log();
    let first_write = log_calls
        .iter()
        .enumerate()
        .skip(before)
        .find(|(_, c)| is_write(c))
        .map(|(i, _)| i)
        .expect("a gateway write");
    let last_write = log_calls
        .iter()
        .rposition(|c| is_write(c))
        .expect("a gateway write");
    assert!(
        entries[journal].1 <= first_write,
        "{entries:?} {log_calls:?}"
    );
    for (name, calls) in &entries[journal + 1..] {
        assert!(*calls > last_write, "{name} before a gateway write");
    }

    assert!(host_journal(&repo).is_none(), "cleared");
    let pre = repo.data.join(PRE_IMAGE_REL);
    let run_bytes = std::fs::read(pre.join("run.json")).expect("run.json");
    let set = PreImageRun::parse(&run_bytes, "run.json", "capgw", PROJECT).expect("a set");
    assert_eq!(Some(set.run_id.as_str()), result.run_id.as_deref());
    assert_eq!(set.project_action, "import");
    assert_eq!(set.providers[0].added, ["Line1/Speed"]);
    for (name, sha) in &set.files {
        let bytes = std::fs::read(pre.join(name)).expect(name);
        assert_eq!(
            &ignition_core::deploy::tree::sha256_hex(&bytes),
            sha,
            "{name}"
        );
    }
    let events: Vec<String> = event_lines(&run_dir_of(&repo, &result))
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        position(&events, "host_pre_image_written") < position(&events, "journal_written")
            && position(&events, "journal_written") < position(&events, "tag_import_sent")
            && position(&events, "tag_record_written") < position(&events, "journal_cleared"),
        "{events:?}"
    );
}

fn position_of(names: &[&str], wanted: &str) -> usize {
    names
        .iter()
        .position(|n| *n == wanted)
        .unwrap_or_else(|| panic!("{wanted} is not in {names:?}"))
}

/// A rolled-back run switches the journal to `rolling_back` when its
/// rollback starts and clears it once the rollback verified.
#[tokio::test]
async fn journal_cleared_after_a_rolled_back_run() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        COUNT_DOC,
        FakeProjects::new().with_tag_import_failure(2),
    )
    .await;
    let result = apply(&repo, &gateway).await.expect("a verdict");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert!(host_journal(&repo).is_none(), "cleared after the verify");
    let events = event_lines(&run_dir_of(&repo, &result));
    let states: Vec<&str> = events
        .iter()
        .filter(|(n, _)| n == "journal_written")
        .map(|(_, v)| v["state"].as_str().expect("state"))
        .collect();
    assert_eq!(states, ["applying", "rolling_back"]);
    let names: Vec<String> = events.into_iter().map(|(n, _)| n).collect();
    assert!(
        position(&names, "rollback_verified") < position(&names, "journal_cleared"),
        "{names:?}"
    );
}

/// `rollback_failed` leaves the journal, `rolling_back`, and the message
/// names the command that retries the undo from the host copy.
#[tokio::test]
async fn journal_left_after_rollback_failed() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(
        &repo,
        FakeProjects::new().with_created_default_value(json!("seed-sentinel")),
    )
    .await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));

    let err = apply(&repo, &gateway).await.expect_err("rollback_failed");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    let journal = host_journal(&repo).expect("the journal stays");
    assert_eq!(journal.state, JournalState::RollingBack);
    let run_dir = only_run_dir(&repo);
    let run_id = run_dir
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(journal.run_id, run_id);
    let text = err.to_string();
    assert!(
        text.contains(&format!(
            "ign deploy projects --env dev --rollback-run {run_id}"
        )),
        "{text}"
    );
    assert!(text.contains(JOURNAL_REL), "{text}");
    assert!(!text.contains("seed-sentinel"), "never a value: {text}");
}

/// A host pre-image write that fails stops the run before any gateway
/// write; no journal is left.
#[tokio::test]
async fn journal_host_pre_image_failure_writes_nothing_to_the_gateway() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    let log = HostLog::new(&repo, &gateway, true);
    let tag_record = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record");

    let err = run_logged(&repo, &gateway, &log)
        .await
        .expect_err("the host copy failed");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(
        gateway.call_log().iter().all(|c| !is_write(c)),
        "{:?}",
        gateway.call_log()
    );
    assert!(host_journal(&repo).is_none());
    assert!(log.entries().is_empty(), "{:?}", log.entries());
    assert_eq!(
        std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record"),
        tag_record
    );
}

// ---------------------------------------------------------------------------
// 26-12: `--rollback-run` from the host copy (TAGDEP-10, T-26-45).
// ---------------------------------------------------------------------------

use ignition_core::actions::deploy_projects::{
    deploy_projects_rollback_run, load_project_for_rollback,
};

/// `--rollback-run run_id` (with `check`) of `repo`, as a fresh process on
/// another runner would make it: the checkout and the target, nothing else.
async fn rollback_run(
    repo: &Checkout,
    gateway: &dyn ProjectGateway,
    run_id: &str,
    check: bool,
) -> Result<DeployProjectsResult, CoreError> {
    let input = repo.input();
    let loaded = load_project_for_rollback(&input).expect("the checkout loads");
    let host = repo.host();
    deploy_projects_rollback_run(
        &loaded,
        &input,
        &surfaces(gateway, &host, Seams::default()),
        &ProjectsRunOptions {
            repo: input.repo.clone(),
            targets_root: input.targets.clone(),
            target_file: loaded.target_file.clone(),
            master: loaded.target.nodes.master.clone(),
            record_writer: None,
            tag_record_writer: None,
            run_writer: None,
            designers: DesignersPolicy::default(),
            children_settle: Duration::from_millis(50),
            children_poll: Duration::from_millis(5),
            tag_verify_settle: Duration::from_millis(500),
            tag_verify_poll: Duration::from_millis(5),
            backup: None,
            backup_policy: BackupPolicy::default(),
        },
        run_id,
        check,
        &NoProjectsObserver,
    )
    .await
}

/// The run directories of the repo.
fn run_dirs(repo: &Checkout) -> usize {
    std::fs::read_dir(repo.repo.join(".ign/deploy/capgw")).map_or(0, |d| d.count())
}

/// A green run that changed a node and imported the project is undone from
/// the host copy: the tag goes back by `MergeOverwrite` of its stripped
/// pre-image (no value sent, the live value stays), the project is imported
/// again from its pre-image, the tag record before the run is put back, the
/// journal is cleared, and the verdict is `rolled_back`, exit 6.
#[tokio::test]
async fn rollback_run_restores_a_green_run_that_changed_nodes() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));
    let pre_project = digest(&gateway.export_bytes(PROJECT).await.expect("export"));
    let pre_record = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("the old record");

    let applied = apply(&repo, &gateway).await.expect("green");
    assert_eq!(applied.outcome, ProjectOutcome::Applied, "{applied:?}");
    let run_id = applied.run_id.clone().expect("a run id");
    assert_ne!(
        std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record"),
        pre_record
    );
    gateway.set_tag_tree(PROVIDER, {
        let mut tree = live_count(json!([]));
        // a runtime write after the run
        tree["tags"][1]["tags"][0]["value"] = json!(41);
        tree["tags"][1]["tags"][0]["documentation"] = json!("new");
        tree
    });
    let sent_before = gateway.tag_import_bodies().len();

    let undo = rollback_run(&repo, &gateway, &run_id, false)
        .await
        .expect("a verdict");
    assert_eq!(undo.outcome, ProjectOutcome::RolledBack, "{undo:?}");
    assert!(!undo.ok);
    assert_eq!(undo.slug.as_deref(), Some("deploy_rolled_back"));
    assert_eq!(undo.failure_exit_code(), Some(6));
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old", "{count}");
    assert_eq!(count["value"], 41, "the live value stays");
    let bodies = gateway.tag_import_bodies();
    for body in &bodies[sent_before..] {
        assert!(
            !body.to_string().contains("\"value\""),
            "no pre-run value is written: {body}"
        );
    }
    assert_eq!(
        digest(&gateway.export_bytes(PROJECT).await.expect("export")),
        pre_project,
        "the project is its pre-image"
    );
    assert_eq!(
        std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record"),
        pre_record,
        "the tag record before the run is back"
    );
    assert!(host_journal(&repo).is_none(), "cleared");
    let rollback = &tags_json(&undo)["rollback"];
    assert_eq!(rollback["cause"], "rollback_run");
    assert_eq!(rollback["reimported"], json!(["[PlantA]Line1/Count"]));
    assert_eq!(rollback["project_reimported"], true);
    assert_eq!(rollback["verify"]["equal"], true, "{rollback}");
    let undo_dir = run_dir_of(&repo, &undo);
    assert_ne!(undo.run_id.as_deref(), Some(run_id.as_str()));
    let names: Vec<String> = event_lines(&undo_dir).into_iter().map(|(n, _)| n).collect();
    assert!(
        position(&names, "journal_written") < position(&names, "rollback_started")
            && position(&names, "tag_record_written") < position(&names, "journal_cleared"),
        "{names:?}"
    );
    let saved: Value =
        serde_json::from_slice(&std::fs::read(undo_dir.join("run.json")).expect("run.json"))
            .expect("json");
    assert_eq!(saved["kind"], "rollback_run");
    assert_eq!(saved["target_run_id"], run_id.as_str());
}

/// R3-DEL: a green run that created a tag in an existing provider is undone
/// as far as it can be (the changed tag and the project restored), then ends
/// `rollback_failed` naming the created tag; the journal stays.
#[tokio::test]
async fn rollback_run_of_a_run_that_created_nodes_ends_rollback_failed() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));
    let pre_project = digest(&gateway.export_bytes(PROJECT).await.expect("export"));
    let applied = apply(&repo, &gateway).await.expect("green");
    assert_eq!(applied.outcome, ProjectOutcome::Applied, "{applied:?}");
    let run_id = applied.run_id.clone().expect("a run id");
    let run_record = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record");

    let err = rollback_run(&repo, &gateway, &run_id, false)
        .await
        .expect_err("a created tag cannot be removed");
    assert_eq!(err.code(), "rollback_failed", "{err}");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(
        text.contains("[PlantA]Line1/Speed (created by this run; this release has no tag delete)"),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "ign deploy projects --env dev --rollback-run {run_id}"
        )),
        "{text}"
    );
    assert!(text.contains("pre-image"), "{text}");
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old", "the other restore ran");
    assert!(
        live_node(&gateway, "Line1/Speed").await.is_some(),
        "it stays"
    );
    assert_eq!(
        digest(&gateway.export_bytes(PROJECT).await.expect("export")),
        pre_project,
        "the project restore ran"
    );
    let journal = host_journal(&repo).expect("the journal stays");
    assert_eq!(journal.state, JournalState::RollingBack);
    assert_eq!(journal.run_id, run_id);
    assert_eq!(
        std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record"),
        run_record,
        "records go back only after a green undo"
    );
    // the journal now refuses the next deploy, naming the same command
    let next = apply(&repo, &gateway).await.expect_err("refused");
    assert_eq!(next.code(), "deploy_journal_present");
}

/// Only the latest run's pre-image is kept: another run id, or a host with
/// no pre-image, is `deploy_run_invalid` with nothing written and no run
/// directory made.
#[tokio::test]
async fn rollback_run_of_another_run_is_refused() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));
    let err = rollback_run(&repo, &gateway, OLD_RUN, false)
        .await
        .expect_err("no pre-image yet");
    assert_eq!(err.code(), "deploy_run_invalid", "{err}");
    assert!(err.to_string().contains("pre-image/run.json"), "{err}");

    let applied = apply(&repo, &gateway).await.expect("green");
    let run_id = applied.run_id.clone().expect("a run id");
    let calls = gateway.call_log().len();
    let err = rollback_run(&repo, &gateway, OLD_RUN, false)
        .await
        .expect_err("not the latest run");
    assert_eq!(err.code(), "deploy_run_invalid", "{err}");
    assert_eq!(err.exit_code(), 6);
    let text = err.to_string();
    assert!(text.contains(&run_id) && text.contains(OLD_RUN), "{text}");
    assert!(
        gateway.call_log()[calls..].iter().all(|c| !is_write(c)),
        "{:?}",
        gateway.call_log()
    );
    assert_eq!(run_dirs(&repo), 1, "no run directory for a refusal");
    assert!(host_journal(&repo).is_none());

    let err = rollback_run(&repo, &gateway, "../x", false)
        .await
        .expect_err("a malformed run id");
    assert_eq!(err.code(), "invalid_input");
}

/// `--check --rollback-run` reports what the undo would restore, the nodes
/// it could not remove, and writes nothing to the gateway or the host.
#[tokio::test]
async fn rollback_run_check_writes_nothing() {
    let repo = checkout(&[PROVIDER]);
    repo.tag("Line1/Count", COUNT_DOC);
    repo.tag("Line1/Speed", SPEED);
    let gateway = importing(&repo, FakeProjects::new()).await;
    gateway.set_tag_tree(PROVIDER, live_count(json!([])));
    let applied = apply(&repo, &gateway).await.expect("green");
    let run_id = applied.run_id.clone().expect("a run id");
    let gateway = ForbidProjectWrites(gateway);
    let host_before = files_below(&repo.data);

    let checked = rollback_run(&repo, &gateway, &run_id, true)
        .await
        .expect("the check completes");
    assert_eq!(checked.outcome, ProjectOutcome::Checked);
    assert!(checked.check && checked.ok && checked.would_change);
    let rollback = &tags_json(&checked)["rollback"];
    assert_eq!(rollback["cause"], "rollback_run");
    assert_eq!(rollback["reimported"], json!(["[PlantA]Line1/Count"]));
    assert_eq!(rollback["undeletable"], json!(["[PlantA]Line1/Speed"]));
    assert_eq!(rollback["undeletable_count"], 1);
    assert_eq!(rollback["project_reimported"], true);
    assert!(
        checked
            .warnings
            .iter()
            .any(|w| w.contains("would end rollback_failed")),
        "{:?}",
        checked.warnings
    );
    assert_eq!(
        files_below(&repo.data),
        host_before,
        "the host is untouched"
    );
    assert_eq!(run_dirs(&repo), 1, "no run directory for a check");
    assert_eq!(
        live_node(&gateway.0, "Line1/Count").await.expect("live")["documentation"],
        "new"
    );
}

/// An interrupted run (journal `applying`, its second batch never sent, no
/// record written) is undone from the host copy by another runner: only the
/// node that changed is sent back, the project is restored, the journal is
/// cleared.
#[tokio::test]
async fn rollback_run_restores_an_interrupted_run() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(&repo, COUNT_DOC, FakeProjects::new()).await;
    let pre_project = digest(&gateway.export_bytes(PROJECT).await.expect("export"));
    let pre_record = std::fs::read(repo.data.join(TAG_RECORD_REL)).expect("record");
    let pre_release = std::fs::read(repo.data.join(RELEASE_REL)).expect("release");
    let applied = apply(&repo, &gateway).await.expect("green");
    let run_id = applied.run_id.clone().expect("a run id");

    // The runner died after the first batch: Line2/X never changed, no
    // record was written, the journal still says applying.
    gateway.set_tag_tree(PROVIDER, two_lines(json!({"documentation": "new"})));
    put(&repo.data, TAG_RECORD_REL, &pre_record);
    put(&repo.data, RELEASE_REL, &pre_release);
    put_journal(&repo, &run_id, JournalState::Applying);
    let sent_before = gateway.tag_import_calls().len();

    let undo = rollback_run(&repo, &gateway, &run_id, false)
        .await
        .expect("a verdict");
    assert_eq!(undo.outcome, ProjectOutcome::RolledBack, "{undo:?}");
    assert_eq!(
        gateway.tag_import_calls()[sent_before..]
            .iter()
            .map(|(_, path, policy)| (path.clone(), policy.clone()))
            .collect::<Vec<_>>(),
        pairs(&[("Line1", "MergeOverwrite")]),
        "the batch that never landed is not sent"
    );
    let count = live_node(&gateway, "Line1/Count").await.expect("live");
    assert_eq!(count["documentation"], "old");
    assert_eq!(count["value"], 9);
    let x = live_node(&gateway, "Line2/X").await.expect("live");
    assert_eq!(x["documentation"], "old");
    assert_eq!(
        digest(&gateway.export_bytes(PROJECT).await.expect("export")),
        pre_project
    );
    assert!(host_journal(&repo).is_none(), "cleared");
    assert_eq!(tags_json(&undo)["journal"]["state"], "applying");
}

// -- The backup of a pair (26-13, TAGDEP-08 and TAGDEP-10 pair half) ---------

use ignition_core::client::redundancy::RedundancyStatusWire;
use ignition_core::deploy::pair::UnsyncedReason;

/// How [`FakeBackup`] answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BackupShape {
    /// The master's configuration, in the pair capture's shape.
    Synced,
    /// The owned providers as they were before the run, whatever happens.
    Stale,
    /// Stale until the master was asked to resync, then synced.
    StaleUntilResync,
    /// Not linked to its peer.
    Unlinked,
    /// Its tag export cannot be read.
    ExportFails,
}

/// A backup node over the master's exports. Its export is the master's
/// with every AtomicTag `value`, `timestamp` and `quality` removed outside
/// `_types_` and every `tags` array reversed: the F-P2 and F-P1 shape of the
/// pair capture. Only reads exist on [`BackupTags`]; the fake records each.
struct FakeBackup<'a> {
    master: &'a FakeProjects,
    master_deploy: &'a FakeGateway,
    shape: BackupShape,
    exhausted: bool,
    stale: BTreeMap<String, Vec<u8>>,
    reads: Mutex<Vec<String>>,
}

use std::collections::BTreeMap;

/// `node` as the backup exports it (F-P2, F-P1).
fn backup_shape(node: &mut Value, in_types: bool) {
    let in_types = in_types || node["name"] == "_types_";
    if !in_types
        && node["tagType"] == "AtomicTag"
        && let Some(object) = node.as_object_mut()
    {
        for key in ["value", "timestamp", "quality"] {
            object.remove(key);
        }
    }
    if let Some(children) = node.get_mut("tags").and_then(Value::as_array_mut) {
        children.reverse();
        for child in children {
            backup_shape(child, in_types);
        }
    }
}

impl<'a> FakeBackup<'a> {
    /// A backup of `master` answering `shape`; a stale one holds the master's
    /// owned providers as they are now.
    async fn new(
        master: &'a FakeProjects,
        master_deploy: &'a FakeGateway,
        shape: BackupShape,
    ) -> FakeBackup<'a> {
        let mut stale = BTreeMap::new();
        stale.insert(
            PROVIDER.to_string(),
            master
                .tag_export(PROVIDER)
                .await
                .expect("the master export"),
        );
        FakeBackup {
            master,
            master_deploy,
            shape,
            exhausted: false,
            stale,
            reads: Mutex::new(Vec::new()),
        }
    }

    fn reads(&self) -> Vec<String> {
        self.reads.lock().expect("reads").clone()
    }
}

#[async_trait::async_trait]
impl BackupTags for FakeBackup<'_> {
    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
        self.reads
            .lock()
            .expect("reads")
            .push(format!("export:{provider}"));
        let bytes = match self.shape {
            BackupShape::ExportFails => {
                return Err(CoreError::Network {
                    url: "http://backup.invalid/data/api/v1/tags/export".to_string(),
                    source: None,
                    observation: None,
                });
            }
            BackupShape::Stale | BackupShape::Unlinked => self.stale[provider].clone(),
            BackupShape::StaleUntilResync if self.master_deploy.resyncs() == 0 => {
                self.stale[provider].clone()
            }
            BackupShape::Synced | BackupShape::StaleUntilResync => {
                self.master.tag_export(provider).await?
            }
        };
        let mut tree: Value = serde_json::from_slice(&bytes).expect("an export");
        backup_shape(&mut tree, false);
        Ok(serde_json::to_vec(&tree).expect("json"))
    }

    async fn status(&self) -> Result<RedundancyStatusWire, CoreError> {
        self.reads.lock().expect("reads").push("status".to_string());
        Ok(serde_json::from_value(json!({
            "role": "Backup", "activityLevel": "Cold", "projectState": "Good",
            "peerConnected": self.shape != BackupShape::Unlinked,
            "syncPending": false, "lastSyncTimestamp": 1794179312498i64
        }))
        .expect("a status"))
    }

    async fn sync_exhausted(&self, _since_ms: i64) -> Result<bool, CoreError> {
        self.reads.lock().expect("reads").push("logs".to_string());
        Ok(self.exhausted)
    }
}

/// A backup wait of 60 ms per budget, polled every 5 ms.
fn fast_backup() -> BackupPolicy {
    BackupPolicy {
        budget: Duration::from_millis(60),
        interval: Duration::from_millis(5),
    }
}

/// Plan and run `repo` against `gateway` with the master's deploy API
/// `deploy` and the backup `backup`, waited for with `policy`.
async fn apply_paired(
    repo: &Checkout,
    gateway: &FakeProjects,
    deploy: &dyn DeployApi,
    backup: &FakeBackup<'_>,
    policy: BackupPolicy,
) -> Result<DeployProjectsResult, CoreError> {
    let input = repo.input();
    let seams = Seams {
        deploy: Some(deploy),
        ..Seams::default()
    };
    let (loaded, prepared) = plan_with(repo, &input, gateway, seams)
        .await
        .expect("the plan completes");
    run_paired(
        repo,
        &input,
        gateway,
        seams,
        &loaded,
        prepared,
        Some((backup as &dyn BackupTags, policy)),
    )
    .await
}

/// The repo adds `Line1/Speed` to a gateway that does not hold it.
async fn adding_speed(repo: &Checkout) -> FakeProjects {
    repo.tag("Line1/Speed", SPEED);
    importing(repo, FakeProjects::new()).await
}

fn backup_json(result: &DeployProjectsResult) -> Value {
    tags_json(result)["backup"].clone()
}

#[tokio::test]
async fn backup_converges_after_a_green_tag_run() {
    let repo = checkout(&[PROVIDER]);
    let gateway = adding_speed(&repo).await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let backup = FakeBackup::new(&gateway, &fake, BackupShape::Synced).await;

    let result = apply_paired(&repo, &gateway, &deploy, &backup, fast_backup())
        .await
        .expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    assert!(result.ok && result.slug.is_none());
    assert_eq!(result.failure_exit_code(), None);
    let report = backup_json(&result);
    assert_eq!(report["converged"], true, "{report}");
    assert_eq!(report["reason"], Value::Null);
    assert_eq!(report["resync_sent"], false);
    assert_eq!(report["providers"], json!([PROVIDER]));
    assert_eq!(report["differing"], json!([]));
    assert_eq!(fake.resyncs(), 0, "no resync was needed");
    // The value-less backup export converged with the master's: the master
    // holds the created tag's value, the backup does not.
    let speed = live_node(&gateway, "Line1/Speed").await.expect("created");
    assert!(speed.get("value").is_some(), "{speed}");
    // Only reads reached the backup, after the master was verified.
    assert!(
        backup
            .reads()
            .iter()
            .all(|r| r == "status" || r == "logs" || r.starts_with("export:")),
        "{:?}",
        backup.reads()
    );
    let log = event_names(&only_run_dir(&repo));
    let at = |name: &str| position(&log, name);
    assert!(at("tags_verified") < at("backup_wait_started"), "{log:?}");
    assert!(
        at("tag_record_written") < at("backup_wait_started"),
        "{log:?}"
    );
    assert!(at("backup_verdict") < at("finished"), "{log:?}");
}

#[tokio::test]
async fn backup_unsynced_ends_applied_backup_unsynced_and_keeps_the_run() {
    let repo = checkout(&[PROVIDER]);
    let gateway = adding_speed(&repo).await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let backup = FakeBackup::new(&gateway, &fake, BackupShape::Stale).await;

    let result = apply_paired(&repo, &gateway, &deploy, &backup, fast_backup())
        .await
        .expect("a verdict, not an error");
    assert_eq!(
        result.outcome,
        ProjectOutcome::AppliedBackupUnsynced,
        "{result:?}"
    );
    assert_eq!(result.outcome.as_str(), "applied_backup_unsynced");
    assert!(!result.ok);
    assert_eq!(result.slug.as_deref(), Some("deploy_backup_unsynced"));
    assert_eq!(result.failure_exit_code(), Some(6));
    // The master stands: records written, the journal cleared, nothing
    // rolled back.
    assert!(result.record_written);
    assert!(repo.data.join(TAG_RECORD_REL).exists(), "the tag record");
    assert!(host_journal(&repo).is_none(), "the journal was cleared");
    assert_eq!(tags_json(&result)["rollback"], Value::Null);
    assert!(live_node(&gateway, "Line1/Speed").await.is_some());
    let report = backup_json(&result);
    assert_eq!(report["converged"], false, "{report}");
    assert_eq!(report["reason"], "content_differs");
    assert_eq!(report["resync_sent"], true);
    assert_eq!(report["differing"], json!([PROVIDER]));
    assert_eq!(fake.resyncs(), 1, "exactly one resync, to the master");
    let log = event_names(&only_run_dir(&repo));
    assert!(!log.iter().any(|e| e == "rollback_started"), "{log:?}");
    assert_eq!(log.iter().filter(|e| *e == "backup_resync").count(), 1);
    let written: Value = serde_json::from_slice(
        &std::fs::read(only_run_dir(&repo).join("result.json")).expect("result.json"),
    )
    .expect("json");
    assert_eq!(written["outcome"], "applied_backup_unsynced");
    assert_eq!(written["slug"], "deploy_backup_unsynced");
}

#[tokio::test]
async fn backup_converges_after_the_one_resync() {
    let repo = checkout(&[PROVIDER]);
    let gateway = adding_speed(&repo).await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let backup = FakeBackup::new(&gateway, &fake, BackupShape::StaleUntilResync).await;

    let result = apply_paired(&repo, &gateway, &deploy, &backup, fast_backup())
        .await
        .expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    let report = backup_json(&result);
    assert_eq!(report["converged"], true, "{report}");
    assert_eq!(report["resync_sent"], true);
    assert_eq!(fake.resyncs(), 1);
}

#[tokio::test]
async fn backup_sync_exhausted_ends_the_wait_early_with_one_resync() {
    let repo = checkout(&[PROVIDER]);
    let gateway = adding_speed(&repo).await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let mut backup = FakeBackup::new(&gateway, &fake, BackupShape::Stale).await;
    backup.exhausted = true;
    let policy = BackupPolicy {
        budget: Duration::from_secs(20),
        interval: Duration::from_millis(5),
    };

    let started = std::time::Instant::now();
    let result = apply_paired(&repo, &gateway, &deploy, &backup, policy)
        .await
        .expect("a verdict");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait ended early: {:?}",
        started.elapsed()
    );
    assert_eq!(result.outcome, ProjectOutcome::AppliedBackupUnsynced);
    let report = backup_json(&result);
    assert_eq!(report["reason"], "sync_exhausted", "{report}");
    assert_eq!(report["resync_sent"], true);
    assert_eq!(fake.resyncs(), 1);
}

#[tokio::test]
async fn backup_unreadable_export_is_unreachable_not_an_error() {
    let repo = checkout(&[PROVIDER]);
    let gateway = adding_speed(&repo).await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let backup = FakeBackup::new(&gateway, &fake, BackupShape::ExportFails).await;

    let result = apply_paired(&repo, &gateway, &deploy, &backup, fast_backup())
        .await
        .expect("a verdict, not an error");
    assert_eq!(result.outcome, ProjectOutcome::AppliedBackupUnsynced);
    assert_eq!(
        backup_json(&result)["reason"],
        json!(UnsyncedReason::Unreachable.as_str())
    );
    assert!(result.record_written);
}

#[tokio::test]
async fn backup_absent_target_makes_no_backup_call() {
    let repo = checkout(&[PROVIDER]);
    let gateway = adding_speed(&repo).await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let input = repo.input();
    let seams = Seams {
        deploy: Some(&deploy),
        ..Seams::default()
    };
    let (loaded, prepared) = plan_with(&repo, &input, &gateway, seams)
        .await
        .expect("the plan completes");
    let result = run_paired(&repo, &input, &gateway, seams, &loaded, prepared, None)
        .await
        .expect("the run completes");
    assert_eq!(result.outcome, ProjectOutcome::Applied, "{result:?}");
    assert_eq!(backup_json(&result), Value::Null);
    assert_eq!(fake.resyncs(), 0);
    let log = event_names(&only_run_dir(&repo));
    assert!(!log.iter().any(|e| e.starts_with("backup_")), "{log:?}");
}

#[tokio::test]
async fn backup_after_a_rollback_converges_and_keeps_the_verdict() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        COUNT_DOC,
        FakeProjects::new().with_tag_import_failure(2),
    )
    .await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let backup = FakeBackup::new(&gateway, &fake, BackupShape::Synced).await;

    let result = apply_paired(&repo, &gateway, &deploy, &backup, fast_backup())
        .await
        .expect("a verdict, not an error");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert_eq!(result.slug.as_deref(), Some("deploy_rolled_back"));
    assert_eq!(result.failure_exit_code(), Some(6));
    let report = backup_json(&result);
    assert_eq!(report["converged"], true, "{report}");
    assert_eq!(report["providers"], json!([PROVIDER]));
    assert_eq!(fake.resyncs(), 0);
    let log = event_names(&only_run_dir(&repo));
    let at = |name: &str| position(&log, name);
    assert!(
        at("rollback_verified") < at("backup_wait_started"),
        "{log:?}"
    );
}

#[tokio::test]
async fn backup_unsynced_after_a_rollback_is_a_warning_only() {
    let repo = checkout(&[PROVIDER]);
    let gateway = changed_two(
        &repo,
        COUNT_DOC,
        FakeProjects::new().with_tag_import_failure(2),
    )
    .await;
    let fake = FakeGateway::new();
    let deploy = fake.client("master");
    let backup = FakeBackup::new(&gateway, &fake, BackupShape::Unlinked).await;
    let sent_before = gateway.tag_import_calls().len();

    let result = apply_paired(&repo, &gateway, &deploy, &backup, fast_backup())
        .await
        .expect("a verdict, not an error");
    assert_eq!(result.outcome, ProjectOutcome::TagsRolledBack, "{result:?}");
    assert_eq!(result.slug.as_deref(), Some("deploy_rolled_back"));
    assert_eq!(result.failure_exit_code(), Some(6));
    let report = backup_json(&result);
    assert_eq!(report["converged"], false, "{report}");
    assert_eq!(report["reason"], "not_syncing");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("backup") && w.contains("not_syncing")),
        "{:?}",
        result.warnings
    );
    // No second rollback: the run's three imports and nothing after.
    assert_eq!(gateway.tag_import_calls().len() - sent_before, 3);
    let log = event_names(&only_run_dir(&repo));
    assert_eq!(
        log.iter().filter(|e| *e == "rollback_started").count(),
        1,
        "{log:?}"
    );
}
