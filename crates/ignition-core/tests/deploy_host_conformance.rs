//! The conformance table every `ApplyHost` must pass (Phase 23, D-01).
//!
//! `scenarios()` is host-agnostic data: each row is a name and an async body
//! that drives a [`Ctx`] (a host plus the data directory it works on) and
//! asserts on the files it finds. The local host runs every row here
//! (`conformance_local_<row>`); the POSIX and PowerShell script hosts (23-06,
//! 23-20) supply another `Ctx` and run the same rows.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::host::{ApplyHost, BatchReport, BatchSpec, DeleteOp, WriteOp};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::record::{RecordState, RunRecord, TouchedEntry};
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::tree::{RenderedTree, sha256_hex};

const GW: &str = "conf-gw";
const MODE: &str = "dev";
const RUN: &str = "20261016T100000Z-aaaaaa";
const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// One conformance row.
pub struct Scenario {
    pub name: &'static str,
    pub run: for<'a> fn(&'a Ctx) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
}

macro_rules! row {
    ($name:ident) => {
        Scenario {
            name: stringify!($name),
            run: |ctx| Box::pin($name(ctx)),
        }
    };
}

/// Every row. A host passes when each one passes against it.
pub fn scenarios() -> Vec<Scenario> {
    vec![row!(add_one_file)]
}

/// A host and the data directory it works on.
pub struct Ctx {
    pub host: Box<dyn ApplyHost>,
    pub data: tempfile::TempDir,
}

/// What a batch needs: the spec and the rendered tree.
pub struct Batch {
    pub spec: BatchSpec,
    pub tree: RenderedTree,
}

impl Ctx {
    /// A local host over a fresh gateway data directory whose mode
    /// collection exists and is empty.
    pub fn local() -> Ctx {
        let data = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(data.path().join("config/resources").join(MODE))
            .expect("collection");
        let host = LocalFsTransport::new(data.path().to_path_buf(), "nodes.conf")
            .with_apply(GW, MODE)
            .expect("apply host");
        Ctx {
            host: Box::new(host),
            data,
        }
    }

    pub fn root(&self) -> &Path {
        self.data.path()
    }

    pub fn collection(&self) -> PathBuf {
        self.root().join("config/resources").join(MODE)
    }

    pub fn deploy_dir(&self) -> PathBuf {
        self.root().join("ign-deploy").join(GW).join(MODE)
    }

    /// Write `bytes` at `rel` in the collection (parents created).
    pub fn seed(&self, rel: &str, bytes: &[u8]) {
        let path = self.collection().join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, bytes).expect("seed");
    }

    /// Every file of the collection: path to SHA-256.
    pub fn listing(&self) -> BTreeMap<String, String> {
        fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
            let Ok(read) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in read {
                let entry = entry.expect("entry");
                let path = entry.path();
                let kind = entry.file_type().expect("type");
                if kind.is_dir() {
                    walk(base, &path, out);
                } else {
                    let rel = path.strip_prefix(base).expect("prefix");
                    out.insert(
                        rel.to_str().expect("utf8").replace('\\', "/"),
                        sha256_hex(&std::fs::read(&path).expect("read")),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.collection(), &self.collection(), &mut out);
        out
    }

    pub fn ledger_bytes(&self) -> Option<Vec<u8>> {
        std::fs::read(self.deploy_dir().join("ledger.json")).ok()
    }

    pub fn journal(&self) -> Option<Journal> {
        let bytes = std::fs::read(self.deploy_dir().join("journal.json")).ok()?;
        Some(Journal::parse("journal.json", &bytes, GW, MODE).expect("journal parses"))
    }

    pub fn snapshot_dir(&self, run: &str) -> PathBuf {
        self.deploy_dir().join("snapshots").join(run)
    }

    pub fn staging_dir(&self, run: &str) -> PathBuf {
        self.deploy_dir().join("staging").join(run)
    }

    pub fn record(&self, run: &str) -> Option<RunRecord> {
        let bytes = std::fs::read(self.snapshot_dir(run).join("record.json")).ok()?;
        Some(RunRecord::parse("record.json", &bytes, GW, MODE).expect("record parses"))
    }

    /// A batch for `run` made against the files as they are now: each
    /// write's and delete's `pre` is the live hash, the ledger's pre is the
    /// live ledger's.
    pub fn batch(
        &self,
        run: &str,
        writes: &[(&str, &[u8])],
        deletes: &[&str],
        ledger: Option<&[u8]>,
    ) -> Batch {
        let live = self.listing();
        let mut tree = RenderedTree::default();
        let mut write_ops = Vec::new();
        for (path, bytes) in writes {
            let rel = RelPath::new(path).expect("rel");
            tree.files.insert(rel.clone(), bytes.to_vec());
            write_ops.push(WriteOp {
                path: rel,
                sha256: sha256_hex(bytes),
                pre: live.get(*path).cloned(),
            });
        }
        write_ops.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
        let mut delete_ops: Vec<DeleteOp> = deletes
            .iter()
            .map(|path| DeleteOp {
                path: RelPath::new(path).expect("rel"),
                pre: live
                    .get(*path)
                    .cloned()
                    .expect("a delete names a live file"),
            })
            .collect();
        delete_ops.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
        let ledger_pre = self.ledger_bytes().map(|b| sha256_hex(&b));

        let mut touched: Vec<TouchedEntry> = write_ops
            .iter()
            .map(|w| TouchedEntry {
                path: w.path.as_str().to_string(),
                pre_sha256: w.pre.clone(),
                post_sha256: Some(w.sha256.clone()),
            })
            .chain(delete_ops.iter().map(|d| TouchedEntry {
                path: d.path.as_str().to_string(),
                pre_sha256: Some(d.pre.clone()),
                post_sha256: None,
            }))
            .collect();
        if let Some(bytes) = ledger {
            touched.push(TouchedEntry {
                path: LEDGER_KEY.to_string(),
                pre_sha256: ledger_pre.clone(),
                post_sha256: Some(sha256_hex(bytes)),
            });
        }
        touched.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        let pre_map: BTreeMap<String, Option<String>> = touched
            .iter()
            .map(|e| (e.path.clone(), e.pre_sha256.clone()))
            .collect();
        let pre_fingerprint = fingerprint(&pre_map);
        let journal = Journal {
            schema_version: 1,
            run_id: run.to_string(),
            gateway: GW.to_string(),
            mode: MODE.to_string(),
            phase: JournalPhase::Snapshotting,
            pre_fingerprint: pre_fingerprint.clone(),
            touched: touched.iter().map(|e| e.path.clone()).collect(),
        };
        let record = RunRecord {
            schema_version: 1,
            run_id: run.to_string(),
            gateway: GW.to_string(),
            mode: MODE.to_string(),
            render_sha256: HASH.to_string(),
            plan_sha256: HASH.to_string(),
            state: RecordState::Applying,
            rolled_back_by: None,
            touched,
            pre_fingerprint,
            post_fingerprint: None,
        };
        Batch {
            spec: BatchSpec {
                run_id: RunId::parse(run).expect("run id"),
                gateway: GW.to_string(),
                mode: MODE.to_string(),
                writes: write_ops,
                deletes: delete_ops,
                ledger_new: ledger.map(<[u8]>::to_vec),
                ledger_pre,
                journal,
                record,
            },
            tree,
        }
    }

    /// `prepare` then `commit` of `batch`.
    pub async fn apply(&self, batch: &Batch) -> BatchReport {
        let prepared = self
            .host
            .prepare(&batch.spec, &batch.tree)
            .await
            .expect("prepare");
        self.host.commit(&prepared).await.expect("commit")
    }
}

/// A ledger for the test gateway that lists `files`.
pub fn ledger_bytes(run: &str, files: &[&str]) -> Vec<u8> {
    Ledger {
        schema_version: 1,
        gateway: GW.to_string(),
        mode: MODE.to_string(),
        render_sha256: HASH.to_string(),
        run_id: run.to_string(),
        files: files.iter().map(|f| (*f).to_string()).collect(),
    }
    .to_bytes()
}

// ---- rows ----------------------------------------------------------------

const FILE_A: &str = "ignition/tag-provider/default/config.json";

/// One new file and a first ledger are committed under a journal and a
/// snapshot, then restored to exactly the pre-run listing.
async fn add_one_file(c: &Ctx) {
    let before = c.listing();
    let ledger = ledger_bytes(RUN, &[FILE_A]);
    let batch = c.batch(RUN, &[(FILE_A, br#"{"a":1}"#)], &[], Some(&ledger));
    let report = c.apply(&batch).await;

    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert!(!report.stale && report.failure.is_none(), "{report:?}");
    assert_eq!(report.moved, vec![0]);
    let after = c.listing();
    assert_eq!(after.get(FILE_A), Some(&sha256_hex(br#"{"a":1}"#)));
    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Written);
    let record = c.record(RUN).expect("record.json");
    assert_eq!(record.state, RecordState::Applying);
    let entry = record
        .touched
        .iter()
        .find(|e| e.path == FILE_A)
        .expect("entry");
    assert_eq!(entry.pre_sha256, None, "the file did not exist before");
    assert_eq!(c.ledger_bytes().as_deref(), Some(ledger.as_slice()));
    Ledger::parse("ledger.json", &c.ledger_bytes().expect("ledger"), GW, MODE)
        .expect("ledger parses");

    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before, "restore returns the pre-run listing");
    assert_eq!(c.ledger_bytes(), None, "the first ledger is removed");
    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Restoring);
    assert!(c.snapshot_dir(RUN).is_dir(), "the snapshot stays");
}

// ---- the local host --------------------------------------------------------

async fn run_local(name: &str) {
    let scenario = scenarios()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no row named {name}"));
    let ctx = Ctx::local();
    (scenario.run)(&ctx).await;
}

#[tokio::test]
async fn conformance_local_add_one_file() {
    run_local("add_one_file").await;
}
