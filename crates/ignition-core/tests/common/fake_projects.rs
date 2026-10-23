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
//!   entries and its export 0; the WHK-Global zip held 1,174 and its export
//!   0. The export here holds file entries only.
//! - **Volatile stamps (C8b).** Each export writes
//!   `attributes.lastModification` and `attributes.lastModificationSignature`
//!   into every `resource.json` that has an `attributes` object, with a new
//!   value on every export, so a comparison that passes has ignored them. The
//!   captures record the two field names and that 196 of 245 differing
//!   WHK-Global members differed in nothing else.
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
}

/// The in-memory gateway project store.
pub struct FakeProjects {
    projects: Mutex<BTreeMap<String, StoredProject>>,
    calls: Mutex<Calls>,
    stamp: AtomicU64,
    empty_changes: AtomicBool,
}

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
        }
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
        let projects = self.projects.lock().expect("projects");
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

    async fn project_import(
        &self,
        name: &str,
        _zip: Vec<u8>,
        _overwrite: bool,
    ) -> Result<ImportOutcome, CoreError> {
        panic!("check must not write: project_import({name}) was called");
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
