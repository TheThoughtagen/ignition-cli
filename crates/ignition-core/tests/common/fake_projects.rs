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
//! 527) in a 200 answer. The export answers the tree as it stands.
//!
//! The other policies (26-08) copy the capture rows exactly:
//!
//! - **MergeOverwrite** applies the keys the body holds and keeps every other
//!   key, the live `value` included (C2, C6 Q, p26-change): members and
//!   `parameters` merge by name, so a stripped instance keeps its member
//!   values (C7 I3).
//! - **Overwrite** replaces the node: an omitted property is gone (C6 P); a
//!   top-level AtomicTag sent without `value` comes back holding its
//!   `defaultValue` (p26-rollback-shape V), or no value without one; an
//!   instance member sent without `value` loses it (p26-rollback-shape I1/M1,
//!   C7 I4); a carried `value` is kept (V2, I2).
//! - **Neither removes an omitted child** (C4, C5): a folder in the body has
//!   its own keys merged or replaced and its listed children imported with
//!   the same policy; the others stay.
//! - `Ignore` is refused as not modelled.
//!
//! Knobs for the cases the captures describe but a run cannot cause:
//! `with_tag_import_failure` answers one import with a failure count,
//! `with_value_write_after_first_import` stands in for a runtime write while
//! the run is sending, `with_tag_churn` makes reads after the first import
//! differ for a while (Pitfall 6), and `with_tag_child_order_flips` lists
//! children in reverse order on every other export (F-P1).
//!
//! For the rollback (26-11): `with_tag_import_partial` applies the first
//! nodes of one import and answers a failure for the rest (a batch that is
//! not clean may have applied part of it), `with_write_after_import` sets one
//! key of one node right after a given import (a runtime write between the
//! failure and the rollback, or another administrator's change),
//! `with_project_import_denied` refuses every project import, and
//! `tag_provider_delete` removes a provider and its tree when the signature
//! is the current one (the undo of a provider the run created). Three knobs
//! stand in for other clients around a create: `with_provider_create_conflict`
//! (another client created it first: `resource_exists`),
//! `with_provider_create_lost_answer` (applied, answer lost) and
//! `with_provider_resigned_after_import` (changed after the create).
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
use ignition_core::client::tags::{
    CollisionPolicy, TagImportAnswer, TagProviderCreate, TagProviderModify, TagProviderRecord,
};
use ignition_core::deploy::apply::preflight::DesignerSeen;
use ignition_core::deploy::tag_delete::{
    DeleteAnswer, PathResult, RouteHandshake, TAG_DELETE_HTTP_500, TagDeleteRoute,
};
use ignition_core::deploy::transport::projects::{ReleaseRecordWriter, TagRecordWriter};
use ignition_core::error::{CoreError, SignatureReason};

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
    /// The scripted tag behaviours (26-08 knobs).
    tag_script: Mutex<TagScript>,
    /// The `defaultValue` a created AtomicTag gets instead of its `value`.
    created_default: Mutex<Option<serde_json::Value>>,
    /// The `value` a created AtomicTag holds instead of the sent one.
    created_value: Mutex<Option<serde_json::Value>>,
    /// Each listed tag provider's resource record (26-10). A provider
    /// listed by `with_tag_provider` gets an enabled record with an empty
    /// `config` and no description, the suite's `SETTINGS`.
    provider_records: Mutex<BTreeMap<String, TagProviderRecord>>,
    /// The signature every provider modify carried, in call order.
    provider_modify_signatures: Mutex<Vec<String>>,
    /// Provider creates and modifies answer success and change nothing.
    provider_writes_ignored: AtomicBool,
    /// The last provider signature handed out.
    provider_signature_seq: AtomicU64,
    /// Every project import is refused (`import_denied`).
    deny_project_import: AtomicBool,
    /// How the next provider create goes when a test scripts it.
    provider_create_script: Mutex<Option<CreateScript>>,
    /// Providers another client re-signs right after the 1-based tag import.
    resign_after: Mutex<Vec<(usize, String)>>,
    /// How many provider reads right after a provider create fail with the
    /// gateway's own error (26-REVIEW: a 500 under load), and how many are
    /// still due.
    reads_failing_after_create: (AtomicU64, AtomicU64),
}

/// A scripted provider create (26-11 security fix).
#[derive(Clone, Copy)]
enum CreateScript {
    /// Another client created the provider first: the create answers
    /// `resource_exists` and the provider is that client's.
    Conflict,
    /// The create is applied (with another description when `differ`) and
    /// its answer is lost: a `network_error` after the request was sent.
    LostAnswer { differ: bool },
}

/// The export of a provider with no tag (C14): the structural `_types_`.
const EMPTY_PROVIDER_EXPORT: &str =
    r#"{"name":"","tagType":"Provider","tags":[{"name":"_types_","tagType":"Folder"}]}"#;

/// The tag reads the check made, for footprint assertions.
#[derive(Default)]
struct TagCalls {
    list: usize,
    exports: Vec<String>,
    /// Every tag import as `(provider, path, policy)`, in call order.
    imports: Vec<(String, String, String)>,
    /// The body of every tag import, in call order.
    bodies: Vec<serde_json::Value>,
}

/// The 26-08 knobs: tag behaviours a run cannot cause by itself.
#[derive(Default)]
struct TagScript {
    /// The 1-based imports that answer one failure (`Bad`), applying
    /// nothing.
    fail_imports: Vec<usize>,
    /// The 1-based import that applies only its first `n` nodes and answers
    /// a failure for each of the others: `(import, n)` (26-11).
    partial_import: Option<(usize, usize)>,
    /// Writes `(import, provider, path, key, value)` applied once, right
    /// after the 1-based import `import` answered (whatever it answered): a
    /// stand-in for a runtime value write (`key` `value`) or another
    /// administrator's change while the run is sending or rolling back.
    writes_after: Vec<(usize, String, String, String, serde_json::Value)>,
    /// Nodes `(import, provider, parent, node)` added once below `parent`,
    /// right after the 1-based import `import` answered: another
    /// administrator's new node while the run is sending (26-19).
    adds_after: Vec<(usize, String, String, serde_json::Value)>,
    /// How many exports after the first import carry a changing extra node.
    churn: usize,
    /// How many churned exports were answered.
    churned: usize,
    /// Reverse every `tags` array on every other export.
    flip_order: bool,
    /// Whether the last export was answered in order, for `flip_order`.
    in_order: bool,
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
            tag_script: Mutex::new(TagScript::default()),
            created_default: Mutex::new(None),
            created_value: Mutex::new(None),
            provider_records: Mutex::new(BTreeMap::new()),
            provider_modify_signatures: Mutex::new(Vec::new()),
            provider_writes_ignored: AtomicBool::new(false),
            provider_signature_seq: AtomicU64::new(0),
            deny_project_import: AtomicBool::new(false),
            provider_create_script: Mutex::new(None),
            resign_after: Mutex::new(Vec::new()),
            reads_failing_after_create: (AtomicU64::new(0), AtomicU64::new(0)),
        }
    }

    /// The first `n` provider reads after each provider create answer the
    /// gateway's own error (`internal`), as the pair did once under load
    /// (26-21 open point 3).
    pub fn with_provider_reads_failing_after_create(self, n: u64) -> Self {
        self.reads_failing_after_create.0.store(n, Ordering::SeqCst);
        self
    }

    /// The resource record of tag provider `name`, as the gateway holds it
    /// now (26-10).
    pub fn provider_record(&self, name: &str) -> Option<TagProviderRecord> {
        self.provider_records
            .lock()
            .expect("provider records")
            .get(name)
            .cloned()
    }

    /// Replace the resource record of the listed provider `name` with these
    /// settings and a new signature, as an administrator would.
    pub fn set_provider_settings(
        &self,
        name: &str,
        enabled: bool,
        description: Option<&str>,
        config: serde_json::Value,
    ) {
        let signature = self.next_signature(name);
        let mut records = self.provider_records.lock().expect("provider records");
        let record = records.get_mut(name).expect("the provider is listed");
        record.enabled = enabled;
        record.config = config;
        record.signature = Some(signature);
        match description {
            Some(text) => {
                record
                    .extra
                    .insert("description".to_string(), serde_json::json!(text));
            }
            None => {
                record.extra.remove("description");
            }
        }
    }

    /// The signature every provider modify carried, in call order.
    pub fn provider_modify_signatures(&self) -> Vec<String> {
        self.provider_modify_signatures
            .lock()
            .expect("modify signatures")
            .clone()
    }

    /// Provider creates and modifies answer success and change nothing, as a
    /// gateway that drops what it does not keep would (C11 dropped an
    /// unknown settings key).
    pub fn with_provider_writes_ignored(self) -> Self {
        self.provider_writes_ignored.store(true, Ordering::SeqCst);
        self
    }

    /// A new signature for `name`'s record.
    fn next_signature(&self, name: &str) -> String {
        let n = self.provider_signature_seq.fetch_add(1, Ordering::SeqCst) + 1;
        format!("sig-{name}-{n:04}")
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
        let signature = self.next_signature(name);
        self.provider_records
            .lock()
            .expect("provider records")
            .entry(name.to_string())
            .or_insert_with(|| TagProviderRecord {
                name: name.to_string(),
                enabled: true,
                config: serde_json::json!({}),
                metrics: serde_json::Value::Null,
                healthchecks: serde_json::Value::Null,
                signature: Some(signature),
                extra: BTreeMap::from([("collection".to_string(), serde_json::json!("core"))]),
            });
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

    /// Replace the tree of the listed provider `provider`, as if the gateway
    /// changed between two reads (26-05, the plan-stale cases).
    pub fn set_tag_tree(&self, provider: &str, export: serde_json::Value) {
        let bytes = serde_json::to_vec(&export).expect("tag tree serializes");
        let mut providers = self.tag_providers.lock().expect("tag providers");
        let entry = providers
            .iter_mut()
            .find(|(name, _)| name == provider)
            .expect("the provider is listed");
        entry.1 = bytes;
    }

    /// Every tag import as `(provider, path, policy)`, in call order.
    pub fn tag_import_calls(&self) -> Vec<(String, String, String)> {
        self.tag_calls.lock().expect("tag calls").imports.clone()
    }

    /// The body of every tag import, in call order.
    pub fn tag_import_bodies(&self) -> Vec<serde_json::Value> {
        self.tag_calls.lock().expect("tag calls").bodies.clone()
    }

    /// The `n`th tag import (1-based) answers HTTP 200 with one failure
    /// (quality `Bad`) and applies nothing (C1 F1: only the counts tell).
    /// Given again, each named import fails.
    pub fn with_tag_import_failure(self, n: usize) -> Self {
        self.tag_script
            .lock()
            .expect("tag script")
            .fail_imports
            .push(n);
        self
    }

    /// The `n`th tag import (1-based) applies only the first `applied`
    /// nodes of its body and answers one failure (quality `Bad`) for each
    /// of the others (26-11: a batch that is not clean may have applied
    /// part of its nodes; C1 shows the answer counts per node).
    pub fn with_tag_import_partial(self, n: usize, applied: usize) -> Self {
        self.tag_script.lock().expect("tag script").partial_import = Some((n, applied));
        self
    }

    /// Right after the first tag import, the tag at `path` of `provider`
    /// takes `value`, as a runtime write while the run is still sending.
    pub fn with_value_write_after_first_import(
        self,
        provider: &str,
        path: &str,
        value: serde_json::Value,
    ) -> Self {
        self.with_write_after_import(1, provider, path, "value", value)
    }

    /// Right after the `n`th tag import (1-based) answers, whatever it
    /// answered, the node at `path` of `provider` takes `key: value`: a
    /// runtime value write (`key` `value`) between a failure and the
    /// rollback, or another administrator's configuration change (26-11).
    pub fn with_write_after_import(
        self,
        n: usize,
        provider: &str,
        path: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Self {
        self.tag_script
            .lock()
            .expect("tag script")
            .writes_after
            .push((
                n,
                provider.to_string(),
                path.to_string(),
                key.to_string(),
                value,
            ));
        self
    }

    /// Right after the `n`th tag import (1-based) answers, whatever it
    /// answered, `node` (named) is added below `parent` of `provider`:
    /// another administrator's node in a folder the run created (26-19).
    pub fn with_tag_added_after_import(
        self,
        n: usize,
        provider: &str,
        parent: &str,
        node: serde_json::Value,
    ) -> Self {
        self.tag_script
            .lock()
            .expect("tag script")
            .adds_after
            .push((n, provider.to_string(), parent.to_string(), node));
        self
    }

    /// Remove the node at provider-relative `path` of `provider` and
    /// everything below it, as the tagDelete route's `deleteTags` does
    /// (26-17 live record: a folder delete removes its children, an
    /// instance delete its members). `None` when the provider is not
    /// listed, `Some(false)` when no node is at `path`. Test support for
    /// [`FakeTagDeleteRoute`] only: it is not a [`ProjectGateway`] method,
    /// so the gateway surface still has no tag delete.
    pub fn delete_tag_path(&self, provider: &str, path: &str) -> Option<bool> {
        self.delete_tag_paths(provider, &[path.to_string()])
            .map(|removed| removed[0])
    }

    /// [`Self::delete_tag_path`] of each of `paths` in order, on one read
    /// and one write of the provider's tree (26-20: a call of 500 paths
    /// against a 3 MB tree), each answering whether a node was at its path
    /// when its turn came.
    pub fn delete_tag_paths(&self, provider: &str, paths: &[String]) -> Option<Vec<bool>> {
        let mut providers = self.tag_providers.lock().expect("tag providers");
        let (_, export) = providers.iter_mut().find(|(name, _)| name == provider)?;
        let mut tree: serde_json::Value =
            serde_json::from_slice(export).expect("fake tag tree is JSON");
        let removed = paths
            .iter()
            .map(|path| remove_at(&mut tree, path))
            .collect();
        *export = serde_json::to_vec(&tree).expect("fake tag tree serializes");
        Some(removed)
    }

    /// Another client creates the provider of the next create first, with
    /// settings and a signature of its own: the create answers
    /// `resource_exists` (26-11 security fix).
    pub fn with_provider_create_conflict(self) -> Self {
        *self.provider_create_script.lock().expect("create script") = Some(CreateScript::Conflict);
        self
    }

    /// The next provider create is applied, with a description of its own
    /// when `differ`, and then answers `network_error`, as a create whose
    /// answer was lost after the request went out (26-11 security fix).
    pub fn with_provider_create_lost_answer(self, differ: bool) -> Self {
        *self.provider_create_script.lock().expect("create script") =
            Some(CreateScript::LostAnswer { differ });
        self
    }

    /// Right after the `n`th tag import (1-based) another client changes
    /// provider `name`, which gives its record a new signature.
    pub fn with_provider_resigned_after_import(self, n: usize, name: &str) -> Self {
        self.resign_after
            .lock()
            .expect("resign")
            .push((n, name.to_string()));
        self
    }

    /// Every project import answers the gateway's own refusal
    /// (`import_denied`) and changes nothing (26-11).
    pub fn with_project_import_denied(self) -> Self {
        self.deny_project_import.store(true, Ordering::SeqCst);
        self
    }

    /// The first `n` exports after the first tag import each hold an extra
    /// live-only root node whose `documentation` changes every read, so no
    /// two of them agree; the export after them is the tree as it stands.
    pub fn with_tag_churn(self, n: usize) -> Self {
        self.tag_script.lock().expect("tag script").churn = n;
        self
    }

    /// Every other export lists the children of every `tags` array in reverse
    /// order (F-P1: two reads of one tree need not list children alike).
    pub fn with_tag_child_order_flips(self) -> Self {
        self.tag_script.lock().expect("tag script").flip_order = true;
        self
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
        let Some((_, export)) = providers.iter().find(|(name, _)| name == provider) else {
            return Err(CoreError::NotFound {
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/tags/export")),
            });
        };
        let mut script = self.tag_script.lock().expect("tag script");
        script.in_order = !script.in_order;
        let churning = self.tag_imported() && script.churned < script.churn;
        let flip = script.flip_order && !script.in_order;
        if !churning && !flip {
            return Ok(export.clone());
        }
        let mut tree: serde_json::Value =
            serde_json::from_slice(export).expect("fake tag tree is JSON");
        if churning {
            script.churned += 1;
            children(&mut tree).push(serde_json::json!({
                "name": "Settling", "tagType": "AtomicTag",
                "documentation": format!("read {}", script.churned)
            }));
        }
        if flip {
            reverse_children(&mut tree);
        }
        Ok(serde_json::to_vec(&tree).expect("fake tag tree serializes"))
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
        if policy == CollisionPolicy::Ignore {
            return Err(CoreError::Internal("fake: policy not modelled".to_string()));
        }
        let call = self.tag_calls.lock().expect("tag calls").imports.len();
        if let Ok(sent) = serde_json::from_slice::<serde_json::Value>(&body) {
            self.tag_calls.lock().expect("tag calls").bodies.push(sent);
        }
        let (fails, partial) = {
            let script = self.tag_script.lock().expect("tag script");
            (
                script.fail_imports.contains(&call),
                script
                    .partial_import
                    .filter(|(n, _)| *n == call)
                    .map(|(_, applied)| applied),
            )
        };
        let answer = if fails {
            TagImportAnswer {
                success_count: Some(0),
                failure_count: Some(1),
                bad_quality: vec!["Bad".to_string()],
                ..TagImportAnswer::default()
            }
        } else {
            self.apply_import(provider, path, policy, &body, partial)?
        };
        self.apply_scripted_writes(call);
        Ok(answer)
    }

    async fn tag_provider_record(
        &self,
        name: &str,
    ) -> Result<Option<TagProviderRecord>, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .order
            .push(format!("tag-provider-find:{name}"));
        let due = &self.reads_failing_after_create.1;
        // A compare-exchange loop rather than `fetch_update` (deprecated on
        // newer stable) or `try_update` (newer than the MSRV).
        let mut left = due.load(Ordering::SeqCst);
        let take = loop {
            if left == 0 {
                break false;
            }
            match due.compare_exchange(left, left - 1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break true,
                Err(now) => left = now,
            }
        };
        if take {
            return Err(CoreError::Internal(
                "fake: the gateway answered 500".to_string(),
            ));
        }
        Ok(self.provider_record(name))
    }

    async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError> {
        for item in body {
            self.calls
                .lock()
                .expect("calls")
                .order
                .push(format!("tag-provider-create:{}", item.name));
            self.reads_failing_after_create.1.store(
                self.reads_failing_after_create.0.load(Ordering::SeqCst),
                Ordering::SeqCst,
            );
            if self.provider_writes_ignored.load(Ordering::SeqCst) {
                continue;
            }
            let script = self
                .provider_create_script
                .lock()
                .expect("create script")
                .take();
            let signature = self.next_signature(&item.name);
            let mut extra =
                BTreeMap::from([("collection".to_string(), serde_json::json!(item.collection))]);
            let description = match script {
                Some(CreateScript::Conflict | CreateScript::LostAnswer { differ: true }) => {
                    Some("another client's provider".to_string())
                }
                _ => item.description.clone(),
            };
            if let Some(text) = &description {
                extra.insert("description".to_string(), serde_json::json!(text));
            }
            let mut records = self.provider_records.lock().expect("provider records");
            if records.contains_key(&item.name) {
                return Err(CoreError::ResourceExists {
                    name: item.name.clone(),
                    endpoint: Some(format!(
                        "{FAKE_BASE}/data/api/v1/resources/ignition/tag-provider"
                    )),
                });
            }
            records.insert(
                item.name.clone(),
                TagProviderRecord {
                    name: item.name.clone(),
                    enabled: item.enabled,
                    config: item.config.clone(),
                    metrics: serde_json::Value::Null,
                    healthchecks: serde_json::Value::Null,
                    signature: Some(signature),
                    extra,
                },
            );
            self.tag_providers
                .lock()
                .expect("tag providers")
                .push((item.name.clone(), EMPTY_PROVIDER_EXPORT.as_bytes().to_vec()));
            match script {
                Some(CreateScript::Conflict) => {
                    return Err(CoreError::ResourceExists {
                        name: item.name.clone(),
                        endpoint: Some(format!(
                            "{FAKE_BASE}/data/api/v1/resources/ignition/tag-provider"
                        )),
                    });
                }
                Some(CreateScript::LostAnswer { .. }) => {
                    return Err(CoreError::Network {
                        url: format!("{FAKE_BASE}/data/api/v1/resources/ignition/tag-provider"),
                        source: None,
                        observation: None,
                    });
                }
                None => {}
            }
        }
        Ok(())
    }

    async fn tag_provider_modify(&self, body: &[TagProviderModify]) -> Result<(), CoreError> {
        for item in body {
            self.calls
                .lock()
                .expect("calls")
                .order
                .push(format!("tag-provider-modify:{}", item.name));
            self.provider_modify_signatures
                .lock()
                .expect("modify signatures")
                .push(item.signature.clone());
            let endpoint = Some(format!(
                "{FAKE_BASE}/data/api/v1/resources/ignition/tag-provider"
            ));
            let current = self.provider_record(&item.name);
            let Some(current) = current else {
                return Err(CoreError::NotFound { endpoint });
            };
            if current.signature.as_deref() != Some(item.signature.as_str()) {
                return Err(CoreError::ResourceSignatureRejected {
                    name: item.name.clone(),
                    reason: SignatureReason::Stale,
                    endpoint,
                });
            }
            if self.provider_writes_ignored.load(Ordering::SeqCst) {
                let signature = self.next_signature(&item.name);
                if let Some(record) = self
                    .provider_records
                    .lock()
                    .expect("provider records")
                    .get_mut(&item.name)
                {
                    record.signature = Some(signature);
                }
                continue;
            }
            self.set_provider_settings(
                &item.name,
                item.enabled,
                item.description.as_deref(),
                item.config.clone(),
            );
        }
        Ok(())
    }

    async fn tag_provider_delete(&self, name: &str, signature: &str) -> Result<(), CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .order
            .push(format!("tag-provider-delete:{name}"));
        let endpoint = Some(format!(
            "{FAKE_BASE}/data/api/v1/resources/ignition/tag-provider"
        ));
        let mut records = self.provider_records.lock().expect("provider records");
        let Some(current) = records.get(name) else {
            return Err(CoreError::NotFound { endpoint });
        };
        if current.signature.as_deref() != Some(signature) {
            return Err(CoreError::ResourceSignatureRejected {
                name: name.to_string(),
                reason: SignatureReason::Stale,
                endpoint,
            });
        }
        records.remove(name);
        self.tag_providers
            .lock()
            .expect("tag providers")
            .retain(|(listed, _)| listed != name);
        Ok(())
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
        if self.deny_project_import.load(Ordering::SeqCst) {
            return Err(CoreError::ImportDenied {
                project: name.to_string(),
                problem: "the gateway refused the import".to_string(),
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/projects/import/{name}")),
            });
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
    /// Apply one tag import to the provider's tree (module doc): all of its
    /// nodes, or only the first `partial` with a failure for each other.
    fn apply_import(
        &self,
        provider: &str,
        path: &str,
        policy: CollisionPolicy,
        body: &[u8],
        partial: Option<usize>,
    ) -> Result<TagImportAnswer, CoreError> {
        let mut providers = self.tag_providers.lock().expect("tag providers");
        let Some((_, export)) = providers.iter_mut().find(|(name, _)| name == provider) else {
            return Err(CoreError::NotFound {
                endpoint: Some(format!("{FAKE_BASE}/data/api/v1/tags/import")),
            });
        };
        let mut tree: serde_json::Value =
            serde_json::from_slice(export).expect("fake tag tree is JSON");
        let body: serde_json::Value = serde_json::from_slice(body)
            .map_err(|err| CoreError::Internal(format!("fake: import body is not JSON: {err}")))?;
        let Some(nodes) = body.get("tags").and_then(serde_json::Value::as_array) else {
            return Err(CoreError::Internal(
                "fake: import body is not {\"tags\":[...]}".to_string(),
            ));
        };
        let in_types = path.split('/').next() == Some("_types_");
        let mut nodes: Vec<serde_json::Value> = nodes
            .iter()
            .cloned()
            .map(|mut node| {
                let is_types_root = path.is_empty()
                    && node.get("name").and_then(serde_json::Value::as_str) == Some("_types_");
                if policy == CollisionPolicy::Abort && !in_types && !is_types_root {
                    self.seed_created(&mut node);
                }
                node
            })
            .collect();
        let mut counts = (0u64, 0u64);
        if let Some(applied) = partial {
            let dropped = nodes.len().saturating_sub(applied);
            nodes.truncate(applied);
            counts.1 += dropped as u64;
        }
        let mut collided = Vec::new();
        import_into(
            tag_level(&mut tree, path),
            &nodes,
            policy,
            &mut counts,
            (path, &mut collided),
        );
        *export = serde_json::to_vec(&tree).expect("fake tag tree serializes");
        let (success, failure) = counts;
        Ok(TagImportAnswer {
            success_count: Some(success),
            failure_count: Some(failure),
            bad_quality: vec!["Bad (527)".to_string(); failure as usize],
            // C1: an Abort collision names the node's full path.
            collided: collided
                .into_iter()
                .map(|at| format!("[{provider}]{at}"))
                .collect(),
            collisions_unnamed: 0,
        })
    }

    /// The scripted writes due after the 1-based tag import `call`.
    fn apply_scripted_writes(&self, call: usize) {
        let resign: Vec<String> = {
            let mut list = self.resign_after.lock().expect("resign");
            let (due, rest) = std::mem::take(&mut *list)
                .into_iter()
                .partition(|(n, _)| *n == call);
            *list = rest;
            due.into_iter().map(|(_, name)| name).collect()
        };
        for name in resign {
            let signature = self.next_signature(&name);
            if let Some(record) = self
                .provider_records
                .lock()
                .expect("provider records")
                .get_mut(&name)
            {
                record.signature = Some(signature);
            }
        }
        let due: Vec<(String, String, String, serde_json::Value)> = {
            let mut script = self.tag_script.lock().expect("tag script");
            let (due, rest) = std::mem::take(&mut script.writes_after)
                .into_iter()
                .partition(|(n, ..)| *n == call);
            script.writes_after = rest;
            due.into_iter()
                .map(|(_, provider, path, key, value)| (provider, path, key, value))
                .collect()
        };
        let adds: Vec<(String, String, serde_json::Value)> = {
            let mut script = self.tag_script.lock().expect("tag script");
            let (due, rest) = std::mem::take(&mut script.adds_after)
                .into_iter()
                .partition(|(n, ..)| *n == call);
            script.adds_after = rest;
            due.into_iter()
                .map(|(_, provider, parent, node)| (provider, parent, node))
                .collect()
        };
        for (target, parent, node) in adds {
            let mut providers = self.tag_providers.lock().expect("tag providers");
            let entry = providers
                .iter_mut()
                .find(|(name, _)| *name == target)
                .expect("the provider is listed");
            let mut tree: serde_json::Value =
                serde_json::from_slice(&entry.1).expect("fake tag tree is JSON");
            tag_level(&mut tree, &parent).push(node);
            entry.1 = serde_json::to_vec(&tree).expect("fake tag tree serializes");
        }
        for (target, at, key, value) in due {
            let mut providers = self.tag_providers.lock().expect("tag providers");
            let entry = providers
                .iter_mut()
                .find(|(name, _)| *name == target)
                .expect("the written provider is listed");
            let mut tree: serde_json::Value =
                serde_json::from_slice(&entry.1).expect("fake tag tree is JSON");
            let (parent, name) = at.rsplit_once('/').unwrap_or(("", at.as_str()));
            let node = tag_level(&mut tree, parent)
                .iter_mut()
                .find(|n| n.get("name").and_then(serde_json::Value::as_str) == Some(name))
                .expect("the written tag exists");
            node.as_object_mut()
                .expect("a tag node is an object")
                .insert(key, value);
            entry.1 = serde_json::to_vec(&tree).expect("fake tag tree serializes");
        }
    }

    /// Whether any tag import has been sent.
    fn tag_imported(&self) -> bool {
        !self.tag_calls.lock().expect("tag calls").imports.is_empty()
    }

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

/// An import of `nodes` into `level` under `policy`, as 26-LIVE-CAPTURES.md
/// recorded on 8.3.6 (module doc). A new node is added and counts once per
/// node in it (`F` holding `T1` answered `successCount` 2). A folder whose
/// name is already a folder counts as a success, its own keys are merged
/// (Abort and MergeOverwrite) or replaced (Overwrite), and its listed
/// children are imported into the existing one with the same policy: no
/// policy removes an omitted child (C1d, C4, C5). Any other name already at
/// the level fails under Abort (`failureCount` 1, quality `Bad`, sub-code
/// 527, C1c), merges under MergeOverwrite and is replaced under Overwrite.
/// `counts` is `(success, failure)`.
fn import_into(
    level: &mut Vec<serde_json::Value>,
    nodes: &[serde_json::Value],
    policy: CollisionPolicy,
    counts: &mut (u64, u64),
    (at, collided): (&str, &mut Vec<String>),
) {
    for node in nodes {
        let name = node.get("name").and_then(serde_json::Value::as_str);
        let below = |name: &str| {
            if at.is_empty() {
                name.to_string()
            } else {
                format!("{at}/{name}")
            }
        };
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
                let own = level[at].as_object_mut().expect("a tag node is an object");
                if policy == CollisionPolicy::Overwrite {
                    own.retain(|key, _| key == "name" || key == "tags");
                }
                if policy != CollisionPolicy::Abort {
                    for (key, value) in node.as_object().into_iter().flatten() {
                        if key != "name" && key != "tags" {
                            own.insert(key.clone(), value.clone());
                        }
                    }
                }
                let kids = node
                    .get("tags")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let path = below(name.unwrap_or_default());
                import_into(
                    children(&mut level[at]),
                    &kids,
                    policy,
                    counts,
                    (&path, collided),
                );
            }
            Some(_) if policy == CollisionPolicy::Abort => {
                counts.1 += 1;
                collided.push(below(name.unwrap_or_default()));
            }
            Some(at) if policy == CollisionPolicy::MergeOverwrite => {
                counts.0 += 1;
                merge_into(&mut level[at], node);
            }
            Some(at) => {
                counts.0 += 1;
                let mut replaced = node.clone();
                let object = replaced.as_object_mut().expect("a tag node is an object");
                if object.get("tagType").and_then(|t| t.as_str()) == Some("AtomicTag")
                    && !object.contains_key("value")
                    && let Some(default) = object.get("defaultValue").cloned()
                {
                    object.insert("value".to_string(), default);
                }
                level[at] = replaced;
            }
        }
    }
}

/// MergeOverwrite of `body` onto `existing`: every key the body holds is set,
/// every other key stays. Members (`tags`) merge by name and `parameters` by
/// key, so an instance sent stripped keeps its member values (C7 I3).
fn merge_into(existing: &mut serde_json::Value, body: &serde_json::Value) {
    let Some(object) = existing.as_object_mut() else {
        return;
    };
    for (key, value) in body.as_object().into_iter().flatten() {
        match (key.as_str(), object.get_mut(key)) {
            ("name", _) => {}
            ("tags", Some(serde_json::Value::Array(members))) => {
                for member in value.as_array().into_iter().flatten() {
                    let name = member.get("name").and_then(serde_json::Value::as_str);
                    match members
                        .iter_mut()
                        .find(|m| m.get("name").and_then(serde_json::Value::as_str) == name)
                    {
                        Some(found) => merge_into(found, member),
                        None => members.push(member.clone()),
                    }
                }
            }
            ("parameters", Some(serde_json::Value::Object(params))) => {
                for (param, entry) in value.as_object().into_iter().flatten() {
                    match (params.get_mut(param), entry) {
                        (
                            Some(serde_json::Value::Object(have)),
                            serde_json::Value::Object(sent),
                        ) => {
                            for (k, v) in sent {
                                have.insert(k.clone(), v.clone());
                            }
                        }
                        _ => {
                            params.insert(param.clone(), entry.clone());
                        }
                    }
                }
            }
            _ => {
                object.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Reverse the children of every `tags` array below `node`.
fn reverse_children(node: &mut serde_json::Value) {
    if let Some(kids) = node
        .get_mut("tags")
        .and_then(serde_json::Value::as_array_mut)
    {
        kids.reverse();
        for kid in kids {
            reverse_children(kid);
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

    async fn tag_provider_record(
        &self,
        name: &str,
    ) -> Result<Option<TagProviderRecord>, CoreError> {
        self.0.tag_provider_record(name).await
    }

    async fn tag_provider_create(&self, body: &[TagProviderCreate]) -> Result<(), CoreError> {
        panic!(
            "check must not write: tag_provider_create({:?}) was called",
            body.iter().map(|b| b.name.as_str()).collect::<Vec<_>>()
        );
    }

    async fn tag_provider_modify(&self, body: &[TagProviderModify]) -> Result<(), CoreError> {
        panic!(
            "check must not write: tag_provider_modify({:?}) was called",
            body.iter().map(|b| b.name.as_str()).collect::<Vec<_>>()
        );
    }

    async fn tag_provider_delete(&self, name: &str, _signature: &str) -> Result<(), CoreError> {
        panic!("check must not write: tag_provider_delete({name}) was called");
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

/// A [`TagRecordWriter`] that always fails with `transport_failed` (26-06),
/// for the engine cases where a tag record cannot be written after the tags
/// and the project are verified.
pub struct FailingTagRecordWriter;

#[async_trait::async_trait]
impl TagRecordWriter for FailingTagRecordWriter {
    async fn write_tag_record(
        &self,
        _gateway: &str,
        _provider: &str,
        _bytes: &[u8],
    ) -> Result<(), CoreError> {
        Err(CoreError::TransportFailed {
            operation: "write".to_string(),
            node: "nodes.master".to_string(),
            detail: "the tag record cannot be written".to_string(),
        })
    }
}

/// Remove the node at provider-relative `path` of `tree` and everything
/// below it; `false` when no node is there.
fn remove_at(tree: &mut serde_json::Value, path: &str) -> bool {
    let mut node = tree;
    let segments: Vec<&str> = path.split('/').collect();
    let (last, parents) = segments.split_last().expect("a path has a segment");
    for segment in parents {
        let Some(next) = node
            .get_mut("tags")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|tags| {
                tags.iter_mut().find(|child| {
                    child.get("name").and_then(serde_json::Value::as_str) == Some(segment)
                })
            })
        else {
            return false;
        };
        node = next;
    }
    let Some(tags) = node
        .get_mut("tags")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    let before = tags.len();
    tags.retain(|child| child.get("name").and_then(serde_json::Value::as_str) != Some(last));
    tags.len() != before
}

/// One scripted answer of [`FakeTagDeleteRoute::handshake`] (26-17 install
/// record: a freshly imported route answered HTTP 500 and 405 before its
/// `version` answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeHandshake {
    /// The route answers its version handshake.
    Ready,
    /// HTTP 500: WebDev still compiling the route.
    Http500,
    /// HTTP 405: the route is not registered yet.
    Http405,
    /// HTTP 403: the caller lacks the deploy role.
    AuthDenied,
}

/// One call the fake route received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeRouteCall {
    /// `version`.
    Handshake,
    /// `delete` of `(provider, paths)`.
    Delete(String, Vec<String>),
}

/// A [`TagDeleteRoute`] over [`FakeProjects`]' tag tree (26-19): `delete`
/// removes each path and everything below it from the same tree the fake's
/// export answers, as the 26-17 live record shows (`folder delete removes
/// children: yes`, `instance delete removes members: yes`, a good delete
/// answers quality `Good`, `good: true`, `existsAfter: false`; an absent
/// path answers `Bad_NotFound`, `good: false`).
///
/// The handshake answers a scripted sequence, the last one repeating; the
/// allowlist is the handshake's `providers`, and a delete for any other
/// provider is refused `provider_not_allowed` before anything is removed.
/// A delete of no path panics: ign must never send one. Every call is
/// recorded.
pub struct FakeTagDeleteRoute<'a> {
    tags: &'a FakeProjects,
    providers: Vec<String>,
    handshakes: Mutex<std::collections::VecDeque<FakeHandshake>>,
    last: Mutex<FakeHandshake>,
    calls: Mutex<Vec<FakeRouteCall>>,
    fail_delete: Mutex<Vec<usize>>,
    deletes: AtomicU64,
    /// The handshake's `maxPaths` (26-17: 500).
    max_paths: Mutex<usize>,
    /// Paths a delete answers as removed and leaves in place (26-20).
    kept: Mutex<Vec<String>>,
}

impl<'a> FakeTagDeleteRoute<'a> {
    /// A ready route over `tags` allowing `providers`.
    pub fn new(tags: &'a FakeProjects, providers: &[&str]) -> Self {
        Self {
            tags,
            providers: providers.iter().map(|p| (*p).to_string()).collect(),
            handshakes: Mutex::new(std::collections::VecDeque::new()),
            last: Mutex::new(FakeHandshake::Ready),
            calls: Mutex::new(Vec::new()),
            fail_delete: Mutex::new(Vec::new()),
            deletes: AtomicU64::new(0),
            max_paths: Mutex::new(500),
            kept: Mutex::new(Vec::new()),
        }
    }

    /// The handshake answers `maxPaths` `n` instead of 500 (26-20).
    pub fn with_max_paths(self, n: usize) -> Self {
        *self.max_paths.lock().expect("max paths") = n;
        self
    }

    /// A delete of `path` answers a clean removal and leaves the node in
    /// place: a route that lies, for the verify to catch (26-20).
    pub fn with_kept_path(self, path: &str) -> Self {
        self.kept.lock().expect("kept").push(path.to_string());
        self
    }

    /// The handshake answers `sequence` in order, then its last answer
    /// for every later call.
    pub fn with_handshakes(self, sequence: &[FakeHandshake]) -> Self {
        if let Some(last) = sequence.last() {
            *self.last.lock().expect("last") = *last;
        }
        self.handshakes
            .lock()
            .expect("handshakes")
            .extend(sequence.iter().copied());
        self
    }

    /// The `n`th delete call (1-based) fails with `network_error` and
    /// removes nothing.
    pub fn with_delete_failure(self, n: usize) -> Self {
        self.fail_delete.lock().expect("fail delete").push(n);
        self
    }

    /// Every call, in order.
    pub fn calls(&self) -> Vec<FakeRouteCall> {
        self.calls.lock().expect("calls").clone()
    }

    /// Every delete call as `(provider, paths)`, in order.
    pub fn delete_calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls()
            .into_iter()
            .filter_map(|call| match call {
                FakeRouteCall::Delete(provider, paths) => Some((provider, paths)),
                FakeRouteCall::Handshake => None,
            })
            .collect()
    }

    /// How many handshakes were made.
    pub fn handshake_count(&self) -> usize {
        self.calls()
            .iter()
            .filter(|call| **call == FakeRouteCall::Handshake)
            .count()
    }
}

#[async_trait::async_trait]
impl TagDeleteRoute for FakeTagDeleteRoute<'_> {
    async fn handshake(&self) -> Result<RouteHandshake, CoreError> {
        self.calls
            .lock()
            .expect("calls")
            .push(FakeRouteCall::Handshake);
        let answer = self
            .handshakes
            .lock()
            .expect("handshakes")
            .pop_front()
            .unwrap_or(*self.last.lock().expect("last"));
        let endpoint = Some(format!("{FAKE_BASE}/system/webdev/ign-cli/cli/tagDelete"));
        match answer {
            FakeHandshake::Ready => Ok(RouteHandshake {
                route_version: ignition_core::webdev::ROUTE_BUNDLE_VERSION.to_string(),
                min_cli: "0.0.0".to_string(),
                providers: self.providers.clone(),
                max_paths: *self.max_paths.lock().expect("max paths"),
                max_path_chars: 1000,
            }),
            FakeHandshake::Http500 => {
                Err(CoreError::Internal(format!("{TAG_DELETE_HTTP_500} (fake)")))
            }
            FakeHandshake::Http405 => Err(CoreError::RoutesNotDeployed {
                project: "ign-cli".to_string(),
                route: "tagDelete".to_string(),
                endpoint,
            }),
            FakeHandshake::AuthDenied => Err(CoreError::Auth {
                status: 403,
                endpoint,
            }),
        }
    }

    async fn delete(&self, provider: &str, paths: &[String]) -> Result<DeleteAnswer, CoreError> {
        assert!(!paths.is_empty(), "ign sent the tagDelete route no path");
        self.calls
            .lock()
            .expect("calls")
            .push(FakeRouteCall::Delete(provider.to_string(), paths.to_vec()));
        let n =
            usize::try_from(self.deletes.fetch_add(1, Ordering::SeqCst) + 1).expect("a call count");
        if self.fail_delete.lock().expect("fail delete").contains(&n) {
            return Err(CoreError::Network {
                url: format!("{FAKE_BASE}/system/webdev/ign-cli/cli/tagDelete"),
                source: None,
                observation: None,
            });
        }
        if !self.providers.iter().any(|p| p == provider) {
            return Err(CoreError::WebdevRouteError {
                code: "provider_not_allowed".to_string(),
                message: format!("provider {provider} is not in the route's allowlist"),
                endpoint: None,
            });
        }
        let kept = self.kept.lock().expect("kept").clone();
        let removable: Vec<String> = paths
            .iter()
            .filter(|path| !kept.contains(path))
            .cloned()
            .collect();
        let mut answers = self
            .tags
            .delete_tag_paths(provider, &removable)
            .unwrap_or_else(|| vec![false; removable.len()])
            .into_iter();
        let results = paths
            .iter()
            .map(|path| {
                if kept.contains(path) {
                    return PathResult {
                        path: path.clone(),
                        quality: Some("Good".to_string()),
                        good: Some(true),
                        exists_after: false,
                    };
                }
                let removed = answers.next().unwrap_or(false);
                PathResult {
                    path: path.clone(),
                    quality: Some(if removed { "Good" } else { "Bad_NotFound" }.to_string()),
                    good: Some(removed),
                    exists_after: false,
                }
            })
            .collect();
        Ok(DeleteAnswer {
            provider: provider.to_string(),
            results,
            elapsed_ms: 1,
        })
    }
}
