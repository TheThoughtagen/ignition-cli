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
use ignition_core::deploy::apply::preflight::DesignersPolicy;
use ignition_core::deploy::projects::{ProjectAction, ReleaseRecord};
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
    assert_eq!(tags.changed[0].properties, ["parameters.P1.value"]);
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
        Ok(loaded) => deploy_projects_prepare(
            &loaded,
            &input,
            &ProjectsSurfaces {
                gateway: &gateway,
                host: &host,
            },
        )
        .await
        .err()
        .expect("refused"),
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
