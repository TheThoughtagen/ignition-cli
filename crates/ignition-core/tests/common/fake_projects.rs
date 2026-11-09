//! `FakeProjects`: an in-memory gateway project store implementing
//! [`ProjectGateway`] (25-05). The `deploy projects` engine suite
//! (`tests/deploy_projects.rs`) runs against it instead of a live gateway.
//!
//! A fake that encodes assumptions as facts turns false beliefs into green
//! tests (threat T-25-62), so every behaviour below cites the
//! `25-LIVE-CAPTURES.md` block it copies and the places the captures are
//! silent are named.
//!
//! What it copies:
//!
//! - **Layout (C8c, K1 a).** A project is a map of member path to bytes in
//!   the checkout layout. The export zip carries the same paths verbatim: the
//!   C8c probe shows the gateway stores and exports the paths it was given
//!   and never converts, and a checkout-layout import exports under the same
//!   keys. No `resources/` segment is added or stripped.
//! - **No directory entries (C8c).** The synthetic zip held 6 directory
//!   entries and its export 0; a real parent project's zip held 1,174 and its export
//!   0. The export here holds file entries only.
//! - **Volatile stamps (C8b).** Each export writes
//!   `attributes.lastModification` and `attributes.lastModificationSignature`
//!   into every `resource.json` that has an `attributes` object, with a new
//!   value on every export, so a comparison that passes has ignored them. The
//!   captures record the two field names and that 196 of 245 differing
//!   parent project members differed in nothing else.
//! - **`project.json` (C8b, C8c).** The export adds `description` and
//!   `parent` (and nothing else) when the stored file lacks them. The
//!   captures name those two keys as the only difference.
//! - **Listing and find (C8e).** `projects/list` carries one record per
//!   project and `find` answers the same record; an absent project is `None`
//!   (the real client maps the 404 to `NotFound`, which the gateway adapter
//!   turns into `None`).
//!
//! Where the captures are silent:
//!
//! - The value shapes of the two stamps. The captures record field names
//!   only. The fake writes an `{actor, timestamp}` object and a hex string.
//! - A `resource.json` without an `attributes` object. Every descriptor in
//!   the captures had one (`"attributes":{}` in the synthetic checkout), so
//!   the fake leaves a descriptor without one unstamped and the suite never
//!   uses one.
//! - The order of `projects/list`. The fake answers in descending byte order
//!   of the name, so no test can pass on a listing that happens to arrive
//!   sorted.
//!
//! Children (25-15). `with_child` adds a project whose `parent` is another
//! one. `mutate_child_after_import` edits a project between the reads of a
//! sweep, as a gateway still settling after a parent overwrite would (C9 saw
//! the children's exports change for about 5 s). `fail_export_of` makes every
//! export of one project fail with `network_error`. None of the three is a
//! write the verb can make: they stand in for the gateway and for other
//! administrators, and the engine suite never reaches them through
//! [`ProjectGateway`].
//!
//! Tags (26-01). `with_tag_tree` seeds a provider's native export tree, and
//! `tag_import` applies an `Abort` import to it as 26-LIVE-CAPTURES.md C1
//! recorded: a new node is added, a folder that already exists takes the new
//! children, and any other name already there is a failure (`Bad`, sub-code
//! 527) in a 200 answer. The export answers the tree as it stands. Other
//! collision policies are refused as not modelled until the captures pin
//! them (26-08).
//!
//! Created AtomicTags (26-03, C12). An AtomicTag the import creates outside
//! `_types_/`, whose body carries `value` and no `defaultValue`, is stored
//! with `defaultValue` equal to that `value`, as the C12 export showed
//! (`F/T.defaultValue: sent "<absent>" exported 3`). The walk follows folder
//! children only: C12's instance members and type members gained nothing.
//! Two knobs stand in for what the captures did not show:
//! `with_created_default_value` stores a different `defaultValue`, and
//! `with_created_value` stores a different `value` (a runtime write between
//! the import and the verify).
//!
//! The one write is `project_import` (25-11). Nothing ever adds a delete,
//! rename, copy, modify or reparent method, so the engine suite can never pass
//! on a gateway surface the verb is forbidden to have (DPRJ-04).
//!
//! `project_import` copies these captures too:
//!
//! - **Overwrite replaces the whole project (C8a).** Every member the zip
//!   omits is gone afterwards, and the answer never names what was deleted.
//! - **Adoption (C8d, K2).** Only `project.json`, each `resource.json` and the
//!   files that `resource.json` lists are kept; an unlisted file or a dotfile
//!   is dropped without an error, and a listed file the zip lacks is rewritten
//!   out of `files`.
//! - **The answer (C8c).** `{"success": true, "changes": [{"name": <project>}]}`.
//!   `answer_empty_changes` makes it `"changes": []` to exercise the warning
//!   for an import that reports no change (research Pitfall 1); the project is
//!   adopted either way.
//!
//! Where the captures are silent: `overwrite=false` on a project that
//! already exists. The fake answers the denial family the client already maps
//! (`import_denied`), and no engine case relies on its wording.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ignition_core::actions::deploy_projects::ProjectGateway;
use ignition_core::client::projects::{ImportOutcome, ProjectRecord};
use ignition_core::client::tags::{CollisionPolicy, TagImportAnswer};
use ignition_core::deploy::apply::preflight::DesignerSeen;
use ignition_core::deploy::transport::projects::ReleaseRecordWriter;
use ignition_core::error::CoreError;

/// Where the fake pretends to live; only ever shown in error endpoints.
const FAKE_BASE: &str = "http://fake-gateway.invalid";

/// One stored project: the record `list` and `find` answer and the members
/// (checkout-layout path to bytes) an export is built from.
pub struct StoredProject {
    pub record: ProjectRecord,
    pub members: BTreeMap<String, Vec<u8>>,
}

/// What the check asked of the gateway, for footprint assertions.
#[derive(Default)]
struct Calls {
    list: usize,
    find: Vec<String>,
    export: Vec<String>,
    import: Vec<(String, bool)>,
    /// Every call in order: `list`, `find:<name>`, `export:<name>`,
    /// `import:<name>:<overwrite>`.
    order: Vec<String>,
    /// How many times the Designer list was read.
    designers: usize,
}

/// What the Designer list answers: one list per read (the last repeats), or a
/// read that fails.
#[derive(Default)]
struct DesignersScript {
    lists: Vec<Vec<DesignerSeen>>,
    fail: bool,
}

/// Edits a project after the first import: the next `remaining` exports of it
/// each leave it different from the one before.
struct Churn {
    remaining: usize,
    edits: usize,
}

/// The in-memory gateway project store.
pub struct FakeProjects {
    projects: Mutex<BTreeMap<String, StoredProject>>,
    calls: Mutex<Calls>,
    stamp: AtomicU64,
    empty_changes: AtomicBool,
    designers: Mutex<DesignersScript>,
    /// What `gateway-info` answers as the gateway's name.
    gateway_name: Mutex<Option<String>>,
    /// Projects that change under their exports once an import has happened.
    churn: Mutex<BTreeMap<String, Churn>>,
    /// Whether any import has been sent.
    imported: AtomicBool,
    /// Projects whose export always fails.
    failing_exports: Mutex<Vec<String>>,
    /// The tag providers the gateway lists, with the bytes their native
    /// export answers, in listing order.
    tag_providers: Mutex<Vec<(String, Vec<u8>)>>,
    /// Whether the tag provider list fails.
    tag_list_fails: AtomicBool,
    /// Tag providers whose export fails.
    failing_tag_exports: Mutex<Vec<String>>,
    /// How the tag reads were used.
    tag_calls: Mutex<TagCalls>,
    /// The `defaultValue` a created AtomicTag gets instead of its `value`.
    created_default: Mutex<Option<serde_json::Value>>,
    /// The `value` a created AtomicTag holds instead of the sent one.
    created_value: Mutex<Option<serde_json::Value>>,
}

/// The tag reads the check made, for footprint assertions.
#[derive(Default)]
struct TagCalls {
    list: usize,
    exports: Vec<String>,
    /// Every tag import as `(provider, path, policy)`, in call order.
    imports: Vec<(String, String, String)>,
}

/// The name the fake gateway reports until a test says otherwise: the
/// `gateway:` of the engine suite's target file, so an ordinary run is aimed
/// at the gateway it names.
pub const FAKE_GATEWAY_NAME: &str = "capgw";

impl Default for FakeProjects {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeProjects {
    /// A gateway with no projects.
    pub fn new() -> Self {
        Self {
            projects: Mutex::new(BTreeMap::new()),
            calls: Mutex::new(Calls::default()),
            stamp: AtomicU64::new(0),
            empty_changes: AtomicBool::new(false),
            designers: Mutex::new(DesignersScript::default()),
            gateway_name: Mutex::new(Some(FAKE_GATEWAY_NAME.to_string())),
            churn: Mutex::new(BTreeMap::new()),
            imported: AtomicBool::new(false),
            failing_exports: Mutex::new(Vec::new()),
            tag_providers: Mutex::new(Vec::new()),
            tag_list_fails: AtomicBool::new(false),
            failing_tag_exports: Mutex::new(Vec::new()),
            tag_calls: Mutex::new(TagCalls::default()),
            created_default: Mutex::new(None),
            created_value: Mutex::new(None),
        }
    }

    /// A created AtomicTag is stored with this `defaultValue` instead of the
    /// one C12 recorded (equal to the sent `value`).
    pub fn with_created_default_value(self, value: serde_json::Value) -> Self {
        *self.created_default.lock().expect("created default") = Some(value);
        self
    }

    /// A created AtomicTag holds this `value` instead of the sent one, as if
    /// it had been written at runtime before the verify read it.
    pub fn with_created_value(self, value: serde_json::Value) -> Self {
        *self.created_value.lock().expect("created value") = Some(value);
        self
    }

    /// The gateway lists tag provider `name`, and its native export answers
    /// `export`.
    pub fn with_tag_provider(self, name: &str, export: Vec<u8>) -> Self {
        self.tag_providers
            .lock()
            .expect("tag providers")
            .push((name.to_string(), export));
        self
    }

    /// Every tag provider list read fails with `network_error`.
    pub fn with_tag_list_error(self) -> Self {
        self.tag_list_fails.store(true, Ordering::SeqCst);
        self
    }

    /// Every export of tag provider `name` fails with `network_error`.
    pub fn with_tag_export_error(self, name: &str) -> Self {
        self.failing_tag_exports
            .lock()
            .expect("failing tag exports")
            .push(name.to_string());
        self
    }

    /// How many times the tag provider list was read.
    pub fn tag_list_count(&self) -> usize {
        self.tag_calls.lock().expect("tag calls").list
    }

    /// Every tag provider exported, in call order.
    pub fn tag_export_calls(&self) -> Vec<String> {
        self.tag_calls.lock().expect("tag calls").exports.clone()
    }

    /// The gateway lists tag provider `provider` holding the native export
    /// tree `export` (`{"name":"","tagType":"Provider","tags":[...]}`). Its
    /// export answers the tree as it stands, so it changes with every
    /// `tag_import`.
    pub fn with_tag_tree(self, provider: &str, export: serde_json::Value) -> Self {
        let bytes = serde_json::to_vec(&export).expect("tag tree serializes");
        self.with_tag_provider(provider, bytes)
    }

    /// Every tag import as `(provider, path, policy)`, in call order.
    pub fn tag_import_calls(&self) -> Vec<(String, String, String)> {
        self.tag_calls.lock().expect("tag calls").imports.clone()
    }

    /// The gateway reports `name` over REST (`gateway-info`).
    pub fn with_gateway_name(self, name: &str) -> Self {
        *self.gateway_name.lock().expect("gateway name") = Some(name.to_string());
        self
    }

    /// The gateway reports no name over REST.
    pub fn with_no_gateway_name(self) -> Self {
        *self.gateway_name.lock().expect("gateway name") = None;
        self
    }

    /// The Designer list answers these lists in turn, one per read; the last
    /// one repeats. Without this no session is ever open.
    pub fn with_designers_script(self, lists: Vec<Vec<DesignerSeen>>) -> Self {
        self.designers.lock().expect("designers").lists = lists;
        self
    }

    /// Every Designer list read fails with `network_error`.
    pub fn with_designers_error(self) -> Self {
        self.designers.lock().expect("designers").fail = true;
        self
    }

    /// How many times the Designer list was read.
    pub fn designers_reads(&self) -> usize {
        self.calls.lock().expect("calls").designers
    }

    /// Add a project holding `members` (checkout-layout path to bytes).
    pub fn with_project(
        self,
        name: &str,
        parent: Option<&str>,
        inheritable: bool,
        members: &[(&str, Vec<u8>)],
    ) -> Self {
        let record = ProjectRecord {
            name: name.to_string(),
            title: Some(name.to_string()),
            description: None,
            enabled: true,
            parent: parent.map(str::to_string),
            inheritable: Some(inheritable),
            invalid_parent: None,
            default_db: None,
            tag_provider: None,
            user_source: None,
            extra: BTreeMap::new(),
        };
        self.projects.lock().expect("projects").insert(
            name.to_string(),
            StoredProject {
                record,
                members: members
                    .iter()
                    .map(|(path, bytes)| ((*path).to_string(), bytes.clone()))
                    .collect(),
            },
        );
        self
    }

    /// Add a project that inherits from `parent` (D-06). Like
    /// [`Self::with_project`], not inheritable itself.
    pub fn with_child(self, name: &str, parent: &str, members: &[(&str, Vec<u8>)]) -> Self {
        self.with_project(name, Some(parent), false, members)
    }

    /// After the first import of any project, the next `times` exports of
    /// `name` each find it edited since the one before (one extra member whose
    /// bytes change on every edit), then it stops changing. With `times` far
    /// beyond what a run reads it never settles. Applied to the deployed
    /// project itself it makes the verify read-back differ, which is how an
    /// `imported_unverified` outcome is produced.
    pub fn mutate_child_after_import(&self, name: &str, times: usize) {
        self.churn.lock().expect("churn").insert(
            name.to_string(),
            Churn {
                remaining: times,
                edits: 0,
            },
        );
    }

    /// Every later export of `name` fails with `network_error`.
    pub fn fail_export_of(&self, name: &str) {
        self.failing_exports
            .lock()
            .expect("failing exports")
            .push(name.to_string());
    }

    /// Add `n` other projects named `Other-000`, `Other-001` and so on, each
    /// with only a `project.json`.
    pub fn with_names(mut self, n: usize) -> Self {
        for i in 0..n {
            self = self.with_project(
                &format!("Other-{i:03}"),
                None,
                false,
                &[("project.json", b"{}".to_vec())],
            );
        }
        self
    }

    /// How many times `name` was exported through the trait.
    pub fn export_count(&self, name: &str) -> usize {
        self.calls
            .lock()
            .expect("calls")
            .export
            .iter()
            .filter(|n| *n == name)
            .count()
    }

    /// Every name exported through the trait, in call order.
    pub fn export_calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").export.clone()
    }

    /// Every name looked up through the trait, in call order.
    pub fn find_calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").find.clone()
    }

    /// How many times the project list was read.
    pub fn list_count(&self) -> usize {
        self.calls.lock().expect("calls").list
    }

    /// Every import sent, in call order: the project and `overwrite`.
    pub fn import_calls(&self) -> Vec<(String, bool)> {
        self.calls.lock().expect("calls").import.clone()
    }

    /// Every call in order: `list`, `find:<name>`, `export:<name>`,
    /// `import:<name>:<overwrite>`.
    pub fn call_log(&self) -> Vec<String> {
        self.calls.lock().expect("calls").order.clone()
    }

    /// Make every later import answer `"changes": []` (it still adopts the
    /// zip).
    pub fn answer_empty_changes(&self) {
        self.empty_changes.store(true, Ordering::SeqCst);
    }

    /// Change one stored member of `name` behind the verb's back, as another
    /// administrator editing the project between the plan and the run would.
    pub fn change_live(&self, name: &str, path: &str, bytes: &[u8]) {
        let mut projects = self.projects.lock().expect("projects");
        let stored = projects.get_mut(name).expect("the fake holds the project");
        stored.members.insert(path.to_string(), bytes.to_vec());
    }

    /// The stored members of every project but `name`, for the check that an
    /// apply left the others byte-identical.
    pub fn snapshot_except(&self, name: &str) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        self.projects
            .lock()
            .expect("projects")
            .iter()
            .filter(|(project, _)| project.as_str() != name)
            .map(|(project, stored)| (project.clone(), stored.members.clone()))
            .collect()
    }

    /// Whether the fake holds `name`.
    pub fn has_project(&self, name: &str) -> bool {
        self.projects.lock().expect("projects").contains_key(name)
    }

    /// The stored (not exported) bytes of one member.
    pub fn stored_member(&self, name: &str, path: &str) -> Option<Vec<u8>> {
        self.projects
            .lock()
            .expect("projects")
            .get(name)
            .and_then(|stored| stored.members.get(path).cloned())
    }

    /// One member of a fresh export of `name`, as bytes. Not a trait call:
    /// it is not counted in [`Self::export_calls`].
    pub fn export_member(&self, name: &str, path: &str) -> Vec<u8> {
        let projects = self.projects.lock().expect("projects");
        let stored = projects.get(name).expect("the fake holds the project");
        let bytes = stored
            .members
            .get(path)
            .expect("the project holds the member");
        self.exported_bytes(stored, path, bytes)
    }

    /// The bytes an export carries for one stored member.
    fn exported_bytes(&self, stored: &StoredProject, path: &str, bytes: &[u8]) -> Vec<u8> {
        if path == "project.json" {
            return export_project_json(&stored.record, bytes);
        }
        if path.rsplit('/').next() == Some("resource.json") {
            let n = self.stamp.fetch_add(1, Ordering::SeqCst) + 1;
            return stamp_descriptor(bytes, n);
        }
        bytes.to_vec()
    }
}

/// C8b: the gateway rewrites `attributes.lastModification` and
/// `attributes.lastModificationSignature` on export. Value shapes are not in
/// the captures (module doc).
fn stamp_descriptor(bytes: &[u8], n: u64) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return bytes.to_vec();
    };
    let Some(attributes) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("attributes"))
        .and_then(|attributes| attributes.as_object_mut())
    else {
        return bytes.to_vec();
    };
    attributes.insert(
        "lastModification".to_string(),
        serde_json::json!({
            "actor": "fake-gateway",
            "timestamp": format!("2026-10-22T13:57:17.{n:06}Z"),
        }),
    );
    attributes.insert(
        "lastModificationSignature".to_string(),
        serde_json::Value::String(format!("{n:064x}")),
    );
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

/// C8b, C8c: `project.json` gains `description` and `parent` on the way out.
fn export_project_json(record: &ProjectRecord, bytes: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return bytes.to_vec();
    };
    let Some(object) = value.as_object_mut() else {
        return bytes.to_vec();
    };
    object.entry("description").or_insert_with(|| {
        serde_json::Value::String(record.description.clone().unwrap_or_default())
    });
    object
        .entry("parent")
        .or_insert_with(|| serde_json::Value::String(record.parent.clone().unwrap_or_default()));
    serde_json::to_vec(&value).unwrap_or_else(|_| bytes.to_vec())
}

#[async_trait::async_trait]
impl ProjectGateway for FakeProjects {
    async fn projects(&self) -> Result<Vec<ProjectRecord>, CoreError> {
        {
            let mut calls = self.calls.lock().expect("calls");
            calls.list += 1;
            calls.order.push("list".to_string());
        }
        let projects = self.projects.lock().expect("projects");
        Ok(projects.values().rev().map(|p| p.record.clone()).collect())
    }

    async fn project_find(&self, name: &str) -> Result<Option<ProjectRecord>, CoreError> {
        {
            let mut calls = self.calls.lock().expect("calls");
            calls.find.push(name.to_string());
            calls.order.push(format!("find:{name}"));
        }
        let projects = self.projects.lock().expect("projects");
        Ok(projects.get(name).map(|p| p.record.clone()))
    }

    async fn export_bytes(&self, name: &str) -> Result<Vec<u8>, CoreError> {
        {
            let mut calls = self.calls.lock().expect("calls");
            calls.export.push(name.to_string());
            calls.order.push(format!("export:{name}"));
        }
        if self
            .failing_exports
            .lock()
            .expect("failing exports")
            .iter()
            .any(|failing| failing == name)
        {
            return Err(CoreError::Network {
                url: format!("{FAKE_BASE}/data/api/v1/projects/export/{name}"),
                source: None,
                observation: None,
            });
        }
        let mut projects = self.projects.lock().expect("projects");
        if self.imported.load(Ordering::SeqCst)
            && let Some(churn) = self.churn.lock().expect("churn").get_mut(name)
            && churn.remaining > 0
            && let Some(stored) = projects.get_mut(name)
        {
            churn.remaining -= 1;
            churn.edits += 1;
            stored.members.insert(
                "ignition/script-python/settling/code.py".to_string(),
                format!("edit {}\n", churn.edits).into_bytes(),
            );
        }
        let Some(stored) = projects.get(name) else {
            return Err(CoreError::NotFound {
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/projects/export/{name}")),
            });
        };
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (path, bytes) in &stored.members {
            writer
                .start_file(path.as_str(), options)
                .expect("zip entry");
            writer
                .write_all(&self.exported_bytes(stored, path, bytes))
                .expect("zip bytes");
        }
        Ok(writer.finish().expect("zip finish").into_inner())
    }

    async fn designers(&self) -> Result<Vec<DesignerSeen>, CoreError> {
        let read = {
            let mut calls = self.calls.lock().expect("calls");
            calls.designers += 1;
            calls.designers - 1
        };
        let script = self.designers.lock().expect("designers");
        if script.fail {
            return Err(CoreError::Network {
                url: format!("{FAKE_BASE}/data/api/v1/designers"),
                source: None,
                observation: None,
            });
        }
        Ok(script
            .lists
            .get(read.min(script.lists.len().saturating_sub(1)))
            .cloned()
            .unwrap_or_default())
    }

    async fn gateway_name(&self) -> Result<Option<String>, CoreError> {
        Ok(self.gateway_name.lock().expect("gateway name").clone())
    }

    async fn tag_provider_names(&self) -> Result<Vec<String>, CoreError> {
        self.tag_calls.lock().expect("tag calls").list += 1;
        self.calls
            .lock()
            .expect("calls")
            .order
            .push("tag-providers".to_string());
        if self.tag_list_fails.load(Ordering::SeqCst) {
            return Err(CoreError::Network {
                url: format!("{FAKE_BASE}/data/api/v1/resources/list/ignition/tag-provider"),
                source: None,
                observation: None,
            });
        }
        Ok(self
            .tag_providers
            .lock()
            .expect("tag providers")
            .iter()
            .map(|(name, _)| name.clone())
            .collect())
    }

    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
        self.tag_calls
            .lock()
            .expect("tag calls")
            .exports
            .push(provider.to_string());
        self.calls
            .lock()
            .expect("calls")
            .order
            .push(format!("tag-export:{provider}"));
        if self
            .failing_tag_exports
            .lock()
            .expect("failing tag exports")
            .iter()
            .any(|failing| failing == provider)
        {
            return Err(CoreError::Network {
                url: format!("{FAKE_BASE}/data/api/v1/tags/export"),
                source: None,
                observation: None,
            });
        }
        let providers = self.tag_providers.lock().expect("tag providers");
        match providers.iter().find(|(name, _)| name == provider) {
            Some((_, export)) => Ok(export.clone()),
            None => Err(CoreError::NotFound {
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/tags/export")),
            }),
        }
    }

    async fn tag_import(
        &self,
        provider: &str,
        path: &str,
        policy: CollisionPolicy,
        body: Vec<u8>,
    ) -> Result<TagImportAnswer, CoreError> {
        self.tag_calls.lock().expect("tag calls").imports.push((
            provider.to_string(),
            path.to_string(),
            policy.as_wire().to_string(),
        ));
        self.calls
            .lock()
            .expect("calls")
            .order
            .push(format!("tag-import:{provider}:{path}:{}", policy.as_wire()));
        if policy != CollisionPolicy::Abort {
            return Err(CoreError::Internal("fake: policy not modelled".to_string()));
        }
        let mut providers = self.tag_providers.lock().expect("tag providers");
        let Some((_, export)) = providers.iter_mut().find(|(name, _)| name == provider) else {
            return Err(CoreError::NotFound {
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/tags/import")),
            });
        };
        let mut tree: serde_json::Value =
            serde_json::from_slice(export).expect("fake tag tree is JSON");
        let body: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|err| CoreError::Internal(format!("fake: import body is not JSON: {err}")))?;
        let Some(nodes) = body.get("tags").and_then(serde_json::Value::as_array) else {
            return Err(CoreError::Internal(
                "fake: import body is not {\"tags\":[...]}".to_string(),
            ));
        };
        let in_types = path.split('/').next() == Some("_types_");
        let nodes: Vec<serde_json::Value> = nodes
            .iter()
            .cloned()
            .map(|mut node| {
                let is_types_root = path.is_empty()
                    && node.get("name").and_then(serde_json::Value::as_str) == Some("_types_");
                if !in_types && !is_types_root {
                    self.seed_created(&mut node);
                }
                node
            })
            .collect();
        let mut counts = (0u64, 0u64);
        abort_into(tag_level(&mut tree, path), &nodes, &mut counts);
        *export = serde_json::to_vec(&tree).expect("fake tag tree serializes");
        let (success, failure) = counts;
        Ok(TagImportAnswer {
            success_count: Some(success),
            failure_count: Some(failure),
            bad_quality: vec!["Bad (527)".to_string(); failure as usize],
        })
    }

    async fn project_import(
        &self,
        name: &str,
        zip: Vec<u8>,
        overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        {
            let mut calls = self.calls.lock().expect("calls");
            calls.import.push((name.to_string(), overwrite));
            calls.order.push(format!("import:{name}:{overwrite}"));
        }
        self.imported.store(true, Ordering::SeqCst);
        let mut projects = self.projects.lock().expect("projects");
        if !overwrite && projects.contains_key(name) {
            return Err(CoreError::ImportDenied {
                project: name.to_string(),
                problem: "the project already exists".to_string(),
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/projects/import/{name}")),
            });
        }
        let members = adopt(&zip);
        let mut record = projects
            .get(name)
            .map(|p| p.record.clone())
            .unwrap_or_else(|| ProjectRecord {
                name: name.to_string(),
                title: Some(name.to_string()),
                description: None,
                enabled: true,
                parent: None,
                inheritable: Some(false),
                invalid_parent: None,
                default_db: None,
                tag_provider: None,
                user_source: None,
                extra: BTreeMap::new(),
            });
        if let Some(meta) = members
            .get("project.json")
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        {
            record.title = meta
                .get("title")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            record.enabled = meta
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            record.inheritable = meta.get("inheritable").and_then(|v| v.as_bool());
            record.parent = meta
                .get("parent")
                .and_then(|v| v.as_str())
                .filter(|p| !p.is_empty())
                .map(str::to_string);
        }
        projects.insert(name.to_string(), StoredProject { record, members });
        let changes = if self.empty_changes.load(Ordering::SeqCst) {
            serde_json::json!([])
        } else {
            serde_json::json!([{ "name": name }])
        };
        Ok(ImportOutcome {
            response: serde_json::json!({ "success": true, "changes": changes }),
        })
    }
}

impl FakeProjects {
    /// C12: an AtomicTag created with `value` and no `defaultValue` is stored
    /// with a `defaultValue`. Follows folder children only.
    fn seed_created(&self, node: &mut serde_json::Value) {
        let tag_type = node
            .get("tagType")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        match tag_type.as_deref() {
            Some("AtomicTag") => {
                let Some(object) = node.as_object_mut() else {
                    return;
                };
                if let Some(value) = object.get("value").cloned()
                    && !object.contains_key("defaultValue")
                {
                    let seed = self
                        .created_default
                        .lock()
                        .expect("created default")
                        .clone()
                        .unwrap_or(value);
                    object.insert("defaultValue".to_string(), seed);
                    if let Some(runtime) = self.created_value.lock().expect("created value").clone()
                    {
                        object.insert("value".to_string(), runtime);
                    }
                }
            }
            Some("Folder") => {
                if let Some(kids) = node
                    .get_mut("tags")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for kid in kids {
                        self.seed_created(kid);
                    }
                }
            }
            _ => {}
        }
    }
}

/// C8d, K2: the members the gateway keeps from an import zip. `project.json`,
/// each `resource.json` and exactly the files it lists; everything else
/// (unlisted files, dotfiles, directories) is dropped without an error, and a
/// listed file the zip lacks is rewritten out of `files`.
fn adopt(zip: &[u8]) -> BTreeMap<String, Vec<u8>> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("the import is a zip");
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("zip entry");
        if entry.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("zip bytes");
        files.insert(entry.name().to_string(), bytes);
    }
    let mut kept = BTreeMap::new();
    if let Some(project) = files.get("project.json") {
        kept.insert("project.json".to_string(), project.clone());
    }
    for (path, bytes) in &files {
        let Some(dir) = path.strip_suffix("resource.json") else {
            continue;
        };
        if !dir.is_empty() && !dir.ends_with('/') {
            continue;
        }
        let Ok(mut descriptor) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            continue;
        };
        let listed: Vec<String> = descriptor
            .get("files")
            .and_then(|f| f.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut present = Vec::new();
        for file in listed {
            if let Some(content) = files.get(&format!("{dir}{file}")) {
                kept.insert(format!("{dir}{file}"), content.clone());
                present.push(serde_json::Value::String(file));
            }
        }
        if let Some(object) = descriptor.as_object_mut() {
            object.insert("files".to_string(), serde_json::Value::Array(present));
        }
        kept.insert(
            path.clone(),
            serde_json::to_vec(&descriptor).expect("descriptor json"),
        );
    }
    kept
}

/// An `Abort` import of `nodes` into `level`, as 26-LIVE-CAPTURES.md C1
/// recorded on 8.3.6: a new node is added and counts once per node in it
/// (`F` holding `T1` answered `successCount` 2); a folder whose name is
/// already a folder counts as a success and its children are imported into
/// the existing one (C1d); any other name already at the level is a failure
/// (`failureCount` 1, quality `Bad`, sub-code 527, C1c). `counts` is
/// `(success, failure)`.
fn abort_into(
    level: &mut Vec<serde_json::Value>,
    nodes: &[serde_json::Value],
    counts: &mut (u64, u64),
) {
    for node in nodes {
        let name = node.get("name").and_then(serde_json::Value::as_str);
        let is_folder =
            |n: &serde_json::Value| n.get("tagType").and_then(|t| t.as_str()) == Some("Folder");
        let taken = level
            .iter()
            .position(|existing| existing.get("name").and_then(serde_json::Value::as_str) == name);
        match taken {
            _ if name.is_none() => counts.1 += 1,
            None => {
                counts.0 += node_count(node);
                level.push(node.clone());
            }
            Some(at) if is_folder(&level[at]) && is_folder(node) => {
                counts.0 += 1;
                let kids = node
                    .get("tags")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                abort_into(children(&mut level[at]), &kids, counts);
            }
            Some(_) => counts.1 += 1,
        }
    }
}

/// `node` and every node below it.
fn node_count(node: &serde_json::Value) -> u64 {
    1 + node
        .get("tags")
        .and_then(serde_json::Value::as_array)
        .map(|tags| tags.iter().map(node_count).sum())
        .unwrap_or(0)
}

/// The `tags` array of the node at `path` (slash-separated) below `tree`,
/// creating each folder the path names that the tree lacks, as an import at a
/// parent path does.
fn tag_level<'a>(tree: &'a mut serde_json::Value, path: &str) -> &'a mut Vec<serde_json::Value> {
    let mut node = tree;
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        let tags = children(node);
        let at = match tags.iter().position(|child| {
            child.get("name").and_then(serde_json::Value::as_str) == Some(segment)
        }) {
            Some(at) => at,
            None => {
                tags.push(serde_json::json!({"name": segment, "tagType": "Folder", "tags": []}));
                tags.len() - 1
            }
        };
        node = &mut tags[at];
    }
    children(node)
}

/// The `tags` array of `node`, created empty when absent.
fn children(node: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
    let object = node.as_object_mut().expect("a tag node is an object");
    object
        .entry("tags")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
        .expect("tags is an array")
}

/// A [`ProjectGateway`] that forwards every read and panics on the write: the
/// proof that a path never reaches `project_import` (DPRJ-05, the check).
pub struct ForbidProjectWrites<T: ProjectGateway>(pub T);

#[async_trait::async_trait]
impl<T: ProjectGateway> ProjectGateway for ForbidProjectWrites<T> {
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

    async fn tag_provider_names(&self) -> Result<Vec<String>, CoreError> {
        self.0.tag_provider_names().await
    }

    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
        self.0.tag_export(provider).await
    }

    async fn project_import(
        &self,
        name: &str,
        _zip: Vec<u8>,
        _overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        panic!("check must not write: project_import({name}) was called");
    }

    async fn tag_import(
        &self,
        provider: &str,
        path: &str,
        _policy: CollisionPolicy,
        _body: Vec<u8>,
    ) -> Result<TagImportAnswer, CoreError> {
        panic!("check must not write: tag_import({provider}, {path}) was called");
    }
}

/// A [`ReleaseRecordWriter`] that always fails with `transport_failed`, to
/// prove a record that cannot be written after a verified import is a warning
/// and not a failure of the deploy.
pub struct FailingRecordWriter;

#[async_trait::async_trait]
impl ReleaseRecordWriter for FailingRecordWriter {
    async fn write_release_record(
        &self,
        _gateway: &str,
        _project: &str,
        _bytes: &[u8],
    ) -> Result<(), CoreError> {
        Err(CoreError::TransportFailed {
            operation: "write".to_string(),
            node: "nodes.master".to_string(),
            detail: "the record cannot be written".to_string(),
        })
    }
}
