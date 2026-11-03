//! The blocking work behind `impl ApplyHost for LocalFsTransport`
//! (Phase 23, D-01).
//!
//! Every function here runs on a blocking thread. Layout below `data_dir`:
//!
//! ```text
//! config/resources/<mode>/...                       the collection
//! config/resources/core/...                         the core collection (`.core/` keys)
//! ign-deploy/<gateway>/<mode>/ledger.json           the declared-names ledger
//! ign-deploy/<gateway>/<mode>/journal.json          the journal of the run in flight
//! ign-deploy/<gateway>/<mode>/snapshots/<run>/      files/<path>, core/<path>, ledger.json,
//!                                                   record.json
//! ign-deploy/<gateway>/<mode>/staging/<run>/        files/<path>, core/<path>, restore/<path>,
//!                                                   restore-core/<path>, ledger.json,
//!                                                   work.tsv, journal.<phase>.json,
//!                                                   record.applying.json
//! ```
//!
//! A `.core/` key (quick 261102-cef, A2) is a file of `config/resources/core`,
//! which must already be a real directory: it is never created, and a run
//! that touches a core key refuses before its first move without it. Its
//! copies use the `core/` (and `restore-core/`) slots, and an emptied
//! directory there is removed down to `<module>/<type>` only.
//!
//! All file work goes through [`Dir`] handles that never follow a symlink.
//! Nothing here writes a temp, backup or dot-named entry inside the
//! collection.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{ApplyFault, ApplySetup, LocalFsTransport};
use crate::deploy::apply::fingerprint::LEDGER_KEY;
use crate::deploy::apply::host::{
    BatchReport, BatchSpec, PRERENDERED_PHASES, Prepared, ProbeReport, restoring_journal,
};
use crate::deploy::apply::journal::{JOURNAL_FILE, JOURNAL_MAX_BYTES, Journal, JournalPhase};
use crate::deploy::apply::record::{RECORD_FILE, RECORD_MAX_BYTES, RecordState, RunRecord};
use crate::deploy::apply::run_store::RunId;
use crate::deploy::component_problem;
use crate::deploy::layout::{KeyCollection, RelPath, core_key, split_key};
use crate::deploy::ledger::{
    LEDGER_FILE, LEDGER_MAX_BYTES, LedgerRoot, journal_location, location_of, record_rel,
    staging_rel,
};
use crate::deploy::transport::batch::{
    RENAME_RETRIES, WORK_FILE, WorkList, WorkOp, journal_file, record_file,
};
use crate::deploy::transport::local_fs::{Dir, FsError, Kind, Visibility, shown};
use crate::deploy::transport::{
    CORE_COLLECTION, READ_MAX_ENTRIES, READ_MAX_FILE_BYTES, READ_MAX_TOTAL_BYTES,
    apply_key_problem, core_probe_problem, core_tag_types, emptied_dir_floor,
};
use crate::deploy::tree::{RenderedTree, sha256_hex};
use crate::error::CoreError;

/// The largest work list read back (64 MiB).
const WORK_MAX_BYTES: u64 = 67_108_864;

/// The pause before the first move retry; it grows with each attempt.
const RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(50);

/// The longest list of paths a failure message names.
const NAMED_MAX: usize = 10;

/// Where a run's files are, below `data_dir`.
struct Layout {
    gateway: String,
    mode: String,
    run: String,
}

impl Layout {
    fn new(setup: &ApplySetup, run: &str) -> Layout {
        Layout {
            gateway: setup.gateway.clone(),
            mode: setup.mode.clone(),
            run: run.to_string(),
        }
    }

    fn under<'a>(&'a self, dir: &[&'a str], tail: &[&'a str]) -> Vec<&'a str> {
        let mut parts = vec!["ign-deploy", self.gateway.as_str(), self.mode.as_str()];
        parts.extend_from_slice(dir);
        parts.extend_from_slice(tail);
        parts
    }

    fn journal(&self) -> Vec<&str> {
        self.under(&[], &[JOURNAL_FILE])
    }

    fn ledger(&self) -> Vec<&str> {
        self.under(&[], &[LEDGER_FILE])
    }

    fn snapshot<'a>(&'a self, tail: &[&'a str]) -> Vec<&'a str> {
        self.under(&["snapshots", self.run.as_str()], tail)
    }

    fn stage<'a>(&'a self, tail: &[&'a str]) -> Vec<&'a str> {
        self.under(&["staging", self.run.as_str()], tail)
    }

    fn staging_rel(&self) -> String {
        staging_rel(&self.gateway, &self.mode, &self.run)
    }
}

/// The collection the apply key `key` names and the components of its
/// collection path (quick 261102-cef, P1).
fn key_parts(key: &str) -> (KeyCollection, Vec<&str>) {
    let (collection, rest) = split_key(key);
    (collection, rest.split('/').collect())
}

/// The snapshot and staging slot of `collection`: `files` or `core`.
fn slot(collection: KeyCollection) -> &'static str {
    match collection {
        KeyCollection::Mode => "files",
        KeyCollection::Core => "core",
    }
}

/// The staging slot a restore copy of `collection` goes through.
fn restore_slot(collection: KeyCollection) -> &'static str {
    match collection {
        KeyCollection::Mode => "restore",
        KeyCollection::Core => "restore-core",
    }
}

/// `prefix`, the slot of `key`'s collection, then its collection path.
fn in_slot<'a>(prefix: &[&'a str], key: &'a str) -> Vec<&'a str> {
    let (collection, rest) = key_parts(key);
    let mut parts = prefix.to_vec();
    parts.push(slot(collection));
    parts.extend(rest);
    parts
}

/// Whether any of `keys` names the core collection.
fn touches_core<'a>(mut keys: impl Iterator<Item = &'a str>) -> bool {
    keys.any(|key| split_key(key).0 == KeyCollection::Core)
}

/// The two collections a run works in: the mode collection, and `core` when
/// the run touches a core key (`None` otherwise, never created).
struct Colls {
    mode: Option<Dir>,
    core: Option<Dir>,
}

impl Colls {
    /// The directory of `collection`, when it is open.
    fn get(&self, collection: KeyCollection) -> Option<&Dir> {
        match collection {
            KeyCollection::Mode => self.mode.as_ref(),
            KeyCollection::Core => self.core.as_ref(),
        }
    }
}

/// What a live path is, for the pre-image check.
#[derive(Debug, PartialEq, Eq)]
enum Live {
    Absent,
    Hash(String),
    /// Not a regular file, or reached through a symlink or non-directory.
    Unsafe,
}

impl Live {
    fn matches(&self, pre: Option<&str>) -> bool {
        match (self, pre) {
            (Live::Absent, None) => true,
            (Live::Hash(h), Some(p)) => h == p,
            _ => false,
        }
    }
}

fn sleep_for(attempt: u32) {
    std::thread::sleep(RETRY_PAUSE * attempt);
}

/// What a commit's move phase shares with its caller.
struct MoveControl {
    /// Set when a [`ApplyFault::HangAfterMove`] stopped the commit after its
    /// k-th move; the async caller then waits where a test can abort.
    hang: bool,
    /// When the commit began: the time budget counts from here.
    started: std::time::Instant,
}

impl LocalFsTransport {
    fn apply_cfg(&self, op: &str) -> Result<&ApplySetup, CoreError> {
        self.apply.as_ref().ok_or_else(|| {
            self.mirror_fail(
                op,
                "this transport has no apply half; build it with apply_host_for_node",
            )
        })
    }

    fn fs_fail(&self, op: &str, what: &str, e: &FsError) -> CoreError {
        self.mirror_fail(op, format!("{what}: {}", e.describe()))
    }

    fn data_root(&self, op: &str) -> Result<Dir, CoreError> {
        Dir::root(&self.data_dir).map_err(|e| self.fs_fail(op, "data_dir", &e))
    }

    /// The mode collection below `config/resources`, which must exist; the
    /// collection itself is created when `create` and absent otherwise
    /// (`Ok(None)`).
    fn collection(
        &self,
        op: &str,
        root: &Dir,
        mode: &str,
        create: bool,
    ) -> Result<Option<Dir>, CoreError> {
        let resources = root.open(&["config", "resources"]).map_err(|e| match e {
            FsError::Missing => self.mirror_fail(
                op,
                "config/resources does not exist; is data_dir a gateway data directory?",
            ),
            other => self.fs_fail(op, "config/resources", &other),
        })?;
        if create {
            resources
                .ensure(&[mode])
                .map(Some)
                .map_err(|e| self.fs_fail(op, "the mode collection", &e))
        } else {
            match resources.open(&[mode]) {
                Ok(dir) => Ok(Some(dir)),
                Err(FsError::Missing) => Ok(None),
                Err(e) => Err(self.fs_fail(op, "the mode collection", &e)),
            }
        }
    }

    /// `config/resources/core`, which must already exist as a real
    /// directory: it is never created (quick 261102-cef, P7). Absent, a
    /// symlink or not a directory refuses, naming why.
    fn core_collection(&self, op: &str, root: &Dir) -> Result<Dir, CoreError> {
        let resources = root.open(&["config", "resources"]).map_err(|e| match e {
            FsError::Missing => self.mirror_fail(
                op,
                "config/resources does not exist; is data_dir a gateway data directory?",
            ),
            other => self.fs_fail(op, "config/resources", &other),
        })?;
        match resources.kind(&[CORE_COLLECTION]) {
            Ok(Kind::Dir) => {}
            Ok(Kind::Missing) => {
                return Err(self.mirror_fail(
                    op,
                    "config/resources/core does not exist; a redundant pair's tag units are written there and it is never created",
                ));
            }
            Ok(Kind::Symlink) => {
                return Err(self.mirror_fail(op, "config/resources/core is a symlink; refused"));
            }
            Ok(_) => {
                return Err(self.mirror_fail(op, "config/resources/core is not a directory"));
            }
            Err(e) => return Err(self.fs_fail(op, "config/resources/core", &e)),
        }
        resources
            .open(&[CORE_COLLECTION])
            .map_err(|e| self.fs_fail(op, "config/resources/core", &e))
    }

    /// `config/resources/core` when it exists, `None` when it is missing. A
    /// symlink or a non-directory still refuses ([`Self::core_collection`]).
    fn core_collection_if_present(&self, op: &str, root: &Dir) -> Result<Option<Dir>, CoreError> {
        let resources = root.open(&["config", "resources"]).map_err(|e| match e {
            FsError::Missing => self.mirror_fail(
                op,
                "config/resources does not exist; is data_dir a gateway data directory?",
            ),
            other => self.fs_fail(op, "config/resources", &other),
        })?;
        match resources.kind(&[CORE_COLLECTION]) {
            Ok(Kind::Missing) => Ok(None),
            _ => self.core_collection(op, root).map(Some),
        }
    }

    /// The core collection when any of `keys` names it, else `None`.
    fn core_for<'a>(
        &self,
        op: &str,
        root: &Dir,
        keys: impl Iterator<Item = &'a str>,
    ) -> Result<Option<Dir>, CoreError> {
        if touches_core(keys) {
            self.core_collection(op, root).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The live state of the file at apply key `key` in `colls`.
    fn live_key(&self, op: &str, colls: &Colls, key: &RelPath) -> Result<Live, CoreError> {
        let (collection, parts) = key_parts(key.as_str());
        self.live_parts(op, colls.get(collection), &parts, key)
    }

    /// The live state of the file at `parts` in `coll`, named `path`.
    fn live_parts(
        &self,
        op: &str,
        coll: Option<&Dir>,
        parts: &[&str],
        path: &RelPath,
    ) -> Result<Live, CoreError> {
        let Some(coll) = coll else {
            return Ok(Live::Absent);
        };
        match coll.read(parts, READ_MAX_FILE_BYTES) {
            Ok(None) => Ok(Live::Absent),
            Ok(Some(bytes)) => Ok(Live::Hash(sha256_hex(&bytes))),
            Err(FsError::Changed) => Ok(Live::Unsafe),
            Err(FsError::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData => Ok(Live::Unsafe),
            Err(e) => Err(self.fs_fail(op, &format!("reading {path}"), &e)),
        }
    }

    fn live_ledger(&self, op: &str, root: &Dir, lay: &Layout) -> Result<Live, CoreError> {
        match root.read(&lay.ledger(), LEDGER_MAX_BYTES as u64) {
            Ok(None) => Ok(Live::Absent),
            Ok(Some(bytes)) => Ok(Live::Hash(sha256_hex(&bytes))),
            Err(FsError::Changed) => Ok(Live::Unsafe),
            Err(FsError::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData => Ok(Live::Unsafe),
            Err(e) => Err(self.fs_fail(op, "reading the ledger", &e)),
        }
    }

    /// Move with the bounded retry (C7a-4): only an i/o refusal is retried,
    /// and nothing is lost by one.
    fn move_with_retry(
        &self,
        src: &Dir,
        from: &[&str],
        dst: &Dir,
        to: &[&str],
        inject: bool,
    ) -> Result<(), FsError> {
        let mut attempt = 0;
        loop {
            // An injected fault refuses every attempt the way a held target
            // does, so it takes the retry path and ends in the real failure.
            let result = if inject {
                Err(FsError::Io(std::io::Error::other("injected move fault")))
            } else {
                src.rename(from, dst, to)
            };
            match result {
                Err(FsError::Io(_)) if attempt < RENAME_RETRIES => {
                    attempt += 1;
                    sleep_for(attempt);
                }
                other => return other,
            }
        }
    }

    // ---- probe -----------------------------------------------------------

    pub(super) fn probe_blocking(&self) -> Result<ProbeReport, CoreError> {
        const OP: &str = "probe";
        let setup = self.apply_cfg(OP)?;
        let root = self.data_root(OP)?;
        let mut problems = Vec::new();
        match root.open(&["config", "resources"]) {
            Ok(resources) => match resources.open(&[setup.mode.as_str()]) {
                Ok(coll) => {
                    if !coll.writable() {
                        problems.push("the mode collection is not writable".to_string());
                    }
                }
                Err(FsError::Missing) => {
                    if !resources.writable() {
                        problems.push(
                            "config/resources is not writable, so the mode collection cannot be created"
                                .to_string(),
                        );
                    }
                }
                Err(e) => problems.push(format!(
                    "the mode collection cannot be used: {}",
                    e.describe()
                )),
            },
            Err(e) => problems.push(format!("config/resources cannot be used: {}", e.describe())),
        }
        match root.ensure(&["ign-deploy", &setup.gateway, &setup.mode]) {
            Ok(dir) => {
                let name = format!("probe-{}.tmp", std::process::id());
                let _ = dir.unlink(&[&name]);
                match dir.create_new(&[&name], b"", Visibility::Private) {
                    Ok(()) => {
                        let _ = dir.unlink(&[&name]);
                    }
                    Err(e) => problems.push(format!(
                        "the ign-deploy directory is not writable: {}",
                        e.describe()
                    )),
                }
            }
            Err(e) => problems.push(format!(
                "the ign-deploy directory cannot be created or opened: {}",
                e.describe()
            )),
        }
        Ok(ProbeReport {
            writable: problems.is_empty(),
            problems,
        })
    }

    /// [`probe_blocking`](Self::probe_blocking) for a run that touches
    /// `touched` (quick 261102-cef, P10): with a `.core/` key, `core` must be
    /// a real directory, and for each core tag type touched the deepest
    /// existing folder of `core/ignition/<type>` (the type folder, else
    /// `core/ignition`, else `core`) must be writable, by the same check the
    /// mode collection gets. Without one it is the plain probe.
    pub(super) fn probe_paths_blocking(
        &self,
        touched: &[String],
    ) -> Result<ProbeReport, CoreError> {
        const OP: &str = "probe";
        let mut report = self.probe_blocking()?;
        let types = core_tag_types(touched);
        if types.is_empty() {
            return Ok(report);
        }
        let root = self.data_root(OP)?;
        let mut codes: Vec<String> = Vec::new();
        match root.kind(&["config", "resources", CORE_COLLECTION]) {
            Ok(Kind::Dir) => {
                // The first level that exists decides (quick 261103-ell, F5): a
                // missing one goes up, a directory must be writable and
                // searchable, a symlink or anything else is unsafe and the
                // folder above it is never probed in its place.
                for ty in types {
                    let levels: [&[&str]; 3] = [&["ignition", ty], &["ignition"], &[]];
                    let mut code = None;
                    for parts in levels {
                        let mut full = vec!["config", "resources", CORE_COLLECTION];
                        full.extend_from_slice(parts);
                        match root.kind(&full) {
                            Ok(Kind::Missing) => continue,
                            Ok(Kind::Dir) => {
                                let usable = root.open(&full).is_ok_and(|dir| dir.writable());
                                if !usable {
                                    code = Some(format!("core_tags_unwritable:{ty}"));
                                }
                            }
                            Ok(_) | Err(FsError::Changed) => {
                                code = Some(format!("core_tags_unsafe:{ty}"));
                            }
                            Err(_) => code = Some(format!("core_tags_unwritable:{ty}")),
                        }
                        break;
                    }
                    codes.extend(code);
                }
            }
            Ok(Kind::Missing) => codes.push("core_missing".to_string()),
            Ok(Kind::Symlink) => codes.push("core_symlink".to_string()),
            Ok(_) => codes.push("core_not_dir".to_string()),
            Err(e) => report.problems.push(format!(
                "config/resources/core cannot be used: {}",
                e.describe()
            )),
        }
        report
            .problems
            .extend(codes.iter().filter_map(|code| core_probe_problem(code)));
        report.writable = report.problems.is_empty();
        Ok(report)
    }

    // ---- prepare ---------------------------------------------------------

    pub(super) fn prepare_blocking(
        &self,
        spec: &BatchSpec,
        tree: &RenderedTree,
    ) -> Result<Prepared, CoreError> {
        const OP: &str = "prepare";
        let setup = self.apply_cfg(OP)?;
        if spec.gateway != setup.gateway || spec.mode != setup.mode {
            return Err(self.mirror_fail(
                OP,
                "the spec names a different gateway or mode than this host",
            ));
        }
        if let Some(problem) = spec.problem() {
            return Err(self.mirror_fail(OP, format!("the batch is not valid: {problem}")));
        }
        let work = WorkList::from_spec(spec).map_err(|why| self.mirror_fail(OP, why))?;
        let mut total: u64 = 0;
        for op in &spec.writes {
            let Some(bytes) = rendered_bytes(tree, &op.path) else {
                return Err(
                    self.mirror_fail(OP, format!("{} is not in the rendered tree", op.path))
                );
            };
            if bytes.len() as u64 > READ_MAX_FILE_BYTES || sha256_hex(bytes) != op.sha256 {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "the rendered bytes of {} are over the cap or do not match the spec",
                        op.path
                    ),
                ));
            }
            total = total.saturating_add(bytes.len() as u64);
            if total > READ_MAX_TOTAL_BYTES {
                return Err(self.mirror_fail(OP, "the batch exceeds the total size cap"));
            }
        }

        let lay = Layout::new(setup, spec.run_id.as_str());
        let root = self.data_root(OP)?;
        // A run that touches `core` needs it as it is; refused before
        // anything is staged (quick 261102-cef, P7).
        self.core_for(
            OP,
            &root,
            spec.writes
                .iter()
                .map(|w| w.path.as_str())
                .chain(spec.deletes.iter().map(|d| d.path.as_str())),
        )?;
        root.ensure(&lay.under(&["staging"], &[]))
            .map_err(|e| self.fs_fail(OP, "the staging area", &e))?;
        // A stale staging dir of this run (an earlier attempt) is replaced.
        root.remove_tree(&lay.stage(&[]))
            .map_err(|e| self.fs_fail(OP, "a stale staging directory", &e))?;
        root.ensure(&lay.stage(&[]))
            .map_err(|e| self.fs_fail(OP, "the staging directory", &e))?;

        for op in &spec.writes {
            let Some(bytes) = rendered_bytes(tree, &op.path) else {
                return Err(
                    self.mirror_fail(OP, format!("{} is not in the rendered tree", op.path))
                );
            };
            let parts = in_slot(&lay.stage(&[]), op.path.as_str());
            root.create_new(&parts, bytes, Visibility::Shared)
                .map_err(|e| self.fs_fail(OP, &format!("staging {}", op.path), &e))?;
            // The copy is read back: what is staged is what will move.
            let back = root
                .read(&parts, READ_MAX_FILE_BYTES)
                .map_err(|e| self.fs_fail(OP, &format!("re-reading {}", op.path), &e))?;
            if back.as_deref().map(sha256_hex).as_deref() != Some(op.sha256.as_str()) {
                return Err(self.mirror_fail(
                    OP,
                    format!("the staged copy of {} does not match its hash", op.path),
                ));
            }
        }
        if let Some(ledger) = &spec.ledger_new {
            let parts = lay.stage(&[LEDGER_FILE]);
            root.create_new(&parts, ledger, Visibility::Private)
                .map_err(|e| self.fs_fail(OP, "staging the ledger", &e))?;
        }
        let put = |name: &str, bytes: &[u8]| -> Result<(), CoreError> {
            root.create_new(&lay.stage(&[name]), bytes, Visibility::Private)
                .map_err(|e| self.fs_fail(OP, &format!("staging {name}"), &e))
        };
        for phase in PRERENDERED_PHASES {
            put(&journal_file(phase), &spec.journal_at(phase).to_bytes())?;
        }
        put(&record_file(RecordState::Applying), &spec.record.to_bytes())?;
        let lines = work.to_lines();
        put(WORK_FILE, &lines)?;
        Ok(Prepared::new(
            spec.run_id.clone(),
            lay.staging_rel(),
            sha256_hex(&lines),
        ))
    }

    // ---- commit ----------------------------------------------------------

    /// A staged file's bytes, which must exist.
    fn staged(
        &self,
        op: &str,
        root: &Dir,
        lay: &Layout,
        parts: &[&str],
        cap: u64,
    ) -> Result<Vec<u8>, CoreError> {
        match root.read(parts, cap) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(self.mirror_fail(
                op,
                format!("{} is not staged", shown(&parts[lay.stage(&[]).len()..])),
            )),
            Err(e) => Err(self.fs_fail(
                op,
                &format!(
                    "reading the staged {}",
                    shown(&parts[lay.stage(&[]).len()..])
                ),
                &e,
            )),
        }
    }

    /// Undo a commit that has not moved a file: the journal and the partial
    /// snapshot. Best effort; a leftover is what recovery handles.
    fn abort_before_move(&self, root: &Dir, lay: &Layout) {
        let _ = root.remove_tree(&lay.snapshot(&[]));
        let _ = root.unlink(&lay.journal());
    }

    /// `hang` is set when a [`ApplyFault::HangAfterMove`] stopped the commit
    /// after its k-th move; the async caller then waits where a test can abort.
    pub(super) fn apply_commit_blocking(
        &self,
        prepared: &Prepared,
        hang: &mut bool,
    ) -> Result<BatchReport, CoreError> {
        const OP: &str = "commit";
        // The time budget counts from here, as a script's counts from its start.
        let started = std::time::Instant::now();
        let setup = self.apply_cfg(OP)?;
        let lay = Layout::new(setup, prepared.run_id().as_str());
        if prepared.staging() != lay.staging_rel() {
            return Err(self.mirror_fail(OP, "the prepared batch was staged for a different host"));
        }
        let root = self.data_root(OP)?;

        // ---- read-only: the work list and what is staged
        let lines = self.staged(OP, &root, &lay, &lay.stage(&[WORK_FILE]), WORK_MAX_BYTES)?;
        if sha256_hex(&lines) != prepared.work_sha256() {
            return Err(self.mirror_fail(OP, "the staged work list changed after it was prepared"));
        }
        let work = WorkList::parse_lines(&lines).map_err(|why| self.mirror_fail(OP, why))?;
        if work.op != WorkOp::Commit
            || work.run_id != lay.run
            || work.gateway != lay.gateway
            || work.mode != lay.mode
        {
            return Err(self.mirror_fail(OP, "the staged work list is not this host's commit"));
        }
        let mut touched: Vec<String> = work
            .writes
            .iter()
            .map(|w| w.path.as_str().to_string())
            .chain(work.deletes.iter().map(|d| d.path.as_str().to_string()))
            .collect();
        if work.ledger.is_some() {
            touched.push(LEDGER_KEY.to_string());
        }
        touched.sort();

        let mut journals: BTreeMap<&'static str, Vec<u8>> = BTreeMap::new();
        for phase in PRERENDERED_PHASES {
            let bytes = self.staged(
                OP,
                &root,
                &lay,
                &lay.stage(&[&journal_file(phase)]),
                JOURNAL_MAX_BYTES as u64,
            )?;
            let journal = Journal::parse(&journal_file(phase), &bytes, &lay.gateway, &lay.mode)?;
            if journal.phase != phase || journal.run_id != lay.run || journal.touched != touched {
                return Err(self.mirror_fail(OP, "a staged journal is not this batch's"));
            }
            journals.insert(phase_key(phase), bytes);
        }
        let record_bytes = self.staged(
            OP,
            &root,
            &lay,
            &lay.stage(&[&record_file(RecordState::Applying)]),
            RECORD_MAX_BYTES as u64,
        )?;
        let record = RunRecord::parse("record", &record_bytes, &lay.gateway, &lay.mode)?;
        if record.run_id != lay.run || record.state != RecordState::Applying {
            return Err(self.mirror_fail(OP, "the staged record is not this batch's"));
        }
        for w in &work.writes {
            let parts = in_slot(&lay.stage(&[]), w.path.as_str());
            let bytes = self.staged(OP, &root, &lay, &parts, READ_MAX_FILE_BYTES)?;
            if sha256_hex(&bytes) != w.sha256 {
                return Err(self.mirror_fail(
                    OP,
                    format!(
                        "the staged copy of {} changed after it was prepared",
                        w.path
                    ),
                ));
            }
        }
        if let Some(ledger) = &work.ledger {
            let bytes = self.staged(
                OP,
                &root,
                &lay,
                &lay.stage(&[LEDGER_FILE]),
                LEDGER_MAX_BYTES as u64,
            )?;
            if sha256_hex(&bytes) != ledger.new_sha256 {
                return Err(self.mirror_fail(OP, "the staged ledger changed after it was prepared"));
            }
        }

        // ---- 1. the journal, exclusively; an existing one refuses
        match self.read_journal_blocking(&lay.gateway, &lay.mode)? {
            None => {}
            Some(existing) => return Err(self.journal_present(&lay, &existing)),
        }
        match root.publish_new(
            &lay.journal(),
            &journals[phase_key(JournalPhase::Snapshotting)],
        ) {
            Ok(()) => {}
            Err(FsError::Exists) => {
                let existing = self
                    .read_journal_blocking(&lay.gateway, &lay.mode)?
                    .unwrap_or_default();
                return Err(self.journal_present(&lay, &existing));
            }
            Err(e) => return Err(self.fs_fail(OP, "creating the journal", &e)),
        }
        let mut report = BatchReport {
            phase: JournalPhase::Snapshotting,
            stale: false,
            moved: Vec::new(),
            deleted: Vec::new(),
            removed_dirs: Vec::new(),
            failure: None,
            deadline: false,
        };

        match self.commit_before_first_move(&root, &lay, &work, &journals, &record_bytes) {
            Ok(None) => {}
            Ok(Some(stale)) => {
                self.abort_before_move(&root, &lay);
                report.stale = true;
                report.failure = Some(stale);
                return Ok(report);
            }
            Err(e) => {
                self.abort_before_move(&root, &lay);
                return Err(e);
            }
        }
        report.phase = JournalPhase::Writing;
        let mut control = MoveControl {
            hang: false,
            started,
        };
        let moved = self.commit_moves(&root, &lay, &work, &journals, &mut report, &mut control);
        *hang = control.hang;
        moved?;
        Ok(report)
    }

    fn journal_present(&self, lay: &Layout, existing: &[u8]) -> CoreError {
        let run_id = Journal::parse("journal", existing, &lay.gateway, &lay.mode)
            .map(|j| j.run_id)
            .unwrap_or_else(|_| "unknown".to_string());
        CoreError::DeployJournalPresent {
            run_id,
            env: "<env>".to_string(),
            location: journal_location(LedgerRoot::DataDir, &lay.gateway, &lay.mode),
        }
    }

    /// Steps 2 to 5 up to and including phase `ready`. `Ok(Some(detail))` is a
    /// pre-image mismatch (nothing written beyond the journal and the partial
    /// snapshot); the caller removes those.
    fn commit_before_first_move(
        &self,
        root: &Dir,
        lay: &Layout,
        work: &WorkList,
        journals: &BTreeMap<&'static str, Vec<u8>>,
        record_bytes: &[u8],
    ) -> Result<Option<String>, CoreError> {
        const OP: &str = "commit";
        let colls = Colls {
            mode: self.collection(OP, root, &lay.mode, false)?,
            core: self.core_for(OP, root, work_keys(work))?,
        };

        // ---- 2. every touched path's live hash against its pre-image
        let mut stale: Vec<String> = Vec::new();
        for w in &work.writes {
            if !self
                .live_key(OP, &colls, &w.path)?
                .matches(w.pre.as_deref())
            {
                stale.push(w.path.as_str().to_string());
            }
        }
        for d in &work.deletes {
            if !self.live_key(OP, &colls, &d.path)?.matches(Some(&d.pre)) {
                stale.push(d.path.as_str().to_string());
            }
        }
        if let Some(ledger) = &work.ledger
            && !self
                .live_ledger(OP, root, lay)?
                .matches(ledger.pre_sha256.as_deref())
        {
            stale.push(LEDGER_KEY.to_string());
        }
        if !stale.is_empty() {
            stale.sort();
            let named: Vec<&str> = stale.iter().take(NAMED_MAX).map(String::as_str).collect();
            let more = stale.len().saturating_sub(NAMED_MAX);
            return Ok(Some(format!(
                "{} path(s) are not what the plan was made against: {}{}",
                stale.len(),
                named.join(", "),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            )));
        }

        // ---- 3. copy each existing pre-image and the prior ledger; verify
        root.ensure(&lay.snapshot(&[]))
            .map_err(|e| self.fs_fail(OP, "the snapshot directory", &e))?;
        let preimages = work
            .writes
            .iter()
            .filter_map(|w| w.pre.as_deref().map(|p| (&w.path, p)))
            .chain(work.deletes.iter().map(|d| (&d.path, d.pre.as_str())));
        for (path, pre) in preimages {
            let (collection, live_parts) = key_parts(path.as_str());
            let bytes = match colls
                .get(collection)
                .map(|c| c.read(&live_parts, READ_MAX_FILE_BYTES))
            {
                Some(Ok(Some(bytes))) => bytes,
                Some(Err(e)) => return Err(self.fs_fail(OP, &format!("reading {path}"), &e)),
                _ => {
                    return Ok(Some(format!(
                        "1 path(s) are not what the plan was made against: {path}"
                    )));
                }
            };
            if sha256_hex(&bytes) != pre {
                return Ok(Some(format!(
                    "1 path(s) are not what the plan was made against: {path}"
                )));
            }
            let parts = in_slot(&lay.snapshot(&[]), path.as_str());
            root.create_new(&parts, &bytes, Visibility::Private)
                .map_err(|e| self.fs_fail(OP, &format!("snapshotting {path}"), &e))?;
            let back = root
                .read(&parts, READ_MAX_FILE_BYTES)
                .map_err(|e| self.fs_fail(OP, &format!("verifying the snapshot of {path}"), &e))?;
            if back.as_deref().map(sha256_hex).as_deref() != Some(pre) {
                return Err(
                    self.mirror_fail(OP, format!("the snapshot copy of {path} does not match"))
                );
            }
        }
        if let Some(ledger) = &work.ledger
            && let Some(pre) = &ledger.pre_sha256
        {
            let bytes = match root.read(&lay.ledger(), LEDGER_MAX_BYTES as u64) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => {
                    return Ok(Some(
                        "1 path(s) are not what the plan was made against: ledger.json".to_string(),
                    ));
                }
                Err(e) => return Err(self.fs_fail(OP, "reading the ledger", &e)),
            };
            if &sha256_hex(&bytes) != pre {
                return Ok(Some(
                    "1 path(s) are not what the plan was made against: ledger.json".to_string(),
                ));
            }
            let parts = lay.snapshot(&[LEDGER_FILE]);
            root.create_new(&parts, &bytes, Visibility::Private)
                .map_err(|e| self.fs_fail(OP, "snapshotting the ledger", &e))?;
            let back = root
                .read(&parts, LEDGER_MAX_BYTES as u64)
                .map_err(|e| self.fs_fail(OP, "verifying the ledger snapshot", &e))?;
            if back.as_deref().map(sha256_hex).as_deref() != Some(pre.as_str()) {
                return Err(self.mirror_fail(OP, "the ledger snapshot copy does not match"));
            }
        }

        // ---- 4. the record, at state applying
        root.create_new(
            &lay.snapshot(&[RECORD_FILE]),
            record_bytes,
            Visibility::Private,
        )
        .map_err(|e| self.fs_fail(OP, "writing the run record", &e))?;

        // ---- 5. phase ready
        root.replace(&lay.journal(), &journals[phase_key(JournalPhase::Ready)])
            .map_err(|e| self.fs_fail(OP, "journal phase ready", &e))?;
        Ok(None)
    }

    /// Steps 5 (phase `writing`) to 9. A failure after the first move is
    /// returned in the report, with the journal left at `writing`.
    fn commit_moves(
        &self,
        root: &Dir,
        lay: &Layout,
        work: &WorkList,
        journals: &BTreeMap<&'static str, Vec<u8>>,
        report: &mut BatchReport,
        control: &mut MoveControl,
    ) -> Result<(), CoreError> {
        const OP: &str = "commit";
        let stop = |report: &mut BatchReport, why: String| -> Result<(), CoreError> {
            report.failure = Some(why);
            Ok(())
        };
        // No move or delete starts once the budget is spent (review WR-04).
        let budget = work
            .budget_s
            .map(|seconds| std::time::Duration::from_secs(u64::from(seconds)));
        let out_of_time = || budget.is_some_and(|b| control.started.elapsed() >= b);
        let deadline = |report: &mut BatchReport, what: String| -> Result<(), CoreError> {
            report.deadline = true;
            report.failure = Some(format!(
                "{what}: the commit's time budget ran out before it started"
            ));
            Ok(())
        };
        if let Err(e) = root.replace(&lay.journal(), &journals[phase_key(JournalPhase::Writing)]) {
            // Nothing has moved, so this is still a clean abort.
            self.abort_before_move(root, lay);
            return Err(self.fs_fail(OP, "journal phase writing", &e));
        }
        let coll = match self.collection(OP, root, &lay.mode, true) {
            Ok(Some(c)) => c,
            Ok(None) => {
                return stop(
                    report,
                    "the mode collection could not be created".to_string(),
                );
            }
            Err(e) => return stop(report, e.to_string()),
        };
        // `core` is opened again, never created (quick 261102-cef, P7).
        let core = match self.core_for(OP, root, work_keys(work)) {
            Ok(core) => core,
            Err(e) => return stop(report, e.to_string()),
        };
        let colls = Colls {
            mode: Some(coll),
            core,
        };

        // ---- 6. move each staged file over its target
        for (index, w) in work.writes.iter().enumerate() {
            if out_of_time() {
                return deadline(report, format!("write {index}"));
            }
            let (collection, target) = key_parts(w.path.as_str());
            let Some(coll) = colls.get(collection) else {
                return stop(report, format!("write {index}: its collection is not open"));
            };
            let staged = in_slot(&lay.stage(&[]), w.path.as_str());
            match coll.kind(&target) {
                Ok(Kind::Missing | Kind::File) => {}
                Ok(_) => {
                    return stop(
                        report,
                        format!("write {index}: the target is no longer a regular file"),
                    );
                }
                Err(e) => return stop(report, format!("write {index}: {}", e.describe())),
            }
            match root.read(&staged, READ_MAX_FILE_BYTES) {
                Ok(Some(bytes)) if sha256_hex(&bytes) == w.sha256 => {}
                Ok(_) => return stop(report, format!("write {index}: the staged copy changed")),
                Err(e) => {
                    return stop(
                        report,
                        format!("write {index}: the staged copy: {}", e.describe()),
                    );
                }
            }
            // Never move a file onto itself (C7a-4: that deletes it).
            let staged_id = root.ident(&staged).ok().flatten();
            let target_id = coll.ident(&target).ok().flatten();
            if staged_id.is_some() && staged_id == target_id {
                return stop(
                    report,
                    format!("write {index}: the staged copy is the target itself"),
                );
            }
            let inject = matches!(self.fault, Some(ApplyFault::FailMoveAt(k)) if k == index);
            if let Err(e) = self.move_with_retry(root, &staged, coll, &target, inject) {
                return stop(
                    report,
                    format!(
                        "write {index}: cannot move the file into place: {}",
                        e.describe()
                    ),
                );
            }
            report.moved.push(index);
            if matches!(&self.fault, Some(ApplyFault::HangAfterMove(k, _)) if *k == report.moved.len())
            {
                control.hang = true;
                return Ok(());
            }
            if let Some(ApplyFault::SlowMoves(pause)) = &self.fault {
                std::thread::sleep(*pause);
            }
        }

        // ---- 7. the delete set, then the directories the deletes emptied
        for (index, d) in work.deletes.iter().enumerate() {
            if out_of_time() {
                return deadline(report, format!("delete {index}"));
            }
            let (collection, target) = key_parts(d.path.as_str());
            let Some(coll) = colls.get(collection) else {
                return stop(
                    report,
                    format!("delete {index}: its collection is not open"),
                );
            };
            match coll.kind(&target) {
                Ok(Kind::File) => {}
                Ok(_) => {
                    return stop(
                        report,
                        format!("delete {index}: the target is no longer a regular file"),
                    );
                }
                Err(e) => return stop(report, format!("delete {index}: {}", e.describe())),
            }
            if let Err(e) = coll.unlink(&target) {
                return stop(
                    report,
                    format!("delete {index}: cannot delete the file: {}", e.describe()),
                );
            }
            report.deleted.push(index);
        }
        let deleted: Vec<String> = work
            .deletes
            .iter()
            .map(|d| d.path.as_str().to_string())
            .collect();
        let mut removed = BTreeSet::new();
        let emptied = self.remove_emptied_keys(OP, &colls, &deleted, &mut removed);
        report.removed_dirs = removed.into_iter().collect();
        if let Err(e) = emptied {
            return stop(report, format!("removing an emptied directory failed: {e}"));
        }

        // ---- 8. the ledger, last
        if let Some(ledger) = &work.ledger {
            let bytes = match root.read(&lay.stage(&[LEDGER_FILE]), LEDGER_MAX_BYTES as u64) {
                Ok(Some(bytes)) if sha256_hex(&bytes) == ledger.new_sha256 => bytes,
                _ => return stop(report, "the staged ledger changed".to_string()),
            };
            if let Err(e) = root.replace(&lay.ledger(), &bytes) {
                return stop(report, format!("cannot swap the ledger: {}", e.describe()));
            }
        }

        // ---- 9. phase written
        if let Err(e) = root.replace(&lay.journal(), &journals[phase_key(JournalPhase::Written)]) {
            return stop(
                report,
                format!("cannot write journal phase written: {}", e.describe()),
            );
        }
        report.phase = JournalPhase::Written;
        Ok(())
    }

    /// [`remove_emptied`](Self::remove_emptied) for apply keys: each key in
    /// its own collection. A directory removed in `core` is reported with its
    /// `.core/` key.
    fn remove_emptied_keys(
        &self,
        op: &str,
        colls: &Colls,
        keys: &[String],
        removed: &mut BTreeSet<String>,
    ) -> Result<(), CoreError> {
        for collection in [KeyCollection::Mode, KeyCollection::Core] {
            let mine: Vec<String> = keys
                .iter()
                .filter(|key| split_key(key).0 == collection)
                .cloned()
                .collect();
            if mine.is_empty() {
                continue;
            }
            let Some(coll) = colls.get(collection) else {
                continue;
            };
            self.remove_emptied(op, coll, &mine, removed)?;
        }
        Ok(())
    }

    /// Remove each directory the apply keys `paths` (all of one collection,
    /// the one `coll` is) emptied, deepest first, never `<module>/<type>` or
    /// above, except the emptied type folder of a tag type a pair cannot sync
    /// in the mode collection ([`emptied_dir_floor`]). A removed directory is
    /// named by its key: `.core/`-qualified in `core`.
    fn remove_emptied(
        &self,
        op: &str,
        coll: &Dir,
        paths: &[String],
        removed: &mut BTreeSet<String>,
    ) -> Result<(), CoreError> {
        for path in paths {
            let (collection, parts) = key_parts(path);
            for depth in (emptied_dir_floor(path)..parts.len()).rev() {
                let bare = parts[..depth].join("/");
                let rel = match collection {
                    KeyCollection::Mode => bare,
                    KeyCollection::Core => core_key(&bare),
                };
                if removed.contains(&rel) {
                    continue;
                }
                match coll.kind(&parts[..depth]) {
                    Ok(Kind::Dir) => {}
                    Ok(Kind::Missing) => continue,
                    Ok(_) => {
                        return Err(self.mirror_fail(
                            op,
                            format!("{rel} is not a real directory; stopped before removing it"),
                        ));
                    }
                    Err(e) => return Err(self.fs_fail(op, &rel, &e)),
                }
                match coll.rmdir(&parts[..depth]) {
                    Ok(()) => {
                        removed.insert(rel);
                    }
                    Err(FsError::NotEmpty | FsError::Exists) => break,
                    Err(FsError::Missing) => continue,
                    Err(e) => return Err(self.fs_fail(op, &rel, &e)),
                }
            }
        }
        Ok(())
    }

    // ---- restore ---------------------------------------------------------

    pub(super) fn restore_blocking(&self, record: &RunRecord) -> Result<BatchReport, CoreError> {
        const OP: &str = "restore";
        let setup = self.apply_cfg(OP)?;
        if record.gateway != setup.gateway || record.mode != setup.mode {
            return Err(self.mirror_fail(
                OP,
                "the record names a different gateway or mode than this host",
            ));
        }
        let work = WorkList::for_restore(record).map_err(|why| self.mirror_fail(OP, why))?;
        let lay = Layout::new(setup, &record.run_id);
        let journal = restoring_journal(record);
        let journal_bytes = journal.to_bytes();
        Journal::parse("journal", &journal_bytes, &lay.gateway, &lay.mode)?;
        let root = self.data_root(OP)?;
        match root.kind(&lay.snapshot(&[])) {
            Ok(Kind::Dir) => {}
            Ok(Kind::Missing) => {
                return Err(
                    self.mirror_fail(OP, format!("run {} has no snapshot on this host", lay.run))
                );
            }
            Ok(_) | Err(_) => {
                return Err(self.mirror_fail(OP, "the snapshot directory is not a real directory"));
            }
        }

        // ---- the journal at `restoring`, first; another run's journal refuses
        if let Some(existing) = self.read_journal_blocking(&lay.gateway, &lay.mode)? {
            let theirs = Journal::parse(
                &journal_location(LedgerRoot::DataDir, &lay.gateway, &lay.mode),
                &existing,
                &lay.gateway,
                &lay.mode,
            )?;
            if theirs.run_id != lay.run {
                return Err(self.journal_present(&lay, &existing));
            }
        }
        root.replace(&lay.journal(), &journal_bytes)
            .map_err(|e| self.fs_fail(OP, "journal phase restoring", &e))?;

        let mut report = BatchReport {
            phase: JournalPhase::Restoring,
            stale: false,
            moved: Vec::new(),
            deleted: Vec::new(),
            removed_dirs: Vec::new(),
            failure: None,
            deadline: false,
        };
        let coll = match self.collection(OP, &root, &lay.mode, false) {
            Ok(coll) => coll,
            Err(e) => {
                report.failure = Some(e.to_string());
                return Ok(report);
            }
        };
        // `core` is never created, by a restore either (quick 261102-cef). It
        // is needed only for a `.core/` record with a pre-image to put back; a
        // run that only created `.core/` files has nothing to restore there
        // (quick 261103-ell, F4).
        let keys = work.restores.iter().map(|r| r.path.as_str());
        let core = if work
            .restores
            .iter()
            .any(|r| r.pre.is_some() && split_key(&r.path).0 == KeyCollection::Core)
        {
            self.core_for(OP, &root, keys)
        } else if touches_core(keys) {
            self.core_collection_if_present(OP, &root)
        } else {
            Ok(None)
        };
        let core = match core {
            Ok(core) => core,
            Err(e) => {
                report.failure = Some(e.to_string());
                return Ok(report);
            }
        };
        let mut colls = Colls { mode: coll, core };
        let mut removed = BTreeSet::new();
        let mut created: Vec<String> = Vec::new();

        // Collection files first, the ledger last.
        let order: Vec<usize> = (0..work.restores.len())
            .filter(|i| work.restores[*i].path != LEDGER_KEY)
            .chain((0..work.restores.len()).filter(|i| work.restores[*i].path == LEDGER_KEY))
            .collect();
        for index in order {
            let entry = &work.restores[index];
            let is_ledger = entry.path == LEDGER_KEY;
            let outcome = if matches!(self.fault, Some(ApplyFault::FailRestoreAt(k)) if k == index)
            {
                Err("cannot move the file into place: injected restore fault".to_string())
            } else {
                self.restore_one(
                    &root,
                    &mut colls,
                    &lay,
                    entry.path.as_str(),
                    entry.pre.as_deref(),
                    is_ledger,
                )
            };
            match outcome {
                Ok(Restored::Put) => report.moved.push(index),
                Ok(Restored::Removed) => {
                    report.deleted.push(index);
                    if !is_ledger {
                        created.push(entry.path.clone());
                    }
                }
                Err(why) => {
                    report.failure = Some(format!("restore {index}: {why}"));
                    report.removed_dirs = removed.into_iter().collect();
                    return Ok(report);
                }
            }
        }
        if let Err(e) = self.remove_emptied_keys(OP, &colls, &created, &mut removed) {
            report.failure = Some(format!("removing an emptied directory failed: {e}"));
        }
        report.removed_dirs = removed.into_iter().collect();
        Ok(report)
    }

    /// Put one path back. The error is a value-free reason.
    fn restore_one(
        &self,
        root: &Dir,
        colls: &mut Colls,
        lay: &Layout,
        path: &str,
        pre: Option<&str>,
        is_ledger: bool,
    ) -> Result<Restored, String> {
        RelPath::new(path).map_err(|_| "the path is not valid".to_string())?;
        let (collection, key_live_parts) = key_parts(path);
        let live_parts: Vec<&str> = if is_ledger {
            lay.ledger()
        } else {
            key_live_parts
        };
        let live_dir: &Dir = if is_ledger {
            root
        } else if collection == KeyCollection::Core {
            // `core` was opened for the run and is never created.
            match colls.core.as_ref() {
                Some(c) => c,
                None if pre.is_none() => return Ok(Restored::Removed),
                None => return Err("the core collection does not exist".to_string()),
            }
        } else {
            match colls.mode {
                Some(ref c) => c,
                None if pre.is_none() => return Ok(Restored::Removed),
                None => {
                    // The collection is gone; it is recreated for a pre-image.
                    colls.mode = self
                        .collection("restore", root, &lay.mode, true)
                        .map_err(|e| e.to_string())?;
                    colls
                        .mode
                        .as_ref()
                        .ok_or("the mode collection could not be created")?
                }
            }
        };
        let live = live_dir.kind(&live_parts).map_err(|e| e.describe())?;
        match pre {
            None => match live {
                Kind::Missing => Ok(Restored::Removed),
                Kind::File => {
                    live_dir.unlink(&live_parts).map_err(|e| e.describe())?;
                    Ok(Restored::Removed)
                }
                _ => Err("the live path is not a regular file".to_string()),
            },
            Some(hash) => {
                if !matches!(live, Kind::Missing | Kind::File) {
                    return Err("the live path is not a regular file".to_string());
                }
                let snap_parts: Vec<&str> = if is_ledger {
                    lay.snapshot(&[LEDGER_FILE])
                } else {
                    let parts = in_slot(&lay.snapshot(&[]), path);
                    return self.copy_back(
                        root,
                        live_dir,
                        lay,
                        &parts,
                        &live_parts,
                        hash,
                        path,
                        false,
                    );
                };
                self.copy_back(
                    root,
                    live_dir,
                    lay,
                    &snap_parts,
                    &live_parts,
                    hash,
                    path,
                    true,
                )
            }
        }
    }

    /// Copy the verified snapshot bytes `from` back to `to` in `live_dir`.
    #[allow(clippy::too_many_arguments)]
    fn copy_back(
        &self,
        root: &Dir,
        live_dir: &Dir,
        lay: &Layout,
        from: &[&str],
        to: &[&str],
        hash: &str,
        path: &str,
        is_ledger: bool,
    ) -> Result<Restored, String> {
        let cap = if is_ledger {
            LEDGER_MAX_BYTES as u64
        } else {
            READ_MAX_FILE_BYTES
        };
        let bytes = root
            .read(from, cap)
            .map_err(|e| format!("the snapshot copy cannot be read: {}", e.describe()))?
            .ok_or("the snapshot has no copy of the file")?;
        if sha256_hex(&bytes) != hash {
            return Err("the snapshot copy does not match its recorded hash".to_string());
        }
        // Already back (a second restore): nothing to write.
        if let Ok(Some(live)) = live_dir.read(to, cap)
            && sha256_hex(&live) == hash
        {
            return Ok(Restored::Put);
        }
        if is_ledger {
            live_dir.replace(to, &bytes).map_err(|e| e.describe())?;
        } else {
            let (collection, rest) = key_parts(path);
            let mut staged = lay.stage(&[restore_slot(collection)]);
            staged.extend(rest);
            match root.kind(&staged) {
                Ok(Kind::Missing) => {}
                Ok(Kind::File) => root.unlink(&staged).map_err(|e| e.describe())?,
                _ => return Err("a stale restore copy is not a regular file".to_string()),
            }
            root.create_new(&staged, &bytes, Visibility::Shared)
                .map_err(|e| format!("cannot stage the restore copy: {}", e.describe()))?;
            self.move_with_retry(root, &staged, live_dir, to, false)
                .map_err(|e| format!("cannot move the file into place: {}", e.describe()))?;
        }
        match live_dir.read(to, cap) {
            Ok(Some(live)) if sha256_hex(&live) == hash => Ok(Restored::Put),
            _ => Err("the restored file does not match its recorded hash".to_string()),
        }
    }

    // ---- fingerprint -----------------------------------------------------

    pub(super) fn fingerprint_blocking(
        &self,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<String>>, CoreError> {
        const OP: &str = "fingerprint";
        let setup = self.apply_cfg(OP)?;
        let lay = Layout::new(setup, "-");
        let root = self.data_root(OP)?;
        let mode = self.collection(OP, &root, &lay.mode, false)?;
        // An absent `core` holds no file; a symlinked one is refused.
        let core = if touches_core(paths.iter().map(String::as_str)) && !core_is_absent(&root) {
            Some(self.core_collection(OP, &root)?)
        } else {
            None
        };
        let colls = Colls { mode, core };
        let mut out = BTreeMap::new();
        for path in paths {
            let hash = if path == LEDGER_KEY {
                match self.live_ledger(OP, &root, &lay)? {
                    Live::Absent => None,
                    Live::Hash(h) => Some(h),
                    Live::Unsafe => {
                        return Err(self.mirror_fail(OP, "the ledger is not a regular file"));
                    }
                }
            } else {
                let rel = RelPath::new(path).map_err(|why| self.mirror_fail(OP, why))?;
                if let Some(why) = apply_key_problem(&rel) {
                    return Err(self.mirror_fail(OP, why));
                }
                match self.live_key(OP, &colls, &rel)? {
                    Live::Absent => None,
                    Live::Hash(h) => Some(h),
                    Live::Unsafe => {
                        return Err(self.mirror_fail(OP, format!("{rel} is not a regular file")));
                    }
                }
            };
            out.insert(path.clone(), hash);
        }
        Ok(out)
    }

    // ---- finalize, read_record, discard ----------------------------------

    pub(super) fn finalize_blocking(&self, record: &RunRecord) -> Result<(), CoreError> {
        const OP: &str = "finalize";
        let setup = self.apply_cfg(OP)?;
        if record.gateway != setup.gateway || record.mode != setup.mode {
            return Err(self.mirror_fail(
                OP,
                "the record names a different gateway or mode than this host",
            ));
        }
        RunId::parse(&record.run_id)?;
        RunRecord::parse(
            "record",
            &record.to_bytes(),
            &lay_gateway(setup),
            &lay_mode(setup),
        )?;
        let lay = Layout::new(setup, &record.run_id);
        if let Some(existing) = self.read_journal_blocking(&lay.gateway, &lay.mode)? {
            let theirs = Journal::parse(
                &journal_location(LedgerRoot::DataDir, &lay.gateway, &lay.mode),
                &existing,
                &lay.gateway,
                &lay.mode,
            )?;
            if theirs.run_id != lay.run {
                return Err(self.journal_present(&lay, &existing));
            }
        }
        let root = self.data_root(OP)?;
        root.replace(&lay.snapshot(&[RECORD_FILE]), &record.to_bytes())
            .map_err(|e| self.fs_fail(OP, "installing the run record", &e))?;
        match root.unlink(&lay.journal()) {
            Ok(()) | Err(FsError::Missing) => {}
            Err(e) => return Err(self.fs_fail(OP, "removing the journal", &e)),
        }
        root.remove_tree(&lay.stage(&[]))
            .map_err(|e| self.fs_fail(OP, "removing the staging directory", &e))
    }

    pub(super) fn read_record_blocking(
        &self,
        run_id: &RunId,
    ) -> Result<Option<RunRecord>, CoreError> {
        const OP: &str = "read_record";
        let setup = self.apply_cfg(OP)?;
        let lay = Layout::new(setup, run_id.as_str());
        let location = location_of(
            LedgerRoot::DataDir,
            &record_rel(&lay.gateway, &lay.mode, &lay.run),
        );
        let invalid = |problem: String| CoreError::DeployRunInvalid {
            run_id: lay.run.clone(),
            problem: format!("{location}: {problem}"),
        };
        let root = Dir::root(&self.data_dir).map_err(|e| self.fs_fail(OP, "data_dir", &e))?;
        let bytes = match root.read(&lay.snapshot(&[RECORD_FILE]), RECORD_MAX_BYTES as u64) {
            Ok(None) => return Ok(None),
            Ok(Some(bytes)) => bytes,
            Err(e) => return Err(invalid(e.describe())),
        };
        let record = RunRecord::parse(&location, &bytes, &lay.gateway, &lay.mode)?;
        if record.run_id != lay.run {
            return Err(invalid("the record names a different run".to_string()));
        }
        Ok(Some(record))
    }

    pub(super) fn discard_blocking(&self, run_id: &RunId) -> Result<(), CoreError> {
        const OP: &str = "discard";
        let setup = self.apply_cfg(OP)?;
        let lay = Layout::new(setup, run_id.as_str());
        let root = self.data_root(OP)?;
        let mut has_journal = false;
        if let Some(existing) = self.read_journal_blocking(&lay.gateway, &lay.mode)? {
            let journal = Journal::parse(
                &journal_location(LedgerRoot::DataDir, &lay.gateway, &lay.mode),
                &existing,
                &lay.gateway,
                &lay.mode,
            )?;
            if journal.run_id != lay.run {
                return Err(self.journal_present(&lay, &existing));
            }
            if !journal.phase.before_first_move() {
                return Err(CoreError::DeployRunInvalid {
                    run_id: lay.run.clone(),
                    problem: "the run may have moved files; restore it instead of discarding it"
                        .to_string(),
                });
            }
            has_journal = true;
        }
        root.remove_tree(&lay.snapshot(&[]))
            .map_err(|e| self.fs_fail(OP, "removing the snapshot", &e))?;
        root.remove_tree(&lay.stage(&[]))
            .map_err(|e| self.fs_fail(OP, "removing the staging directory", &e))?;
        if has_journal {
            match root.unlink(&lay.journal()) {
                Ok(()) | Err(FsError::Missing) => {}
                Err(e) => return Err(self.fs_fail(OP, "removing the journal", &e)),
            }
        }
        Ok(())
    }

    // ---- fetch_snapshot --------------------------------------------------

    pub(super) fn fetch_snapshot_blocking(
        &self,
        run_id: &RunId,
        dest: &Path,
    ) -> Result<BTreeMap<String, String>, CoreError> {
        const OP: &str = "fetch_snapshot";
        let setup = self.apply_cfg(OP)?;
        let lay = Layout::new(setup, run_id.as_str());
        let root = self.data_root(OP)?;
        let snapshot = root.open(&lay.snapshot(&[])).map_err(|e| match e {
            FsError::Missing => {
                self.mirror_fail(OP, format!("run {} has no snapshot on this host", lay.run))
            }
            other => self.fs_fail(OP, "the snapshot directory", &other),
        })?;
        let out_dir = dest.join("snapshot");
        std::fs::create_dir(&out_dir).map_err(|e| {
            self.mirror_fail(
                OP,
                format!("cannot create the snapshot copy directory ({})", e.kind()),
            )
        })?;
        let mut hashes = BTreeMap::new();
        let mut budget = (0usize, 0u64);
        self.copy_snapshot_dir(OP, &snapshot, &[], &out_dir, "", &mut hashes, &mut budget)?;
        Ok(hashes)
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_snapshot_dir(
        &self,
        op: &str,
        dir: &Dir,
        parts: &[&str],
        out: &Path,
        prefix: &str,
        hashes: &mut BTreeMap<String, String>,
        budget: &mut (usize, u64),
    ) -> Result<(), CoreError> {
        for (name, kind) in dir
            .list(parts)
            .map_err(|e| self.fs_fail(op, "listing the snapshot", &e))?
        {
            if let Some(why) = component_problem(&name) {
                return Err(self.mirror_fail(op, format!("a snapshot entry name {why}")));
            }
            budget.0 += 1;
            if budget.0 > READ_MAX_ENTRIES {
                return Err(self.mirror_fail(op, "the snapshot has too many entries"));
            }
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let mut child: Vec<&str> = parts.to_vec();
            child.push(&name);
            match kind {
                Kind::Dir => {
                    let target = out.join(&name);
                    std::fs::create_dir(&target).map_err(|e| {
                        self.mirror_fail(op, format!("cannot create {rel} ({})", e.kind()))
                    })?;
                    self.copy_snapshot_dir(op, dir, &child, &target, &rel, hashes, budget)?;
                }
                Kind::File => {
                    let bytes = dir
                        .read(&child, READ_MAX_FILE_BYTES)
                        .map_err(|e| self.fs_fail(op, &format!("reading {rel}"), &e))?
                        .ok_or_else(|| {
                            self.mirror_fail(op, format!("{rel} vanished while it was copied"))
                        })?;
                    budget.1 = budget.1.saturating_add(bytes.len() as u64);
                    if budget.1 > READ_MAX_TOTAL_BYTES {
                        return Err(self.mirror_fail(op, "the snapshot exceeds the total size cap"));
                    }
                    write_private(&out.join(&name), &bytes).map_err(|e| {
                        self.mirror_fail(op, format!("cannot write {rel} ({})", e.kind()))
                    })?;
                    hashes.insert(rel, format!("{:x}", Sha256::digest(&bytes)));
                }
                _ => {
                    return Err(self.mirror_fail(
                        op,
                        format!("{rel} in the snapshot is not a regular file or directory"),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// The rendered bytes of the apply key `key`: the render is unrouted, so a
/// `.core/` key reads its collection path.
fn rendered_bytes<'a>(tree: &'a RenderedTree, key: &RelPath) -> Option<&'a Vec<u8>> {
    let path = RelPath::new(split_key(key.as_str()).1).ok()?;
    tree.files.get(&path)
}

/// Every key a commit work list writes or deletes.
fn work_keys(work: &WorkList) -> impl Iterator<Item = &str> {
    work.writes
        .iter()
        .map(|w| w.path.as_str())
        .chain(work.deletes.iter().map(|d| d.path.as_str()))
}

/// Whether `config/resources/core` is plainly absent (not a symlink or
/// another refused shape).
fn core_is_absent(root: &Dir) -> bool {
    matches!(
        root.kind(&["config", "resources", CORE_COLLECTION]),
        Ok(Kind::Missing)
    )
}

/// What `restore_one` did.
enum Restored {
    Put,
    Removed,
}

fn phase_key(phase: JournalPhase) -> &'static str {
    match phase {
        JournalPhase::Snapshotting => "snapshotting",
        JournalPhase::Ready => "ready",
        JournalPhase::Writing => "writing",
        JournalPhase::Written => "written",
        JournalPhase::Restoring => "restoring",
    }
}

fn lay_gateway(setup: &ApplySetup) -> String {
    setup.gateway.clone()
}

fn lay_mode(setup: &ApplySetup) -> String {
    setup.mode.clone()
}

/// Create `path` (which must not exist) owner-only and write `bytes`.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}
