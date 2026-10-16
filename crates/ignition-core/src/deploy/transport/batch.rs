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
//! O<TAB>commit | restore
//! W<TAB><index><TAB><sha256><TAB><pre sha256 or -><TAB><path>     (commit)
//! D<TAB><index><TAB><pre sha256><TAB><path>                       (commit)
//! L<TAB><new ledger sha256 or -><TAB><prior ledger sha256 or ->   (commit)
//! P<TAB><index><TAB><pre sha256 or -><TAB><path>                  (restore)
//! ```
//!
//! Fields are TAB-separated. A path (or any text) containing TAB, CR, LF or
//! any control character is refused when the list is built and when it is
//! parsed. A host reports each entry's result by its index. A `P` record's
//! path is a collection path or `ledger.json`.

use super::mirror_path_problem;
use crate::deploy::apply::fingerprint::LEDGER_KEY;
use crate::deploy::apply::host::BatchSpec;
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

/// What the work list asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkOp {
    /// Journal, check, snapshot, move, delete, swap the ledger.
    Commit,
    /// Put the snapshot back.
    Restore,
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
    /// Paths to put back, index = position (restore).
    pub restores: Vec<WorkRestore>,
}

impl WorkList {
    /// The work list of a commit.
    pub fn from_spec(spec: &BatchSpec) -> Result<WorkList, String> {
        let list = WorkList {
            op: WorkOp::Commit,
            run_id: spec.run_id.as_str().to_string(),
            gateway: spec.gateway.clone(),
            mode: spec.mode.clone(),
            writes: spec
                .writes
                .iter()
                .map(|w| WorkWrite {
                    path: w.path.clone(),
                    sha256: w.sha256.clone(),
                    pre: w.pre.clone(),
                })
                .collect(),
            deletes: spec
                .deletes
                .iter()
                .map(|d| WorkDelete {
                    path: d.path.clone(),
                    pre: d.pre.clone(),
                })
                .collect(),
            ledger: spec.ledger_new.as_deref().map(|bytes| WorkLedger {
                new_sha256: crate::deploy::tree::sha256_hex(bytes),
                pre_sha256: spec.ledger_pre.clone(),
            }),
            restores: Vec::new(),
        };
        list.validate()?;
        Ok(list)
    }

    /// The work list of a restore of `record`.
    pub fn for_restore(record: &RunRecord) -> Result<WorkList, String> {
        let list = WorkList {
            op: WorkOp::Restore,
            run_id: record.run_id.clone(),
            gateway: record.gateway.clone(),
            mode: record.mode.clone(),
            writes: Vec::new(),
            deletes: Vec::new(),
            ledger: None,
            restores: record
                .touched
                .iter()
                .map(|e| WorkRestore {
                    path: e.path.clone(),
                    pre: e.pre_sha256.clone(),
                })
                .collect(),
        };
        list.validate()?;
        Ok(list)
    }

    /// The first thing wrong with the list, as a value-free problem.
    fn validate(&self) -> Result<(), String> {
        if RunId::parse(&self.run_id).is_err() {
            return Err("the run id is not valid".to_string());
        }
        if let Some(why) = crate::deploy::component_problem(&self.gateway) {
            return Err(format!("the gateway {why}"));
        }
        if let Some(why) = super::mirror_mode_problem(&self.mode) {
            return Err(why);
        }
        for text in [&self.run_id, &self.gateway, &self.mode] {
            clean(text)?;
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
        let shape_ok = match self.op {
            WorkOp::Commit => self.restores.is_empty(),
            WorkOp::Restore => {
                self.writes.is_empty() && self.deletes.is_empty() && self.ledger.is_none()
            }
        };
        if !shape_ok {
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
        line(&[
            "O".into(),
            match self.op {
                WorkOp::Commit => "commit",
                WorkOp::Restore => "restore",
            }
            .into(),
        ]);
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
        let mut list = WorkList {
            op: WorkOp::Commit,
            run_id: String::new(),
            gateway: String::new(),
            mode: String::new(),
            writes: Vec::new(),
            deletes: Vec::new(),
            ledger: None,
            restores: Vec::new(),
        };
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
        list.op = match need(op, "O")?.as_str() {
            "commit" => WorkOp::Commit,
            "restore" => WorkOp::Restore,
            _ => return Err("the operation is not commit or restore".to_string()),
        };
        list.validate()?;
        Ok(list)
    }
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

    fn commit_list() -> WorkList {
        WorkList {
            op: WorkOp::Commit,
            run_id: "20261015T142233Z-a1b2c3".to_string(),
            gateway: "gw".to_string(),
            mode: "dev".to_string(),
            writes: vec![
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
            ],
            deletes: vec![WorkDelete {
                path: rel("ignition/type/c/config.json"),
                pre: H2.to_string(),
            }],
            ledger: Some(WorkLedger {
                new_sha256: H1.to_string(),
                pre_sha256: None,
            }),
            restores: Vec::new(),
        }
    }

    fn restore_list() -> WorkList {
        WorkList {
            op: WorkOp::Restore,
            run_id: "20261015T142233Z-a1b2c3".to_string(),
            gateway: "gw".to_string(),
            mode: "dev".to_string(),
            writes: Vec::new(),
            deletes: Vec::new(),
            ledger: None,
            restores: vec![
                WorkRestore {
                    path: "ignition/type/a/config.json".to_string(),
                    pre: None,
                },
                WorkRestore {
                    path: LEDGER_KEY.to_string(),
                    pre: Some(H2.to_string()),
                },
            ],
        }
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
            ("bad op", good.replace("O\tcommit", "O\tmirror")),
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
