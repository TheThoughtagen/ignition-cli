//! Engine suite for `ign deploy projects --rehearse` (Phase 26 hardening):
//! the tag steps of a run and a forced rollback on a rehearsal gateway
//! seeded from the production environment's live tags, and the apply's
//! rehearsal gate. Two fakes: the production gateway behind a tap that
//! answers every write with an error and counts it (it must count none),
//! and the rehearsal gateway (`FakeProjects`, with the inherited-shape rule
//! where the case needs it). No network. Names are neutral (`PlantA`,
//! `CapApp`, `capgw`, `rig-gw`).

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::fake_projects::FakeProjects;
use ignition_core::actions::deploy::load_target;
use ignition_core::actions::deploy_projects::{
    DeployRehearsalResult, LoadedProject, NoProjectsObserver, ProjectGateway, ProjectsInput,
    ProjectsPrepared, ProjectsSurfaces, RehearsalTarget, RehearseOptions, TagDeleteSurface,
    deploy_projects_prepare, deploy_projects_rehearse, deploy_projects_rehearse_prepare,
    load_project, rehearsal_gate,
};
use ignition_core::client::projects::{ImportOutcome, ProjectRecord};
use ignition_core::client::tags::{
    CollisionPolicy, TagImportAnswer, TagProviderCreate, TagProviderModify, TagProviderRecord,
};
use ignition_core::deploy::apply::preflight::DesignerSeen;
use ignition_core::deploy::projects::ReleaseRecord;
use ignition_core::deploy::tag_record::TagRecord;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::error::CoreError;
use serde_json::{Value, json};

const PROJECT: &str = "CapApp";
const PROVIDER: &str = "PlantA";
const VERSION: &str = "1.6.0";
const PROJECT_JSON: &str = r#"{"title":"Cap App","enabled":true}"#;
const SCRIPT_RESOURCE: &str =
    r#"{"scope":"A","version":1,"files":["code.py"],"attributes":{"enabled":true}}"#;
const CODE: &[u8] = b"def f():\n    return 1\n";

/// A STANDARD provider's configuration, as the repo and both gateways hold it.
fn standard() -> Value {
    json!({"profile": {"type": "STANDARD"}, "settings": {}})
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let target = root.join(rel);
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(target, bytes).expect("write");
}

fn members() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("project.json", PROJECT_JSON.as_bytes().to_vec()),
        (
            "ignition/script-python/cap/resource.json",
            SCRIPT_RESOURCE.as_bytes().to_vec(),
        ),
        ("ignition/script-python/cap/code.py", CODE.to_vec()),
    ]
}

/// A project repo owning `PlantA`, a production target `dev` (gateway
/// `capgw`) and a rehearsal target `rig` (gateway `riggw`, declared name
/// `rig-gw`), each with its own data directory.
struct Checkout {
    _scratch: tempfile::TempDir,
    repo: PathBuf,
    targets: PathBuf,
    data: PathBuf,
    rig_data: PathBuf,
}

impl Checkout {
    fn new() -> Self {
        let scratch = tempfile::tempdir().expect("tempdir");
        let repo = scratch.path().join("repo");
        let targets = scratch.path().join("targets-root");
        let data = scratch.path().join("data");
        let rig_data = scratch.path().join("rig-data");
        for dir in [&data, &rig_data] {
            std::fs::create_dir_all(dir.join("projects")).expect("data dir");
        }
        for (rel, bytes) in members() {
            put(&repo, rel, &bytes);
        }
        put(
            &repo,
            "deploy/project.yaml",
            format!(
                "schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\ntag_providers: \
                 [{PROVIDER}]\n"
            )
            .as_bytes(),
        );
        put(
            &repo,
            &format!("deploy/tag-providers/{PROVIDER}.json"),
            serde_json::to_vec(
                &json!({"schema_version": 1, "enabled": true, "config": standard()}),
            )
            .expect("json")
            .as_slice(),
        );
        put(
            &data,
            "config/resources/core/ignition/system-properties/config.json",
            br#"{"systemName":"capgw"}"#,
        );
        let checkout = Checkout {
            _scratch: scratch,
            repo,
            targets,
            data,
            rig_data,
        };
        checkout.prod_target("");
        checkout.rig_target("rig-gw", "");
        checkout
    }

    /// `targets/dev.yaml`, with `extra` appended.
    fn prod_target(&self, extra: &str) {
        put(
            &self.targets,
            "targets/dev.yaml",
            format!(
                "schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: \
                 dev\n    local:\n      data_dir: '{}'\n{extra}",
                self.data.display()
            )
            .as_bytes(),
        );
    }

    /// `targets/rig.yaml` declaring the gateway name `name`, with `extra`.
    fn rig_target(&self, name: &str, extra: &str) {
        put(
            &self.targets,
            "targets/rig.yaml",
            format!(
                "schema_version: 1\ngateway: riggw\nmode: dev\nrehearsal: true\nnodes:\n  \
                 master:\n    profile: rig\n    gateway_name: {name}\n    local:\n      \
                 data_dir: '{}'\n{extra}",
                self.rig_data.display()
            )
            .as_bytes(),
        );
    }

    fn tag(&self, path: &str, json: &str) {
        put(
            &self.repo,
            &format!("tags/{PROVIDER}/{path}.json"),
            json.as_bytes(),
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
            allow_destructive: false,
            allow_manual_undo: true,
            artifact_sha256: None,
            tags_sha256: None,
        }
    }

    fn host(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.data.clone(), "nodes.master")
    }

    fn rig_host(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.rig_data.clone(), "nodes.master")
    }

    /// The release record of the live project and this project's tag record
    /// of `PlantA` listing `Line1`, on the production host.
    fn records(&self, artifact_sha256: &str) {
        let record = ReleaseRecord {
            schema_version: 1,
            gateway: "capgw".to_string(),
            project: PROJECT.to_string(),
            release: "v0.9.0".to_string(),
            artifact_sha256: artifact_sha256.to_string(),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
        };
        put(
            &self.data,
            "ign-deploy/capgw/projects/CapApp/release.json",
            &record.to_bytes(),
        );
        let tags = TagRecord {
            schema_version: 1,
            gateway: "capgw".to_string(),
            provider: PROVIDER.to_string(),
            project: PROJECT.to_string(),
            release: "v0.9.0".to_string(),
            run_id: "20261020T101500Z-ab12".to_string(),
            deployed_at: "2026-10-20T10:15:00Z".to_string(),
            tags_sha256: "ab".repeat(32),
            nodes: vec!["Line1".to_string()],
            groups: Vec::new(),
        };
        put(
            &self.data,
            "ign-deploy/capgw/tag-providers/PlantA/record.json",
            &tags.to_bytes(),
        );
    }
}

/// The production gateway: the repo's project, recorded, and `PlantA`
/// holding `tree` with STANDARD settings.
async fn production(repo: &Checkout, tree: Value) -> FakeProjects {
    let gateway = FakeProjects::new()
        .with_project(PROJECT, None, false, &members())
        .with_tag_tree(PROVIDER, tree);
    gateway.set_provider_settings(PROVIDER, true, None, standard());
    let live = gateway.export_bytes(PROJECT).await.expect("export");
    let digest =
        ignition_core::deploy::project_artifact::canonical_digest(&live).expect("an export");
    repo.records(&digest.sha256);
    gateway
}

/// A [`ProjectGateway`] that forwards every read and answers every write
/// with an error, counting it: the production side of a rehearsal must
/// count none.
struct ReadOnlyTap<'a> {
    inner: &'a FakeProjects,
    writes: AtomicUsize,
}

impl<'a> ReadOnlyTap<'a> {
    fn new(inner: &'a FakeProjects) -> Self {
        Self {
            inner,
            writes: AtomicUsize::new(0),
        }
    }

    fn refused(&self, what: &str) -> CoreError {
        self.writes.fetch_add(1, Ordering::SeqCst);
        CoreError::Internal(format!("405 Method Not Allowed: {what}"))
    }

    fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ProjectGateway for ReadOnlyTap<'_> {
    async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError> {
        self.inner.projects().await
    }
    async fn project_find(&self, name: &str) -> Result<Option<ProjectRecord>, CoreError> {
        self.inner.project_find(name).await
    }
    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, CoreError> {
        self.inner.export_bytes(name).await
    }
    async fn designers(&self) -> Result<Vec<DesignerSeen>, CoreError> {
        self.inner.designers().await
    }
    async fn gateway_name(&self) -> Result<Option<String>, CoreError> {
        self.inner.gateway_name().await
    }
    async fn tag_provider_names(&self) -> Result<Vec<String>, CoreError> {
        self.inner.tag_provider_names().await
    }
    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
        self.inner.tag_export(provider).await
    }
    async fn project_import(
        &self,
        _name: &str,
        _zip: Vec<u8>,
        _overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        Err(self.refused("project_import"))
    }
    async fn tag_import(
        &self,
        _provider: &str,
        _path: &str,
        _policy: CollisionPolicy,
        _body: Vec<u8>,
    ) -> Result<TagImportAnswer, CoreError> {
        Err(self.refused("tag_import"))
    }
    async fn tag_provider_record(
        &self,
        name: &str,
    ) -> Result<Option<TagProviderRecord>, CoreError> {
        self.inner.tag_provider_record(name).await
    }
    async fn tag_provider_create(&self, _body: &[TagProviderCreate]) -> Result<(), CoreError> {
        Err(self.refused("tag_provider_create"))
    }
    async fn tag_provider_modify(&self, _body: &[TagProviderModify]) -> Result<(), CoreError> {
        Err(self.refused("tag_provider_modify"))
    }
    async fn tag_provider_delete(&self, _name: &str, _signature: &str) -> Result<(), CoreError> {
        Err(self.refused("tag_provider_delete"))
    }
}

fn surfaces<'a>(
    gateway: &'a dyn ProjectGateway,
    host: &'a LocalFsTransport,
) -> ProjectsSurfaces<'a> {
    ProjectsSurfaces {
        gateway,
        host,
        tree: None,
        apply_host: None,
        deploy: None,
        tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
    }
}

fn rehearsal<'a>(
    repo: &Checkout,
    rig: &'a dyn ProjectGateway,
    host: &'a LocalFsTransport,
) -> RehearsalTarget<'a> {
    let (target_file, target) = load_target(&repo.targets, "rig").expect("the rig target");
    RehearsalTarget {
        env: "rig".to_string(),
        target_file,
        target,
        surfaces: surfaces(rig, host),
        url: "http://localhost:18290".to_string(),
        env_urls: vec!["https://capgw.example:8043".to_string()],
    }
}

fn options(repo: &Checkout) -> RehearseOptions {
    RehearseOptions {
        repo: repo.repo.clone(),
        targets_root: repo.targets.clone(),
        ign_version: VERSION.to_string(),
        tag_verify_settle: Duration::from_millis(300),
        tag_verify_poll: Duration::from_millis(5),
        tag_delete_ready: Duration::from_millis(50),
        tag_delete_poll: Duration::from_millis(5),
    }
}

/// Rehearse `repo` against `prod` (behind the tap) on `rig`.
async fn rehearse(
    repo: &Checkout,
    prod: &FakeProjects,
    rig: &FakeProjects,
) -> (Result<DeployRehearsalResult, CoreError>, usize) {
    let input = repo.input();
    let loaded = load_project(&input).expect("the repo loads");
    let tap = ReadOnlyTap::new(prod);
    let host = repo.host();
    let rig_host = repo.rig_host();
    let env = surfaces(&tap, &host);
    let target = rehearsal(repo, rig, &rig_host);
    let result = match deploy_projects_rehearse_prepare(&loaded, &input, &env, &target).await {
        Ok(prepared) => {
            deploy_projects_rehearse(
                prepared,
                &loaded,
                &input,
                &env,
                &target,
                &options(repo),
                &NoProjectsObserver,
            )
            .await
        }
        Err(err) => Err(err),
    };
    (result, tap.writes())
}

/// The plan of an apply to `prod`, as the CLI draws it before the gate.
async fn apply_plan(repo: &Checkout, prod: &FakeProjects) -> (LoadedProject, ProjectsPrepared) {
    let input = repo.input();
    let loaded = load_project(&input).expect("the repo loads");
    let host = repo.host();
    let prepared = deploy_projects_prepare(&loaded, &input, &surfaces(prod, &host))
        .await
        .expect("the plan");
    (loaded, prepared)
}

const SCALE_TYPE: &str = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Net","tagType":"AtomicTag","value":0,"valueSource":"memory"}]}"#;
const SCANNER_FOLDER: &str = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Speed","tagType":"AtomicTag","value":0,"valueSource":"memory"},{"name":"Weigh","tagType":"Folder","tags":[{"dataType":"Int4","name":"Gross","tagType":"AtomicTag","value":0,"valueSource":"memory"}]}]}"#;
const SCANNER_INSTANCE: &str = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Speed","tagType":"AtomicTag","value":0,"valueSource":"memory"},{"name":"Weigh","tagType":"UdtInstance","typeId":"Scale"}]}"#;
const S1_FOLDER: &str = r#"{"tagType":"UdtInstance","tags":[{"name":"Weigh","tagType":"Folder","tags":[{"name":"Gross","tagType":"AtomicTag","tooltip":"g"}]}],"typeId":"Scanner"}"#;
const S1_INSTANCE: &str = r#"{"tagType":"UdtInstance","tags":[{"name":"Weigh","tagType":"UdtInstance","tooltip":"w","typeId":"Scale"}],"typeId":"Scanner"}"#;

fn named(json: &str, name: &str) -> Value {
    let mut v: Value = serde_json::from_str(json).expect("json");
    v["name"] = json!(name);
    v
}

fn scanner_tree(scanner: &str, s1: &str) -> Value {
    json!({"name":"","tagType":"Provider","tags":[
        {"name":"_types_","tagType":"Folder","tags":[
            named(SCALE_TYPE, "Scale"), named(scanner, "Scanner")
        ]},
        {"name":"Line1","tagType":"Folder","tags":[named(s1, "S1")]}
    ]})
}

/// A repo that changes the type member `Weigh` from a folder to an instance
/// of another type, and production holding the folder shape.
async fn shape_change() -> (Checkout, FakeProjects) {
    let repo = Checkout::new();
    repo.tag("_types_/Scale", SCALE_TYPE);
    repo.tag("_types_/Scanner", SCANNER_INSTANCE);
    repo.tag("Line1/S1", S1_INSTANCE);
    let prod = production(&repo, scanner_tree(SCANNER_FOLDER, S1_FOLDER)).await;
    (repo, prod)
}

fn rig_gateway() -> FakeProjects {
    FakeProjects::new()
        .with_gateway_name("rig-gw")
        .with_inherited_shape_rule()
}

fn json_of(result: &DeployRehearsalResult) -> Value {
    serde_json::to_value(result).expect("json")
}

/// A green rehearsal of a shape-changing run: the seed holds production's
/// tags, the run's prune batches go forward and back on the rehearsal
/// gateway (which refuses an unpruned `Overwrite`), both verifies are green,
/// the record is written, and production saw no write at all.
#[tokio::test]
async fn green_rehearsal_of_a_shape_change() {
    let (repo, prod) = shape_change().await;
    // A leftover provider on the rig is dropped by the seed.
    let rig = rig_gateway().with_tag_tree(
        PROVIDER,
        json!({"name":"","tagType":"Provider","tags":[{"name":"Old","tagType":"Folder"}]}),
    );
    let prod_before = prod.tag_export(PROVIDER).await.expect("export");

    let (result, writes) = rehearse(&repo, &prod, &rig).await;
    let result = result.expect("a verdict");
    assert_eq!(writes, 0, "production took no write");
    assert!(prod.tag_import_calls().is_empty());
    assert!(prod.import_calls().is_empty());
    assert_eq!(
        prod.tag_export(PROVIDER).await.expect("export"),
        prod_before
    );
    assert!(
        rig.import_calls().is_empty(),
        "the project is never imported"
    );

    let data = json_of(&result);
    let rehearsal = &data["rehearsal"];
    assert!(result.ok, "{data:#}");
    assert_eq!(result.slug, None);
    assert_eq!(rehearsal["verdict"], "green", "{rehearsal:#}");
    assert_eq!(rehearsal["failed_at"], Value::Null);
    assert_eq!(rehearsal["seed"]["providers"][0]["settings"], "rehearsed");
    assert_eq!(rehearsal["seed"]["mismatches_count"], 0);
    assert_eq!(rehearsal["forward"]["verify"]["equal"], true);
    let pruned = json!(["[PlantA]Line1/S1/Weigh", "[PlantA]_types_/Scanner/Weigh"]);
    assert_eq!(rehearsal["forward"]["pruned"], pruned, "{rehearsal:#}");
    assert_eq!(rehearsal["rollback"]["green"], true, "{rehearsal:#}");
    assert_eq!(rehearsal["rollback"]["verify"]["equal"], true);
    assert_eq!(rehearsal["rollback"]["pruned"], pruned);
    assert_eq!(rehearsal["ign_version"], VERSION);
    assert_eq!(result.failure_exit_code(), None);

    // The seed import, the forward batches (prune, whole, prune, whole) and
    // the rollback's, all on the rehearsal gateway.
    let calls: Vec<(String, String)> = rig
        .tag_import_calls()
        .into_iter()
        .map(|(_, path, policy)| (path, policy))
        .collect();
    assert_eq!(calls.len(), 9, "{calls:?}");
    assert_eq!(calls[0], ("".to_string(), "Overwrite".to_string()));

    // The record binds the hashes and the version, 0600.
    let id = result.rehearsal.id.clone();
    let path = repo.repo.join(&result.rehearsal.record);
    assert_eq!(
        result.rehearsal.record,
        format!(".ign/deploy/capgw/rehearsals/{id}.json")
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("the record")).expect("json");
    assert_eq!(record["verdict"], "green");
    assert_eq!(record["ign_version"], VERSION);
    assert_eq!(record["rehearsal_gateway"], "riggw");
    assert_eq!(record["live_tags_sha256"], rehearsal["live_tags_sha256"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // The apply gate accepts the record for the same inputs.
    let (loaded, prepared) = apply_plan(&repo, &prod).await;
    let accepted = rehearsal_gate(&loaded, &repo.input(), &prepared, Some(&id), VERSION)
        .expect("a matching record");
    assert_eq!(accepted.map(|r| r.id), Some(id));
}

/// An instance whose parameter the rehearsal gateway stores in another form
/// than it was sent: the forward verify is red at that node, the forced
/// rollback restores it, and the rehearsal is a red verdict with a red
/// record that the apply gate refuses.
#[tokio::test]
async fn red_rehearsal_names_the_node_the_gateway_rewrote() {
    let repo = Checkout::new();
    let motor = r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Speed","tagType":"AtomicTag","value":0,"valueSource":"memory"}]}"#;
    repo.tag("_types_/Motor", motor);
    repo.tag(
        "Line1/M1",
        r#"{"parameters":{"Mode":{"dataType":"String","value":"manual"}},"tagType":"UdtInstance","typeId":"Motor"}"#,
    );
    let prod = production(
        &repo,
        json!({"name":"","tagType":"Provider","tags":[
            {"name":"_types_","tagType":"Folder","tags":[named(motor, "Motor")]},
            {"name":"Line1","tagType":"Folder","tags":[
                {"name":"M1","tagType":"UdtInstance","typeId":"Motor"}
            ]}
        ]}),
    )
    .await;
    // Import 1 is the seed; import 2 the forward batch of M1, after which
    // the gateway holds the parameter as an Integer.
    let rig = rig_gateway().with_write_after_import(
        2,
        PROVIDER,
        "Line1/M1",
        "parameters",
        json!({"Mode": {"dataType": "Integer", "value": 0}}),
    );

    let (result, writes) = rehearse(&repo, &prod, &rig).await;
    let result = result.expect("a verdict");
    assert_eq!(writes, 0);
    let data = json_of(&result);
    let rehearsal = &data["rehearsal"];
    assert!(!result.ok, "{data:#}");
    assert_eq!(result.slug.as_deref(), Some("deploy_rehearsal_red"));
    assert_eq!(result.failure_exit_code(), Some(6));
    assert_eq!(rehearsal["verdict"], "red");
    assert_eq!(rehearsal["failed_at"], "forward");
    assert_eq!(rehearsal["forward"]["verify"]["equal"], false);
    let mismatches = rehearsal["forward"]["verify"]["mismatches"].to_string();
    assert!(mismatches.contains("[PlantA]Line1/M1"), "{mismatches}");
    assert!(mismatches.contains("parameters"), "{mismatches}");
    assert_eq!(rehearsal["rollback"]["green"], true, "{rehearsal:#}");

    let id = result.rehearsal.id.clone();
    let record: Value = serde_json::from_slice(
        &std::fs::read(repo.repo.join(&result.rehearsal.record)).expect("the record"),
    )
    .expect("json");
    assert_eq!(record["verdict"], "red");
    assert_eq!(record["failed_at"], "forward");

    let (loaded, prepared) = apply_plan(&repo, &prod).await;
    let refused = rehearsal_gate(&loaded, &repo.input(), &prepared, Some(&id), VERSION)
        .expect_err("a red record");
    match refused {
        CoreError::RehearsalStale {
            field, recorded, ..
        } => assert_eq!((field.as_str(), recorded.as_str()), ("verdict", "red")),
        other => panic!("expected rehearsal_stale, got {other:?}"),
    }
}

/// The gate: a target that requires a rehearsal refuses an apply without
/// one or with an id that names no record; a record refuses once the live
/// tags, the repo's tags or the ign version moved; a target that does not
/// require one passes without an id.
#[tokio::test]
async fn the_apply_gate_refuses_missing_and_stale_records() {
    let (repo, prod) = shape_change().await;
    let rig = rig_gateway();
    let (result, _) = rehearse(&repo, &prod, &rig).await;
    let id = result.expect("green").rehearsal.id;
    let input = repo.input();

    let (loaded, prepared) = apply_plan(&repo, &prod).await;
    assert_eq!(
        rehearsal_gate(&loaded, &input, &prepared, None, VERSION).expect("not required"),
        None
    );
    repo.prod_target("require_rehearsal: true\n");
    let (loaded, prepared) = apply_plan(&repo, &prod).await;
    let missing = rehearsal_gate(&loaded, &input, &prepared, None, VERSION).expect_err("required");
    assert_eq!(missing.code(), "rehearsal_required");
    let absent = rehearsal_gate(
        &loaded,
        &input,
        &prepared,
        Some("20261111T000000Z-000000"),
        VERSION,
    )
    .expect_err("no such record");
    assert_eq!(absent.code(), "rehearsal_required");
    assert!(
        absent.to_string().contains("no rehearsal record"),
        "{absent}"
    );
    rehearsal_gate(&loaded, &input, &prepared, Some(&id), VERSION).expect("still matching");

    let field = |err: CoreError| match err {
        CoreError::RehearsalStale { field, .. } => field,
        other => panic!("expected rehearsal_stale, got {other:?}"),
    };
    // Another ign build.
    assert_eq!(
        field(rehearsal_gate(&loaded, &input, &prepared, Some(&id), "9.9.9").expect_err("version")),
        "ign_version"
    );
    // The live tags changed (a configuration change of a drift node).
    let mut moved = scanner_tree(SCANNER_FOLDER, S1_FOLDER);
    moved["tags"][1]["tags"]
        .as_array_mut()
        .expect("Line1")
        .push(json!({"name":"Extra","tagType":"Folder"}));
    prod.set_tag_tree(PROVIDER, moved);
    let (loaded, prepared) = apply_plan(&repo, &prod).await;
    assert_eq!(
        field(rehearsal_gate(&loaded, &input, &prepared, Some(&id), VERSION).expect_err("live")),
        "live_tags_sha256"
    );
    // The repo's tags changed.
    prod.set_tag_tree(PROVIDER, scanner_tree(SCANNER_FOLDER, S1_FOLDER));
    repo.tag(
        "_types_/Scale",
        r#"{"tagType":"UdtType","tags":[{"dataType":"Int4","name":"Net","tagType":"AtomicTag","value":1,"valueSource":"memory"}]}"#,
    );
    let (loaded, prepared) = apply_plan(&repo, &prod).await;
    assert_eq!(
        field(rehearsal_gate(&loaded, &input, &prepared, Some(&id), VERSION).expect_err("tags")),
        "tags_sha256"
    );
}

/// A rehearsal target that is the production gateway is refused before any
/// write to either side: by its declared name, and by the name the gateway
/// its profile reaches reports.
#[tokio::test]
async fn a_rehearsal_target_that_is_production_is_refused() {
    let (repo, prod) = shape_change().await;
    repo.rig_target("capgw", "");
    let rig = rig_gateway();
    let (result, writes) = rehearse(&repo, &prod, &rig).await;
    let err = result.expect_err("refused");
    assert_eq!(err.code(), "rehearsal_target_invalid", "{err}");
    assert_eq!(writes, 0);
    assert!(rig.tag_import_calls().is_empty());

    // Declared as `rig-gw`, but its profile reaches the production gateway.
    repo.rig_target("rig-gw", "");
    let reaching_prod = FakeProjects::new().with_gateway_name("capgw");
    let (result, writes) = rehearse(&repo, &prod, &reaching_prod).await;
    let err = result.expect_err("refused");
    assert_eq!(err.code(), "rehearsal_target_invalid", "{err}");
    assert!(err.to_string().contains("reports the name capgw"), "{err}");
    assert_eq!(writes, 0);
    assert!(reaching_prod.tag_import_calls().is_empty());
    assert!(prod.tag_import_calls().is_empty());

    // A target without `rehearsal: true` is never wiped.
    put(
        &repo.targets,
        "targets/rig.yaml",
        format!(
            "schema_version: 1\ngateway: riggw\nmode: dev\nnodes:\n  master:\n    profile: \
             rig\n    gateway_name: rig-gw\n    local:\n      data_dir: '{}'\n",
            repo.rig_data.display()
        )
        .as_bytes(),
    );
    let (result, _) = rehearse(&repo, &prod, &rig).await;
    let err = result.expect_err("refused");
    assert_eq!(err.code(), "rehearsal_target_invalid");
    assert!(err.to_string().contains("rehearsal: true"), "{err}");
    assert!(rig.tag_import_calls().is_empty());
}

/// A run that creates a tag in the existing provider, with no tag-delete
/// route on the rehearsal gateway: without `--allow-manual-undo` the
/// rehearsal refuses as the run would, before any write; with it the forced
/// rollback leaves the created node, reports it, and the cleanup deletes and
/// re-seeds the provider so the rehearsal gateway holds production's tags
/// again.
#[tokio::test]
async fn manual_undo_is_reported_and_cleaned_up() {
    let repo = Checkout::new();
    repo.tag(
        "Line1/Speed",
        r#"{"dataType":"Int4","tagType":"AtomicTag","value":3,"valueSource":"memory"}"#,
    );
    let tree = json!({"name":"","tagType":"Provider","tags":[
        {"name":"_types_","tagType":"Folder"},
        {"name":"Line1","tagType":"Folder","tags":[]}
    ]});
    let prod = production(&repo, tree).await;

    let strict = ProjectsInput {
        allow_manual_undo: false,
        ..repo.input()
    };
    let loaded = load_project(&strict).expect("loads");
    let tap = ReadOnlyTap::new(&prod);
    let host = repo.host();
    let rig_host = repo.rig_host();
    let rig = rig_gateway();
    let env = surfaces(&tap, &host);
    let target = rehearsal(&repo, &rig, &rig_host);
    let prepared = deploy_projects_rehearse_prepare(&loaded, &strict, &env, &target)
        .await
        .expect("prepared");
    let err = deploy_projects_rehearse(
        prepared,
        &loaded,
        &strict,
        &env,
        &target,
        &options(&repo),
        &NoProjectsObserver,
    )
    .await
    .expect_err("refused");
    assert_eq!(err.code(), "tags_undo_incomplete");
    assert!(rig.tag_import_calls().is_empty(), "nothing was written");
    assert_eq!(tap.writes(), 0);

    let (result, writes) = rehearse(&repo, &prod, &rig).await;
    let result = result.expect("a verdict");
    assert_eq!(writes, 0);
    let data = json_of(&result);
    let rehearsal = &data["rehearsal"];
    assert_eq!(rehearsal["verdict"], "green", "{rehearsal:#}");
    assert_eq!(
        rehearsal["rollback"]["undo_manual"],
        json!(["[PlantA]Line1/Speed"])
    );
    assert_eq!(rehearsal["cleanup"], json!(["PlantA"]));
    let left = ignition_core::deploy::tag_canon::flatten_export(
        &rig.tag_export(PROVIDER).await.expect("export"),
        PROVIDER,
    )
    .expect("a tree");
    assert!(!left.contains_key("Line1/Speed"), "{left:?}");
    assert!(left.contains_key("Line1"));
}
