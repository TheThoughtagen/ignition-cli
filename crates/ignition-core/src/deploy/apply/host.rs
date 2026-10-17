//! The one write seam the apply engine uses (Phase 23, D-01).
//!
//! [`ApplyHost`] is everything the engine asks of a gateway host while it
//! applies and rolls back: one call that does all of the work inside the scan
//! lock (`commit`), one that undoes it (`restore`), and the read-only and
//! housekeeping calls around them. Nothing in the lock window costs a
//! per-file network round trip: `prepare` stages every file before the lock,
//! `commit` is then a single host call that works from the staged copies.
//!
//! Three hosts implement it: the local filesystem (`LocalFsTransport`, native
//! calls), and a POSIX shell script and a PowerShell script over ssh (23-06,
//! 23-20). All three are held to one scenario table
//! (`tests/deploy_host_conformance.rs`).
//!
//! # What `commit` does, and in what order
//!
//! Nowhere else and in no other order (D-01, D-04):
//!
//! 1. create `journal.json` exclusively at phase `snapshotting` (an existing
//!    journal refuses, nothing written);
//! 2. check every touched path's live hash against its expected pre-image
//!    (a mismatch aborts, nothing moved: the journal and the partial snapshot
//!    are removed and the report is `stale`);
//! 3. copy each existing pre-image and the prior ledger into
//!    `snapshots/<run_id>/` and verify the copies;
//! 4. write `record.json` at state `applying`;
//! 5. journal phase `ready`, then phase `writing`;
//! 6. move each staged file over its target;
//! 7. remove the delete set and the directories the deletes emptied (never
//!    `<module>/<type>` or above);
//! 8. swap the ledger last;
//! 9. journal phase `written`.
//!
//! # What `restore` does
//!
//! Writes the journal at phase `restoring` first, copies (never moves) each
//! pre-image back from the snapshot and verifies its hash, deletes each
//! touched file whose pre-image is absent, restores or removes the ledger,
//! and leaves the snapshot in place, so a second `restore` gives the same
//! tree.
//!
//! Every path a host writes or deletes is checked with
//! `mirror_path_problem` before any I/O, and no host follows a symlink.

use std::collections::BTreeMap;
use std::path::Path;

use super::fingerprint::LEDGER_KEY;
use super::journal::{Journal, JournalPhase};
use super::record::{RecordState, RunRecord};
use super::run_store::RunId;
use crate::deploy::layout::RelPath;
use crate::deploy::ledger::LEDGER_MAX_BYTES;
use crate::deploy::transport::{mirror_mode_problem, mirror_path_problem};
use crate::deploy::tree::{RenderedTree, sha256_hex};
use crate::error::CoreError;

/// One file the run writes: a new file or a replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOp {
    /// The collection path.
    pub path: RelPath,
    /// SHA-256 of the staged bytes, lowercase hex.
    pub sha256: String,
    /// The live hash the plan was made against; `None` when the file must be
    /// absent.
    pub pre: Option<String>,
}

/// One file the run deletes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteOp {
    /// The collection path.
    pub path: RelPath,
    /// The live hash the plan was made against.
    pub pre: String,
}

/// Everything one `prepare` and `commit` need, decided by the engine.
///
/// `writes` and `deletes` are byte-sorted and unique and disjoint. The
/// journal and record are the run's documents as the engine wants them; the
/// host sets the journal `phase` itself at each step.
#[derive(Debug, Clone)]
pub struct BatchSpec {
    /// The run.
    pub run_id: RunId,
    /// The target gateway directory name.
    pub gateway: String,
    /// The target mode collection.
    pub mode: String,
    /// The files to write.
    pub writes: Vec<WriteOp>,
    /// The files to delete.
    pub deletes: Vec<DeleteOp>,
    /// The new ledger bytes (already validated with `Ledger::parse`), swapped
    /// last. `None` leaves the ledger alone.
    pub ledger_new: Option<Vec<u8>>,
    /// The live ledger hash the plan was made against; `None` when there is no
    /// ledger yet.
    pub ledger_pre: Option<String>,
    /// The journal of this run.
    pub journal: Journal,
    /// The run record at state `applying`.
    pub record: RunRecord,
}

impl BatchSpec {
    /// The paths this run touches, byte-sorted: every write, every delete and
    /// [`LEDGER_KEY`] when the ledger is swapped.
    pub fn touched_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .writes
            .iter()
            .map(|w| w.path.as_str().to_string())
            .chain(self.deletes.iter().map(|d| d.path.as_str().to_string()))
            .collect();
        if self.ledger_new.is_some() {
            paths.push(LEDGER_KEY.to_string());
        }
        paths.sort();
        paths
    }

    /// The first thing wrong with this spec, or `None`. A host calls this
    /// before any I/O; the engine builds a valid spec, so a problem here is a
    /// defect, reported value-free.
    pub fn problem(&self) -> Option<String> {
        self.check().err()
    }

    fn check(&self) -> Result<(), String> {
        if let Some(why) = crate::deploy::component_problem(&self.gateway) {
            return Err(format!("the gateway {why}"));
        }
        if let Some(why) = mirror_mode_problem(&self.mode) {
            return Err(why);
        }
        let run = self.run_id.as_str();
        if self.journal.run_id != run || self.record.run_id != run {
            return Err("the journal or record names a different run".to_string());
        }
        for (what, gateway, mode) in [
            ("journal", &self.journal.gateway, &self.journal.mode),
            ("record", &self.record.gateway, &self.record.mode),
        ] {
            if *gateway != self.gateway || *mode != self.mode {
                return Err(format!("the {what} names a different gateway or mode"));
            }
        }
        if self.record.state != RecordState::Applying {
            return Err("the record is not at state applying".to_string());
        }
        sorted_unique(self.writes.iter().map(|w| w.path.as_str()), "writes")?;
        sorted_unique(self.deletes.iter().map(|d| d.path.as_str()), "deletes")?;
        for op in &self.writes {
            if let Some(why) = mirror_path_problem(&op.path) {
                return Err(why);
            }
            if !is_hex64(&op.sha256) || op.pre.as_deref().is_some_and(|p| !is_hex64(p)) {
                return Err(format!(
                    "a hash of {} is not 64 lowercase hex digits",
                    op.path
                ));
            }
        }
        for op in &self.deletes {
            if let Some(why) = mirror_path_problem(&op.path) {
                return Err(why);
            }
            if !is_hex64(&op.pre) {
                return Err(format!("the pre-image hash of {} is not hex", op.path));
            }
            if self.writes.iter().any(|w| w.path == op.path) {
                return Err(format!("{} is both written and deleted", op.path));
            }
        }
        match (&self.ledger_new, &self.ledger_pre) {
            (None, Some(_)) => {
                return Err("a prior ledger hash without a new ledger".to_string());
            }
            (Some(bytes), _) if bytes.is_empty() || bytes.len() > LEDGER_MAX_BYTES => {
                return Err("the new ledger is empty or over the size cap".to_string());
            }
            _ => {}
        }
        if self.ledger_pre.as_deref().is_some_and(|p| !is_hex64(p)) {
            return Err("the prior ledger hash is not hex".to_string());
        }

        // The documents describe exactly this batch.
        let touched = self.touched_paths();
        if self.journal.touched != touched {
            return Err("the journal's touched set is not this batch's".to_string());
        }
        let record_paths: Vec<&str> = self
            .record
            .touched
            .iter()
            .map(|e| e.path.as_str())
            .collect();
        if record_paths != touched.iter().map(String::as_str).collect::<Vec<_>>() {
            return Err("the record's touched set is not this batch's".to_string());
        }
        for entry in &self.record.touched {
            let (pre, post): (Option<&str>, Option<String>) = if entry.path == LEDGER_KEY {
                (
                    self.ledger_pre.as_deref(),
                    self.ledger_new.as_deref().map(sha256_hex),
                )
            } else if let Some(w) = self.writes.iter().find(|w| w.path.as_str() == entry.path) {
                (w.pre.as_deref(), Some(w.sha256.clone()))
            } else if let Some(d) = self.deletes.iter().find(|d| d.path.as_str() == entry.path) {
                (Some(d.pre.as_str()), None)
            } else {
                return Err("the record names a path outside the batch".to_string());
            };
            if entry.pre_sha256.as_deref() != pre || entry.post_sha256 != post {
                return Err(format!(
                    "the record's hashes for {} are not this batch's",
                    super::journal::shown(&entry.path)
                ));
            }
        }

        // What gets installed must read back through the strict parsers.
        for phase in PRERENDERED_PHASES {
            let journal = self.journal_at(phase);
            Journal::parse("journal", &journal.to_bytes(), &self.gateway, &self.mode)
                .map_err(|e| e.to_string())?;
        }
        RunRecord::parse("record", &self.record.to_bytes(), &self.gateway, &self.mode)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The journal of this run at `phase`.
    pub fn journal_at(&self, phase: JournalPhase) -> Journal {
        Journal {
            phase,
            ..self.journal.clone()
        }
    }
}

/// The journal phases `commit` installs, in order.
pub const PRERENDERED_PHASES: [JournalPhase; 4] = [
    JournalPhase::Snapshotting,
    JournalPhase::Ready,
    JournalPhase::Writing,
    JournalPhase::Written,
];

/// The journal a `restore` of `record` installs first: phase `restoring`,
/// the record's touched set and pre-run fingerprint.
pub fn restoring_journal(record: &RunRecord) -> Journal {
    Journal {
        schema_version: record.schema_version,
        run_id: record.run_id.clone(),
        gateway: record.gateway.clone(),
        mode: record.mode.clone(),
        phase: JournalPhase::Restoring,
        pre_fingerprint: record.pre_fingerprint.clone(),
        touched: record.touched.iter().map(|e| e.path.clone()).collect(),
    }
}

fn is_hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn sorted_unique<'a>(paths: impl Iterator<Item = &'a str>, what: &str) -> Result<(), String> {
    let mut previous: Option<&str> = None;
    for path in paths {
        if previous.is_some_and(|p| p.as_bytes() >= path.as_bytes()) {
            return Err(format!("{what} are not byte-sorted and unique"));
        }
        previous = Some(path);
    }
    Ok(())
}

/// What `prepare` staged. Opaque to the engine: it hands the value back to
/// `commit` unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    run_id: RunId,
    staging: String,
    work_sha256: String,
}

impl Prepared {
    /// A prepared batch of `run_id` staged at `staging` with the work list
    /// hashing to `work_sha256`. Called by a host.
    pub fn new(run_id: RunId, staging: String, work_sha256: String) -> Self {
        Prepared {
            run_id,
            staging,
            work_sha256,
        }
    }

    /// The run.
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Where the batch is staged, as the host spells it.
    pub fn staging(&self) -> &str {
        &self.staging
    }

    /// SHA-256 of the staged work list, lowercase hex.
    pub fn work_sha256(&self) -> &str {
        &self.work_sha256
    }
}

/// What `commit` or `restore` did. Indices point into the spec's `writes`
/// and `deletes` for `commit`, and into the record's `touched` for `restore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReport {
    /// The last journal phase the call installed.
    pub phase: JournalPhase,
    /// The pre-image check found a live file that is not what the plan was
    /// made against. Nothing was moved; `failure` says how many paths and
    /// which (value-free).
    pub stale: bool,
    /// Writes (`commit`) or pre-images put back (`restore`) that completed.
    pub moved: Vec<usize>,
    /// Deletes (`commit`) or created files removed (`restore`) that
    /// completed.
    pub deleted: Vec<usize>,
    /// Directories removed because a delete emptied them.
    pub removed_dirs: Vec<String>,
    /// Why the call stopped early, naming the entry by index and never its
    /// content; `None` when it ran to the end.
    pub failure: Option<String>,
}

/// What `probe` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeReport {
    /// The mode collection and the `ign-deploy` root can be written.
    pub writable: bool,
    /// What is wrong, value-free; empty when `writable`.
    pub problems: Vec<String>,
}

/// The write seam of a gateway host. See the module doc for the order of
/// `commit` and `restore`.
#[async_trait::async_trait]
pub trait ApplyHost: Send + Sync {
    /// Whether the mode collection and the `ign-deploy` root are writable.
    /// Outside the lock. Creates and removes one probe file in the
    /// `ign-deploy` root and never writes into the collection.
    async fn probe(&self) -> Result<ProbeReport, CoreError>;

    /// [`probe`](Self::probe) for a run that will touch `touched` (collection
    /// paths and [`LEDGER_KEY`]). A host with limits of its own on what a path
    /// may be reports them here, before anything is written: the PowerShell
    /// host refuses a path whose snapshot or staging copy under the account
    /// home the sftp server could not reach again (23-08, C7b). The default
    /// has no such limit and answers [`probe`](Self::probe).
    async fn probe_paths(&self, touched: &[String]) -> Result<ProbeReport, CoreError> {
        let _ = touched;
        self.probe().await
    }

    /// Stage the batch beside its work list: every file of `spec.writes` from
    /// `tree` (re-hashed after the copy), the new ledger, the work list and
    /// the pre-rendered journal and record files. Outside the lock. Writes
    /// nothing into the collection and nothing outside this run's staging
    /// directory.
    async fn prepare(&self, spec: &BatchSpec, tree: &RenderedTree) -> Result<Prepared, CoreError>;

    /// Journal, check, snapshot, move, delete, swap the ledger: exactly one
    /// host call, inside the lock. A refusal that wrote nothing is an `Err`;
    /// a pre-image mismatch is `Ok` with `stale`; a failure after the first
    /// move is `Ok` with `failure` naming the entry.
    async fn commit(&self, prepared: &Prepared) -> Result<BatchReport, CoreError>;

    /// Undo `record`'s run from its snapshot: one call. Callable any time the
    /// snapshot exists, in or out of the lock. Idempotent.
    async fn restore(&self, record: &RunRecord) -> Result<BatchReport, CoreError>;

    /// The live hash of each of `paths` (collection paths or
    /// [`LEDGER_KEY`]), `None` when absent. Read-only; a path that is not a
    /// regular file is an error, never `None`.
    async fn fingerprint(
        &self,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<String>>, CoreError>;

    /// Install `record` as the run's final record, then remove the journal
    /// and this run's staging. The snapshot stays. Refuses a journal that
    /// belongs to another run.
    async fn finalize(&self, record: &RunRecord) -> Result<(), CoreError>;

    /// The run record of `run_id`; `None` when the run has no snapshot
    /// directory or no record. Fails closed on an unreadable or invalid one.
    async fn read_record(&self, run_id: &RunId) -> Result<Option<RunRecord>, CoreError>;

    /// Copy `snapshots/<run_id>/` into `<dest>/snapshot/` on the runner,
    /// files private (0600 on unix), and return each copied file's SHA-256
    /// keyed by its path below the snapshot directory.
    async fn fetch_snapshot(
        &self,
        run_id: &RunId,
        dest: &Path,
    ) -> Result<BTreeMap<String, String>, CoreError>;

    /// Drop a run that never moved a file: remove `journal.json`,
    /// `snapshots/<run_id>` and `staging/<run_id>`. Never touches the
    /// collection or the ledger.
    async fn discard_interrupted(&self, run_id: &RunId) -> Result<(), CoreError>;
}
