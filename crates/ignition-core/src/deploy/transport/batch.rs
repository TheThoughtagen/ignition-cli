//! The work list a host executes (Phase 23, research Pattern 1).
//!
//! `work.tsv` is a versioned, line-oriented file. The engine builds it in
//! Rust and stages it beside the files it names; a host script reads it with
//! plain shell tools and never parses JSON. The journal and record bytes a
//! host installs are pre-rendered by Rust and staged beside it
//! (`journal.<phase>.json`, `record.<state>.json`), so a script copies bytes
//! and never formats them.
//!
//! ```text
//! ign-work 1
//! R<TAB><run id>
//! G<TAB><gateway>
//! M<TAB><mode>
//! O<TAB>commit | restore | mirror | probe | fingerprint | finalize | discard | unstage
//! T<TAB><retries>                                                 (retries of a held move)
//! S<TAB><absolute dir>                                            (mirror: where the staged files are)
//! J<TAB><sha256 or ->                                             (restore, finalize, discard: the journal seen)
//! F<TAB><file name><TAB><sha256>                                  (a pre-rendered file in staging)
//! W<TAB><index><TAB><sha256><TAB><pre sha256 or -><TAB><path>     (commit, mirror)
//! D<TAB><index><TAB><pre sha256><TAB><path>                       (commit)
//! X<TAB><index><TAB><path>                                        (mirror: a file to delete)
//! L<TAB><new ledger sha256 or -><TAB><prior ledger sha256 or ->   (commit)
//! P<TAB><index><TAB><pre sha256 or -><TAB><path>                  (restore, fingerprint)
//! ```
//!
//! `R` is `-` for the operations that belong to no run (`probe`,
//! `fingerprint`, `mirror`) and `G` is `-` for `mirror`, which touches no
//! gateway directory. A `mirror` list's `W` records carry `-` as the
//! expected live hash, which the operation ignores (a mirror writes over
//! whatever regular file is there).
//!
//! Fields are TAB-separated. A path (or any text) containing TAB, CR, LF or
//! any control character is refused when the list is built and when it is
//! parsed. A host reports each entry's result by its index. A `P` record's
//! path is a collection path or `ledger.json`.

use std::collections::BTreeSet;

use super::mirror_path_problem;
use crate::deploy::apply::fingerprint::LEDGER_KEY;
use crate::deploy::apply::host::{BatchSpec, PRERENDERED_PHASES, restoring_journal};
use crate::deploy::apply::journal::JournalPhase;
use crate::deploy::apply::record::{RecordState, RunRecord};
use crate::deploy::apply::run_store::RunId;
use crate::deploy::layout::RelPath;

/// File name of the work list inside a run's staging directory.
pub const WORK_FILE: &str = "work.tsv";

/// A move refused with `Failure` is retried this many times before the call
/// fails (C7a-4: a scanner holding the target makes the same rename succeed a
/// moment later).
pub const RENAME_RETRIES: u32 = 3;

/// The only version this build writes and reads.
const HEADER: &str = "ign-work 1";

/// The POSIX `sh` script a Linux ssh host runs: every operation of the batch
/// in one file, uploaded beside the staged files (Phase 23, 23-06).
pub const SCRIPT_SH: &str = include_str!("scripts/commit.sh");

/// File name of the uploaded script inside a staging directory.
pub const SCRIPT_SH_FILE: &str = "commit.sh";

/// The Windows PowerShell 5.1 script a Windows ssh host runs: the `commit`
/// and `restore` operations of the batch (Phase 23, 23-08; 23-20 adds the
/// rest), uploaded beside the staged files and run by
/// [`script_stub`](super::winps::script_stub).
pub const SCRIPT_PS1: &str = include_str!("scripts/commit.ps1");

/// File name of the uploaded PowerShell script inside a staging directory.
pub const SCRIPT_PS1_FILE: &str = "commit.ps1";

/// SHA-256 of [`SCRIPT_PS1`] as the Windows stub computes it: over the bytes
/// of the uploaded file.
pub fn script_ps1_sha256() -> String {
    crate::deploy::tree::sha256_hex(SCRIPT_PS1.as_bytes())
}

/// The fixed text the remote shell runs with `sh -c`. Its arguments are the
/// script path, the operation, the staging, collection and deploy roots, and
/// the SHA-256 of the script and of the work list. It reads the script into
/// memory, recomputes both hashes (`sha256sum`, else `shasum -a 256`) over
/// what it holds and over the work list, exits 97 before running anything
/// when either differs, and otherwise runs the script it hashed. It is one
/// line without a control character, because `RemoteCommand::posix` refuses
/// control characters in any argument. No path or reason is ever part of it.
pub const POSIX_STUB: &str = concat!(
    "set -eu; S=$1; O=$2; T=$3; C=$4; D=$5; SS=$6; WS=$7; ",
    "if command -v sha256sum >/dev/null 2>&1; then H=sha256sum; ",
    "elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256'; ",
    "else echo 'ign: the host has no sha256 tool' >&2; exit 98; fi; ",
    "code=$(cat \"$S\") || exit 97; ",
    "a=$(printf '%s\\n' \"$code\" | $H) || exit 97; ",
    "b=$($H <\"$T/work.tsv\") || exit 97; ",
    "[ \"${a%% *}\" = \"$SS\" ] && [ \"${b%% *}\" = \"$WS\" ] || ",
    "{ echo 'ign: the staged script or work list does not match its hash' >&2; exit 97; }; ",
    "exec sh -c \"$code\" ign-commit \"$O\" \"$T\" \"$C\" \"$D\""
);

/// SHA-256 of [`SCRIPT_SH`] as the stub computes it: the script text with
/// exactly one trailing newline.
pub fn script_sh_sha256() -> String {
    crate::deploy::tree::sha256_hex(SCRIPT_SH.as_bytes())
}

/// What the work list asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkOp {
    /// Journal, check, snapshot, move, delete, swap the ledger.
    Commit,
    /// Put the snapshot back.
    Restore,
    /// Move staged files into a collection and delete files, with no journal,
    /// snapshot or record (the Phase 22 `TreeMirror::commit`).
    Mirror,
    /// Whether the collection and the `ign-deploy` root are writable.
    Probe,
    /// The live hash of each `P` path, read-only.
    Fingerprint,
    /// Install the final record, remove the journal and the run's staging.
    Finalize,
    /// Drop a run that never moved a file.
    Discard,
    /// Remove a run's staging directory.
    Unstage,
}

impl WorkOp {
    /// The word the `O` record carries.
    pub fn word(self) -> &'static str {
        match self {
            WorkOp::Commit => "commit",
            WorkOp::Restore => "restore",
            WorkOp::Mirror => "mirror",
            WorkOp::Probe => "probe",
            WorkOp::Fingerprint => "fingerprint",
            WorkOp::Finalize => "finalize",
            WorkOp::Discard => "discard",
            WorkOp::Unstage => "unstage",
        }
    }

    fn from_word(word: &str) -> Option<WorkOp> {
        [
            WorkOp::Commit,
            WorkOp::Restore,
            WorkOp::Mirror,
            WorkOp::Probe,
            WorkOp::Fingerprint,
            WorkOp::Finalize,
            WorkOp::Discard,
            WorkOp::Unstage,
        ]
        .into_iter()
        .find(|op| op.word() == word)
    }

    /// The operations that belong to no run: their `R` is `-`.
    fn is_runless(self) -> bool {
        matches!(self, WorkOp::Probe | WorkOp::Fingerprint | WorkOp::Mirror)
    }

    /// The operations that touch no gateway directory either: `G` is `-`.
    fn is_gatewayless(self) -> bool {
        self == WorkOp::Mirror
    }
}

/// A file the batch expects in staging next to the work list, with its hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDoc {
    /// A pre-rendered file name ([`journal_file`] or [`record_file`]).
    pub name: String,
    /// Its SHA-256.
    pub sha256: String,
}

/// The journal the engine saw before it asked for the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeenJournal {
    /// There was no `journal.json`.
    Absent,
    /// There was one with these bytes (SHA-256).
    Hash(String),
}

/// A file to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkWrite {
    /// Collection path.
    pub path: RelPath,
    /// SHA-256 of the staged copy.
    pub sha256: String,
    /// The expected live hash; `None` when the file must be absent.
    pub pre: Option<String>,
}

/// A file to delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDelete {
    /// Collection path.
    pub path: RelPath,
    /// The expected live hash.
    pub pre: String,
}

/// A path to put back (restore): collection path or `ledger.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRestore {
    /// Collection path or [`LEDGER_KEY`].
    pub path: String,
    /// The pre-run hash; `None` when the path did not exist.
    pub pre: Option<String>,
}

/// The ledger swap of a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkLedger {
    /// SHA-256 of the staged new ledger.
    pub new_sha256: String,
    /// The expected live ledger hash; `None` when there is none yet.
    pub pre_sha256: Option<String>,
}

/// The parsed work list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkList {
    /// What to do.
    pub op: WorkOp,
    /// The run.
    pub run_id: String,
    /// The gateway directory name.
    pub gateway: String,
    /// The mode collection.
    pub mode: String,
    /// Files to write, byte-sorted, index = position.
    pub writes: Vec<WorkWrite>,
    /// Files to delete, byte-sorted, index = position.
    pub deletes: Vec<WorkDelete>,
    /// The ledger swap, when there is one.
    pub ledger: Option<WorkLedger>,
    /// Paths to put back (restore) or to read (fingerprint), index = position.
    pub restores: Vec<WorkRestore>,
    /// Files a mirror deletes, byte-sorted, index = position.
    pub mirror_deletes: Vec<RelPath>,
    /// How often a held move is retried before it fails.
    pub retries: u32,
    /// Pre-rendered files the operation installs, each named with its hash.
    pub docs: Vec<WorkDoc>,
    /// The journal seen before the operation (restore, finalize, discard).
    pub journal_seen: Option<SeenJournal>,
    /// Where the staged files are, when not `<staging>/files` (mirror).
    pub source: Option<String>,
}

/// The most retries a work list may ask for.
const RETRIES_MAX: u32 = 10;

impl WorkList {
    /// An empty list for `op`; the builders below fill in what each needs.
    pub fn empty(op: WorkOp, run_id: &str, gateway: &str, mode: &str) -> WorkList {
        WorkList {
            op,
            run_id: run_id.to_string(),
            gateway: gateway.to_string(),
            mode: mode.to_string(),
            writes: Vec::new(),
            deletes: Vec::new(),
            ledger: None,
            restores: Vec::new(),
            mirror_deletes: Vec::new(),
            retries: RENAME_RETRIES,
            docs: Vec::new(),
            journal_seen: None,
            source: None,
        }
    }

    /// The work list of a commit.
    pub fn from_spec(spec: &BatchSpec) -> Result<WorkList, String> {
        let mut list = WorkList::empty(
            WorkOp::Commit,
            spec.run_id.as_str(),
            &spec.gateway,
            &spec.mode,
        );
        list.writes = spec
            .writes
            .iter()
            .map(|w| WorkWrite {
                path: w.path.clone(),
                sha256: w.sha256.clone(),
                pre: w.pre.clone(),
            })
            .collect();
        list.deletes = spec
            .deletes
            .iter()
            .map(|d| WorkDelete {
                path: d.path.clone(),
                pre: d.pre.clone(),
            })
            .collect();
        list.ledger = spec.ledger_new.as_deref().map(|bytes| WorkLedger {
            new_sha256: crate::deploy::tree::sha256_hex(bytes),
            pre_sha256: spec.ledger_pre.clone(),
        });
        for phase in PRERENDERED_PHASES {
            list.docs.push(WorkDoc {
                name: journal_file(phase),
                sha256: crate::deploy::tree::sha256_hex(&spec.journal_at(phase).to_bytes()),
            });
        }
        list.docs.push(WorkDoc {
            name: record_file(RecordState::Applying),
            sha256: crate::deploy::tree::sha256_hex(&spec.record.to_bytes()),
        });
        list.validate()?;
        Ok(list)
    }

    /// The work list of a restore of `record`.
    pub fn for_restore(record: &RunRecord) -> Result<WorkList, String> {
        let mut list = WorkList::empty(
            WorkOp::Restore,
            &record.run_id,
            &record.gateway,
            &record.mode,
        );
        list.restores = record
            .touched
            .iter()
            .map(|e| WorkRestore {
                path: e.path.clone(),
                pre: e.pre_sha256.clone(),
            })
            .collect();
        list.docs.push(WorkDoc {
            name: journal_file(JournalPhase::Restoring),
            sha256: crate::deploy::tree::sha256_hex(&restoring_journal(record).to_bytes()),
        });
        list.validate()?;
        Ok(list)
    }

    /// The work list that installs `record` as the run's final record
    /// (`finalize`).
    pub fn for_finalize(record: &RunRecord, seen: SeenJournal) -> Result<WorkList, String> {
        let mut list = WorkList::empty(
            WorkOp::Finalize,
            &record.run_id,
            &record.gateway,
            &record.mode,
        );
        list.docs.push(WorkDoc {
            name: record_file(record.state),
            sha256: crate::deploy::tree::sha256_hex(&record.to_bytes()),
        });
        list.journal_seen = Some(seen);
        list.validate()?;
        Ok(list)
    }

    /// The work list that drops a run that never moved a file (`discard`).
    pub fn for_discard(
        run_id: &str,
        gateway: &str,
        mode: &str,
        seen: SeenJournal,
    ) -> Result<WorkList, String> {
        let mut list = WorkList::empty(WorkOp::Discard, run_id, gateway, mode);
        list.journal_seen = Some(seen);
        list.validate()?;
        Ok(list)
    }

    /// The work list that removes a run's staging directory (`unstage`).
    pub fn for_unstage(run_id: &str, gateway: &str, mode: &str) -> Result<WorkList, String> {
        let list = WorkList::empty(WorkOp::Unstage, run_id, gateway, mode);
        list.validate()?;
        Ok(list)
    }

    /// The work list of a writability probe of the `mode` collection and the
    /// `ign-deploy` root of `gateway`.
    pub fn for_probe(gateway: &str, mode: &str) -> Result<WorkList, String> {
        let list = WorkList::empty(WorkOp::Probe, "-", gateway, mode);
        list.validate()?;
        Ok(list)
    }

    /// The work list that reads the live hash of each of `paths`.
    pub fn for_fingerprint(
        gateway: &str,
        mode: &str,
        paths: &[String],
    ) -> Result<WorkList, String> {
        let mut list = WorkList::empty(WorkOp::Fingerprint, "-", gateway, mode);
        list.restores = paths
            .iter()
            .map(|path| WorkRestore {
                path: path.clone(),
                pre: None,
            })
            .collect();
        list.validate()?;
        Ok(list)
    }

    /// The work list of a mirror commit: `writes` (byte-sorted, with the
    /// SHA-256 of each staged copy) are moved in from `source`, `deletes`
    /// (byte-sorted) are removed.
    pub fn for_mirror(
        mode: &str,
        source: &str,
        writes: &[(RelPath, String)],
        deletes: &[RelPath],
    ) -> Result<WorkList, String> {
        let mut list = WorkList::empty(WorkOp::Mirror, "-", "-", mode);
        list.source = Some(source.to_string());
        list.writes = writes
            .iter()
            .map(|(path, sha)| WorkWrite {
                path: path.clone(),
                sha256: sha.clone(),
                pre: None,
            })
            .collect();
        list.mirror_deletes = deletes.to_vec();
        list.validate()?;
        Ok(list)
    }

    /// The first thing wrong with the list, as a value-free problem.
    fn validate(&self) -> Result<(), String> {
        if self.op.is_runless() {
            if self.run_id != "-" {
                return Err("this operation belongs to no run".to_string());
            }
        } else if RunId::parse(&self.run_id).is_err() {
            return Err("the run id is not valid".to_string());
        }
        if self.op.is_gatewayless() {
            if self.gateway != "-" {
                return Err("this operation belongs to no gateway".to_string());
            }
        } else if let Some(why) = crate::deploy::component_problem(&self.gateway) {
            return Err(format!("the gateway {why}"));
        }
        if let Some(why) = super::mirror_mode_problem(&self.mode) {
            return Err(why);
        }
        for text in [&self.run_id, &self.gateway, &self.mode] {
            clean(text)?;
        }
        if self.retries > RETRIES_MAX {
            return Err("the retry count is over the limit".to_string());
        }
        for w in &self.writes {
            clean(w.path.as_str())?;
            if let Some(why) = mirror_path_problem(&w.path) {
                return Err(why);
            }
            hex(&w.sha256)?;
            optional_hex(w.pre.as_deref())?;
        }
        for d in &self.deletes {
            clean(d.path.as_str())?;
            if let Some(why) = mirror_path_problem(&d.path) {
                return Err(why);
            }
            hex(&d.pre)?;
        }
        for path in &self.mirror_deletes {
            clean(path.as_str())?;
            if let Some(why) = mirror_path_problem(path) {
                return Err(why);
            }
        }
        if let Some(ledger) = &self.ledger {
            hex(&ledger.new_sha256)?;
            optional_hex(ledger.pre_sha256.as_deref())?;
        }
        for r in &self.restores {
            clean(&r.path)?;
            if r.path != LEDGER_KEY {
                let rel = RelPath::new(&r.path)?;
                if let Some(why) = mirror_path_problem(&rel) {
                    return Err(why);
                }
            }
            optional_hex(r.pre.as_deref())?;
        }
        let mut seen_docs = BTreeSet::new();
        for doc in &self.docs {
            if !doc_name_ok(&doc.name) {
                return Err("a staged file name is not one of the pre-rendered names".to_string());
            }
            if !seen_docs.insert(doc.name.as_str()) {
                return Err("a staged file is listed twice".to_string());
            }
            hex(&doc.sha256)?;
        }
        if let Some(SeenJournal::Hash(h)) = &self.journal_seen {
            hex(h)?;
        }
        if let Some(source) = &self.source {
            clean(source)?;
            if super::remote::safe_remote_path(source).is_err()
                || !source.starts_with('/')
                || source
                    .split('/')
                    .skip(1)
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err("the staged files directory is not a plain absolute path".to_string());
            }
        }
        let has = |writes: bool, deletes: bool, ledger: bool, restores: bool, xs: bool| {
            (self.writes.is_empty() || writes)
                && (self.deletes.is_empty() || deletes)
                && (self.ledger.is_none() || ledger)
                && (self.restores.is_empty() || restores)
                && (self.mirror_deletes.is_empty() || xs)
        };
        let shape_ok = match self.op {
            WorkOp::Commit => has(true, true, true, false, false),
            WorkOp::Mirror => has(true, false, false, false, true) && self.source.is_some(),
            WorkOp::Restore | WorkOp::Fingerprint => has(false, false, false, true, false),
            WorkOp::Probe | WorkOp::Finalize | WorkOp::Discard | WorkOp::Unstage => {
                has(false, false, false, false, false)
            }
        };
        let extras_ok = (self.source.is_none() || self.op == WorkOp::Mirror)
            && (self.journal_seen.is_none()
                || matches!(
                    self.op,
                    WorkOp::Restore | WorkOp::Finalize | WorkOp::Discard
                ));
        if !shape_ok || !extras_ok {
            return Err("the records do not match the operation".to_string());
        }
        Ok(())
    }

    /// The file bytes: the header and one line per record, LF-terminated.
    pub fn to_lines(&self) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        let dash = |v: Option<&str>| v.unwrap_or("-").to_string();
        let mut line = |fields: &[String]| {
            out.push_str(&fields.join("\t"));
            out.push('\n');
        };
        line(&["R".into(), self.run_id.clone()]);
        line(&["G".into(), self.gateway.clone()]);
        line(&["M".into(), self.mode.clone()]);
        line(&["O".into(), self.op.word().into()]);
        line(&["T".into(), self.retries.to_string()]);
        if let Some(source) = &self.source {
            line(&["S".into(), source.clone()]);
        }
        match &self.journal_seen {
            Some(SeenJournal::Absent) => line(&["J".into(), "-".into()]),
            Some(SeenJournal::Hash(h)) => line(&["J".into(), h.clone()]),
            None => {}
        }
        for doc in &self.docs {
            line(&["F".into(), doc.name.clone(), doc.sha256.clone()]);
        }
        for (i, w) in self.writes.iter().enumerate() {
            line(&[
                "W".into(),
                i.to_string(),
                w.sha256.clone(),
                dash(w.pre.as_deref()),
                w.path.as_str().to_string(),
            ]);
        }
        for (i, d) in self.deletes.iter().enumerate() {
            line(&[
                "D".into(),
                i.to_string(),
                d.pre.clone(),
                d.path.as_str().to_string(),
            ]);
        }
        for (i, path) in self.mirror_deletes.iter().enumerate() {
            line(&["X".into(), i.to_string(), path.as_str().to_string()]);
        }
        if let Some(l) = &self.ledger {
            line(&[
                "L".into(),
                l.new_sha256.clone(),
                dash(l.pre_sha256.as_deref()),
            ]);
        }
        for (i, r) in self.restores.iter().enumerate() {
            line(&[
                "P".into(),
                i.to_string(),
                dash(r.pre.as_deref()),
                r.path.clone(),
            ]);
        }
        out.into_bytes()
    }

    /// Parse `bytes`. Strict: the header, no unknown record, no repeated
    /// single record, consecutive indices, every hash and path valid.
    pub fn parse_lines(bytes: &[u8]) -> Result<WorkList, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "the work list is not UTF-8")?;
        let body = text
            .strip_suffix('\n')
            .ok_or("the work list does not end with a newline")?;
        let mut lines = body.split('\n');
        if lines.next() != Some(HEADER) {
            return Err(format!("the work list does not start with `{HEADER}`"));
        }
        let (mut run_id, mut gateway, mut mode, mut op) = (None, None, None, None);
        let (mut retries, mut journal, mut source): (
            Option<String>,
            Option<String>,
            Option<String>,
        ) = (None, None, None);
        let mut list = WorkList::empty(WorkOp::Commit, "", "", "");
        let dash = |v: &str| (v != "-").then(|| v.to_string());
        for (n, line) in lines.enumerate() {
            let n = n + 2;
            let f: Vec<&str> = line.split('\t').collect();
            let once = |slot: &mut Option<String>, v: &str| -> Result<(), String> {
                if slot.replace(v.to_string()).is_some() {
                    return Err(format!("line {n}: the record is repeated"));
                }
                Ok(())
            };
            let index = |text: &str, expected: usize| -> Result<(), String> {
                if text.parse::<usize>().ok() != Some(expected) {
                    return Err(format!("line {n}: the index is not {expected}"));
                }
                Ok(())
            };
            let arity = |want: usize| -> Result<(), String> {
                if f.len() == want {
                    Ok(())
                } else {
                    Err(format!(
                        "line {n}: expected {want} fields, found {}",
                        f.len()
                    ))
                }
            };
            match f[0] {
                "R" => {
                    arity(2)?;
                    once(&mut run_id, f[1])?;
                }
                "G" => {
                    arity(2)?;
                    once(&mut gateway, f[1])?;
                }
                "M" => {
                    arity(2)?;
                    once(&mut mode, f[1])?;
                }
                "O" => {
                    arity(2)?;
                    once(&mut op, f[1])?;
                }
                "T" => {
                    arity(2)?;
                    once(&mut retries, f[1])?;
                }
                "J" => {
                    arity(2)?;
                    once(&mut journal, f[1])?;
                }
                "S" => {
                    arity(2)?;
                    once(&mut source, f[1])?;
                }
                "F" => {
                    arity(3)?;
                    list.docs.push(WorkDoc {
                        name: f[1].to_string(),
                        sha256: f[2].to_string(),
                    });
                }
                "W" => {
                    arity(5)?;
                    index(f[1], list.writes.len())?;
                    list.writes.push(WorkWrite {
                        path: RelPath::new(f[4]).map_err(|e| format!("line {n}: {e}"))?,
                        sha256: f[2].to_string(),
                        pre: dash(f[3]),
                    });
                }
                "D" => {
                    arity(4)?;
                    index(f[1], list.deletes.len())?;
                    list.deletes.push(WorkDelete {
                        path: RelPath::new(f[3]).map_err(|e| format!("line {n}: {e}"))?,
                        pre: f[2].to_string(),
                    });
                }
                "X" => {
                    arity(3)?;
                    index(f[1], list.mirror_deletes.len())?;
                    list.mirror_deletes
                        .push(RelPath::new(f[2]).map_err(|e| format!("line {n}: {e}"))?);
                }
                "L" => {
                    arity(3)?;
                    if list.ledger.is_some() {
                        return Err(format!("line {n}: the record is repeated"));
                    }
                    list.ledger = Some(WorkLedger {
                        new_sha256: f[1].to_string(),
                        pre_sha256: dash(f[2]),
                    });
                }
                "P" => {
                    arity(4)?;
                    index(f[1], list.restores.len())?;
                    list.restores.push(WorkRestore {
                        path: f[3].to_string(),
                        pre: dash(f[2]),
                    });
                }
                other => {
                    return Err(format!(
                        "line {n}: unknown record `{}`",
                        other
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(8)
                            .collect::<String>()
                    ));
                }
            }
        }
        let need = |v: Option<String>, what: &str| v.ok_or(format!("the {what} record is missing"));
        list.run_id = need(run_id, "R")?;
        list.gateway = need(gateway, "G")?;
        list.mode = need(mode, "M")?;
        list.op = WorkOp::from_word(&need(op, "O")?)
            .ok_or("the operation is not one this build knows")?;
        if let Some(text) = retries {
            list.retries = text
                .parse::<u32>()
                .map_err(|_| "the retry count is not a number".to_string())?;
        }
        list.journal_seen = journal.map(|v| {
            if v == "-" {
                SeenJournal::Absent
            } else {
                SeenJournal::Hash(v)
            }
        });
        list.source = source;
        list.validate()?;
        Ok(list)
    }
}

/// Whether `name` is a pre-rendered journal or record file name.
fn doc_name_ok(name: &str) -> bool {
    let phases = [
        JournalPhase::Snapshotting,
        JournalPhase::Ready,
        JournalPhase::Writing,
        JournalPhase::Written,
        JournalPhase::Restoring,
    ];
    phases.iter().any(|p| journal_file(*p) == name)
        || [
            RecordState::Applying,
            RecordState::Applied,
            RecordState::RolledBack,
        ]
        .iter()
        .any(|s| record_file(*s) == name)
}

/// Refuse TAB, CR, LF and every other control character in a field.
fn clean(text: &str) -> Result<(), String> {
    if text.chars().any(char::is_control) {
        return Err("a field has a TAB, a line break or another control character".to_string());
    }
    Ok(())
}

fn hex(text: &str) -> Result<(), String> {
    if text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err("a hash is not 64 lowercase hex digits".to_string())
    }
}

fn optional_hex(text: Option<&str>) -> Result<(), String> {
    text.map_or(Ok(()), hex)
}

/// The name of the pre-rendered journal file for `phase`.
pub fn journal_file(phase: JournalPhase) -> String {
    format!(
        "journal.{}.json",
        match phase {
            JournalPhase::Snapshotting => "snapshotting",
            JournalPhase::Ready => "ready",
            JournalPhase::Writing => "writing",
            JournalPhase::Written => "written",
            JournalPhase::Restoring => "restoring",
        }
    )
}

/// The name of the pre-rendered record file for `state`.
pub fn record_file(state: RecordState) -> String {
    format!(
        "record.{}.json",
        match state {
            RecordState::Applying => "applying",
            RecordState::Applied => "applied",
            RecordState::RolledBack => "rolled_back",
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const H2: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn rel(p: &str) -> RelPath {
        RelPath::new(p).expect("rel")
    }

    const RUN: &str = "20261015T142233Z-a1b2c3";

    fn commit_list() -> WorkList {
        let mut list = WorkList::empty(WorkOp::Commit, RUN, "gw", "dev");
        list.writes = vec![
            WorkWrite {
                path: rel("ignition/type/a/config.json"),
                sha256: H1.to_string(),
                pre: None,
            },
            WorkWrite {
                path: rel("ignition/type/b/config.json"),
                sha256: H1.to_string(),
                pre: Some(H2.to_string()),
            },
        ];
        list.deletes = vec![WorkDelete {
            path: rel("ignition/type/c/config.json"),
            pre: H2.to_string(),
        }];
        list.ledger = Some(WorkLedger {
            new_sha256: H1.to_string(),
            pre_sha256: None,
        });
        list.docs = vec![WorkDoc {
            name: "journal.ready.json".to_string(),
            sha256: H2.to_string(),
        }];
        list
    }

    fn restore_list() -> WorkList {
        let mut list = WorkList::empty(WorkOp::Restore, RUN, "gw", "dev");
        list.restores = vec![
            WorkRestore {
                path: "ignition/type/a/config.json".to_string(),
                pre: None,
            },
            WorkRestore {
                path: LEDGER_KEY.to_string(),
                pre: Some(H2.to_string()),
            },
        ];
        list.journal_seen = Some(SeenJournal::Hash(H1.to_string()));
        list
    }

    #[test]
    fn commit_list_round_trips() {
        let list = commit_list();
        let bytes = list.to_lines();
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(
            text.starts_with("ign-work 1\nR\t20261015T142233Z-a1b2c3\n"),
            "{text}"
        );
        assert!(text.contains(&format!("W\t0\t{H1}\t-\tignition/type/a/config.json\n")));
        assert!(text.contains(&format!("W\t1\t{H1}\t{H2}\tignition/type/b/config.json\n")));
        assert!(text.contains(&format!("D\t0\t{H2}\tignition/type/c/config.json\n")));
        assert!(text.contains(&format!("L\t{H1}\t-\n")));
        assert_eq!(WorkList::parse_lines(&bytes).expect("parse"), list);
    }

    #[test]
    fn restore_list_round_trips() {
        let list = restore_list();
        assert_eq!(
            WorkList::parse_lines(&list.to_lines()).expect("parse"),
            list
        );
    }

    #[test]
    fn a_control_character_in_a_path_is_refused_on_build_and_on_parse() {
        for bad in ["a\tb", "a\nb", "a\rb", "a\u{1}b"] {
            let mut list = restore_list();
            list.restores[0].path = format!("ignition/type/{bad}/config.json");
            assert!(list.validate().is_err(), "{bad:?} on build");
        }
        // Parse side: a raw TAB inside the path field makes an extra field.
        let text = format!(
            "ign-work 1\nR\t20261015T142233Z-a1b2c3\nG\tgw\nM\tdev\nO\tcommit\nW\t0\t{H1}\t-\tignition/type/a\tb/config.json\n"
        );
        assert!(WorkList::parse_lines(text.as_bytes()).is_err());
        // A control character in the path itself.
        let text = format!(
            "ign-work 1\nR\t20261015T142233Z-a1b2c3\nG\tgw\nM\tdev\nO\tcommit\nW\t0\t{H1}\t-\tignition/type/a\u{1}b/config.json\n"
        );
        let err = WorkList::parse_lines(text.as_bytes()).expect_err("control");
        assert!(err.contains("control") || err.contains("path"), "{err}");
    }

    #[test]
    fn each_malformed_list_is_refused() {
        let good = String::from_utf8(commit_list().to_lines()).expect("utf8");
        let cases: Vec<(&str, String)> = vec![
            ("no header", good.replacen("ign-work 1", "ign-work 2", 1)),
            ("no final newline", good.trim_end().to_string()),
            ("unknown record", format!("{good}Z\t1\n")),
            ("repeated R", format!("{good}R\tx\n")),
            ("repeated L", format!("{good}L\t{H1}\t-\n")),
            (
                "short W",
                good.replace(&format!("W\t0\t{H1}\t-\t"), "W\t0\t"),
            ),
            ("index gap", good.replace("W\t1\t", "W\t2\t")),
            ("bad hash", good.replacen(H1, "xyz", 1)),
            ("upper hash", good.replacen(H1, &H1.to_uppercase(), 1)),
            ("missing G", good.replace("G\tgw\n", "")),
            ("bad op", good.replace("O\tcommit", "O\tpublish")),
            (
                "gateway-owned file",
                good.replace(
                    "ignition/type/c/config.json",
                    "ignition/type/config-mode.json",
                ),
            ),
            (
                "shallow path",
                good.replace("ignition/type/c/config.json", "ignition/c.json"),
            ),
            (
                "dot component",
                good.replace("ignition/type/c/config.json", "ignition/.type/c.json"),
            ),
            ("not utf-8", String::new()),
        ];
        for (name, text) in cases {
            let bytes = if name == "not utf-8" {
                vec![0xff, 0xfe]
            } else {
                text.into_bytes()
            };
            assert!(
                WorkList::parse_lines(&bytes).is_err(),
                "{name} must be refused"
            );
        }
    }

    #[test]
    fn a_restore_record_in_a_commit_list_is_refused() {
        let mut list = commit_list();
        list.restores.push(WorkRestore {
            path: LEDGER_KEY.to_string(),
            pre: None,
        });
        assert!(list.validate().is_err());
    }

    #[test]
    fn the_other_operations_round_trip_and_refuse_foreign_records() {
        let lists = [
            WorkList::for_probe("gw", "dev").expect("probe"),
            WorkList::for_fingerprint(
                "gw",
                "dev",
                &[
                    "ignition/type/a/config.json".to_string(),
                    LEDGER_KEY.to_string(),
                ],
            )
            .expect("fingerprint"),
            WorkList::for_discard(RUN, "gw", "dev", SeenJournal::Hash(H1.to_string()))
                .expect("discard"),
            WorkList::for_discard(RUN, "gw", "dev", SeenJournal::Absent).expect("discard"),
            WorkList::for_unstage(RUN, "gw", "dev").expect("unstage"),
            WorkList::for_mirror(
                "dev",
                "/home/svc/stage",
                &[(rel("ignition/type/a/config.json"), H1.to_string())],
                &[rel("ignition/type/c/config.json")],
            )
            .expect("mirror"),
        ];
        for list in lists {
            let bytes = list.to_lines();
            assert_eq!(WorkList::parse_lines(&bytes).expect("parse"), list);
        }
        // a probe belongs to no run; a mirror to no gateway either
        let mut bad = WorkList::empty(WorkOp::Probe, RUN, "gw", "dev");
        assert!(bad.validate().is_err());
        bad = WorkList::empty(WorkOp::Mirror, "-", "gw", "dev");
        bad.source = Some("/a/b".to_string());
        assert!(bad.validate().is_err());
        // a mirror needs an absolute, plain source; only a mirror has one
        assert!(WorkList::for_mirror("dev", "relative", &[], &[]).is_err());
        assert!(WorkList::for_mirror("dev", "/a/../b", &[], &[]).is_err());
        let mut list = commit_list();
        list.source = Some("/a/b".to_string());
        assert!(list.validate().is_err());
        // a retry count over the limit and an unknown staged name are refused
        let mut list = commit_list();
        list.retries = 11;
        assert!(list.validate().is_err());
        let mut list = commit_list();
        list.docs[0].name = "../journal.ready.json".to_string();
        assert!(list.validate().is_err());
    }

    #[test]
    fn the_script_and_the_stub_agree_on_what_is_hashed() {
        // the stub hashes `$(cat script)` plus one newline, so the script must
        // end with exactly one newline and hold no NUL
        assert!(SCRIPT_SH.ends_with('\n') && !SCRIPT_SH.ends_with("\n\n"));
        assert!(!SCRIPT_SH.contains('\0'));
        assert!(SCRIPT_SH.contains("IGN-RESULT:"));
        // the stub is one line without a control character, so it passes
        // RemoteCommand::posix, and carries no path
        assert!(!POSIX_STUB.chars().any(char::is_control), "{POSIX_STUB:?}");
        assert!(!POSIX_STUB.contains('/') || !POSIX_STUB.contains("ign-deploy"));
        let command = crate::deploy::transport::remote::RemoteCommand::posix(&[
            "sh",
            "-c",
            POSIX_STUB,
            "ign-stub",
            "/h/commit.sh",
            "commit",
            "/h/staging",
            "/d/config/resources/dev",
            "/h/ign-deploy/gw/dev",
            &script_sh_sha256(),
            H1,
        ])
        .expect("the stub passes the quoting rules");
        assert!(command.as_str().starts_with("'sh' '-c' '"));
    }

    #[test]
    fn pre_rendered_names_are_fixed() {
        assert_eq!(
            journal_file(JournalPhase::Snapshotting),
            "journal.snapshotting.json"
        );
        assert_eq!(journal_file(JournalPhase::Written), "journal.written.json");
        assert_eq!(record_file(RecordState::Applying), "record.applying.json");
        assert_eq!(
            record_file(RecordState::RolledBack),
            "record.rolled_back.json"
        );
        assert_eq!(WORK_FILE, "work.tsv");
        assert_eq!(RENAME_RETRIES, 3);
    }
}
