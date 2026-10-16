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
use ignition_core::deploy::transport::local::{ApplyFault, LocalFsTransport};
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
    vec![
        row!(add_one_file),
        row!(fingerprint_is_read_only),
        row!(finalize_removes_journal_and_staging_keeps_snapshot),
        row!(discard_interrupted_touches_no_collection_file),
        row!(fetch_snapshot_copies_with_hashes),
        row!(read_record_absent_is_none),
        row!(probe_reports_writable_without_touching_the_collection),
        row!(replace_file),
        row!(delete_file_and_emptied_dirs),
        row!(ledger_swap_and_restore),
        row!(pre_image_mismatch_aborts_before_any_move),
        row!(journal_present_refuses),
        row!(self_move_refused),
        row!(gateway_owned_file_refused),
        row!(directory_where_file_goes_is_stale),
        row!(symlinked_target_dir_is_refused),
        row!(restore_twice_is_idempotent),
    ]
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

    /// A local host as [`Ctx::local`] with `fault` injected.
    pub fn local_faulty(fault: ApplyFault) -> Ctx {
        let data = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(data.path().join("config/resources").join(MODE))
            .expect("collection");
        let host = LocalFsTransport::new(data.path().to_path_buf(), "nodes.conf")
            .with_apply(GW, MODE)
            .expect("apply host")
            .with_apply_fault(fault);
        Ctx {
            host: Box::new(host),
            data,
        }
    }

    /// A second local host over the same data directory, with no fault.
    pub fn clean_host(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.root().to_path_buf(), "nodes.conf")
            .with_apply(GW, MODE)
            .expect("apply host")
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

const FILE_B: &str = "ignition/tag-provider/default/other.json";

/// Every entry of the collection, directories included, so a stray directory
/// or temp file shows.
fn tree_names(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read {
            let path = entry.expect("entry").path();
            out.push(
                path.strip_prefix(base)
                    .expect("prefix")
                    .display()
                    .to_string(),
            );
            if path.is_dir() {
                walk(base, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// `fingerprint` reports each path's live hash, `None` for an absent path and
/// the ledger by its key, and changes nothing.
async fn fingerprint_is_read_only(c: &Ctx) {
    c.seed(FILE_A, b"one");
    c.seed(FILE_B, b"two");
    let before = (c.listing(), tree_names(c.root()));
    let paths = vec![
        FILE_A.to_string(),
        "ignition/tag-provider/default/absent.json".to_string(),
        LEDGER_KEY.to_string(),
    ];
    let got = c.host.fingerprint(&paths).await.expect("fingerprint");
    assert_eq!(got.get(FILE_A), Some(&Some(sha256_hex(b"one"))));
    assert_eq!(
        got.get("ignition/tag-provider/default/absent.json"),
        Some(&None)
    );
    assert_eq!(got.get(LEDGER_KEY), Some(&None), "no ledger yet");
    assert_eq!((c.listing(), tree_names(c.root())), before);

    // A directory where a file should be is an error, never `None`.
    let err = c
        .host
        .fingerprint(&["ignition/tag-provider/default".to_string()])
        .await
        .expect_err("a two-component path is not a collection path");
    assert_eq!(err.code(), "transport_failed", "{err}");
    std::fs::create_dir_all(
        c.collection()
            .join("ignition/tag-provider/dirnamed/config.json"),
    )
    .expect("dir");
    let err = c
        .host
        .fingerprint(&["ignition/tag-provider/dirnamed/config.json".to_string()])
        .await
        .expect_err("a directory is not a regular file");
    assert_eq!(err.code(), "transport_failed", "{err}");

    // With a ledger on disk it is reported by its hash.
    let ledger = ledger_bytes(RUN, &[FILE_A]);
    std::fs::create_dir_all(c.deploy_dir()).expect("deploy dir");
    std::fs::write(c.deploy_dir().join("ledger.json"), &ledger).expect("ledger");
    let got = c
        .host
        .fingerprint(&[LEDGER_KEY.to_string()])
        .await
        .expect("fingerprint");
    assert_eq!(got.get(LEDGER_KEY), Some(&Some(sha256_hex(&ledger))));
}

/// `finalize` installs the final record and removes the journal and this
/// run's staging; the snapshot stays.
async fn finalize_removes_journal_and_staging_keeps_snapshot(c: &Ctx) {
    let ledger = ledger_bytes(RUN, &[FILE_A]);
    let batch = c.batch(RUN, &[(FILE_A, b"x")], &[], Some(&ledger));
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written);
    assert!(
        c.staging_dir(RUN).is_dir(),
        "staging exists before finalize"
    );
    assert!(c.journal().is_some());

    let mut record = c.record(RUN).expect("record");
    record.state = RecordState::Applied;
    let post: BTreeMap<String, Option<String>> = record.post_map();
    record.post_fingerprint = Some(fingerprint(&post));
    c.host.finalize(&record).await.expect("finalize");

    assert!(c.journal().is_none(), "the journal is gone");
    assert!(
        !c.staging_dir(RUN).exists(),
        "the staging directory is gone"
    );
    assert!(c.snapshot_dir(RUN).is_dir(), "the snapshot stays");
    assert_eq!(c.record(RUN).expect("record").state, RecordState::Applied);
    assert!(
        c.listing().contains_key(FILE_A),
        "the collection is untouched"
    );
}

/// `discard_interrupted` drops a run that never moved a file and touches
/// nothing in the collection; it refuses a run that may have.
async fn discard_interrupted_touches_no_collection_file(c: &Ctx) {
    c.seed(FILE_A, b"live");
    let ledger = ledger_bytes(RUN, &[FILE_A]);
    let batch = c.batch(RUN, &[(FILE_A, b"new")], &[], Some(&ledger));
    // An interrupted run before its first move: staged, journal at ready,
    // snapshot started.
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    std::fs::copy(
        c.staging_dir(RUN).join("journal.ready.json"),
        c.deploy_dir().join("journal.json"),
    )
    .expect("journal");
    std::fs::create_dir_all(c.snapshot_dir(RUN).join("files")).expect("snapshot");
    let _ = prepared;
    let before = (c.listing(), tree_names(&c.collection()));

    c.host
        .discard_interrupted(&batch.spec.run_id)
        .await
        .expect("discard");
    assert!(c.journal().is_none());
    assert!(!c.snapshot_dir(RUN).exists());
    assert!(!c.staging_dir(RUN).exists());
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
    assert_eq!(c.ledger_bytes(), None, "the ledger is not touched");

    // A run whose journal says writing may have moved files: refused.
    let batch = c.batch(RUN, &[(FILE_A, b"new")], &[], None);
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written);
    let err = c
        .host
        .discard_interrupted(&batch.spec.run_id)
        .await
        .expect_err("a written run is restored, not discarded");
    assert_eq!(err.code(), "deploy_run_invalid", "{err}");
    assert!(c.journal().is_some() && c.snapshot_dir(RUN).is_dir());
}

/// `fetch_snapshot` copies the snapshot to the runner with each file's hash,
/// owner-only on unix.
async fn fetch_snapshot_copies_with_hashes(c: &Ctx) {
    c.seed(FILE_A, b"old a");
    let prior = ledger_bytes("20261015T100000Z-bbbbbb", &[FILE_A]);
    std::fs::create_dir_all(c.deploy_dir()).expect("deploy dir");
    std::fs::write(c.deploy_dir().join("ledger.json"), &prior).expect("ledger");
    let next = ledger_bytes(RUN, &[FILE_A, FILE_B]);
    let batch = c.batch(
        RUN,
        &[(FILE_A, b"new a"), (FILE_B, b"new b")],
        &[],
        Some(&next),
    );
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");

    let dest = tempfile::tempdir().expect("dest");
    let hashes = c
        .host
        .fetch_snapshot(&batch.spec.run_id, dest.path())
        .await
        .expect("fetch");
    let copy = dest.path().join("snapshot");
    let old = std::fs::read(copy.join("files").join(FILE_A)).expect("pre-image");
    assert_eq!(old, b"old a");
    assert_eq!(
        std::fs::read(copy.join("ledger.json")).expect("ledger"),
        prior
    );
    assert!(copy.join("record.json").is_file());
    assert!(
        !copy.join("files").join(FILE_B).exists(),
        "a new file has no pre-image"
    );
    assert_eq!(
        hashes.get(&format!("files/{FILE_A}")),
        Some(&sha256_hex(b"old a"))
    );
    assert_eq!(hashes.get("ledger.json"), Some(&sha256_hex(&prior)));
    assert!(hashes.contains_key("record.json"));
    assert_eq!(hashes.len(), 3, "{hashes:?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for rel in [
            format!("files/{FILE_A}"),
            "ledger.json".to_string(),
            "record.json".to_string(),
        ] {
            let mode = std::fs::metadata(copy.join(&rel))
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{rel} is owner-only");
        }
    }
}

/// `read_record` is `None` for a run with no snapshot, and the record once
/// commit wrote it.
async fn read_record_absent_is_none(c: &Ctx) {
    let run = RunId::parse(RUN).expect("run");
    assert!(c.host.read_record(&run).await.expect("read").is_none());
    let batch = c.batch(RUN, &[(FILE_A, b"x")], &[], None);
    c.apply(&batch).await;
    let record = c
        .host
        .read_record(&run)
        .await
        .expect("read")
        .expect("record");
    assert_eq!(record.run_id, RUN);
    assert_eq!(record.state, RecordState::Applying);
    let other = RunId::parse("20261016T100000Z-bbbbbb").expect("run");
    assert!(c.host.read_record(&other).await.expect("read").is_none());
}

/// `probe` writes nothing into the collection, and an unwritable ign-deploy
/// root is reported.
async fn probe_reports_writable_without_touching_the_collection(c: &Ctx) {
    c.seed(FILE_A, b"live");
    let before = (c.listing(), tree_names(&c.collection()));
    let report = c.host.probe().await.expect("probe");
    assert!(report.writable, "{report:?}");
    assert!(report.problems.is_empty(), "{report:?}");
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
    let leftovers: Vec<String> = tree_names(&c.deploy_dir());
    assert!(
        leftovers.is_empty(),
        "the probe file is removed: {leftovers:?}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Root ignores directory modes, so the unwritable case needs a user.
        if !rustix_is_root() {
            let dir = c.deploy_dir();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
            let report = c.host.probe().await.expect("probe");
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .expect("chmod back");
            assert!(!report.writable, "{report:?}");
            assert!(!report.problems.is_empty());
            assert_eq!((c.listing(), tree_names(&c.collection())), before);
        }
    }
}

#[cfg(unix)]
fn rustix_is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| o.stdout == b"0\n")
        .unwrap_or(false)
}

// ---- commit and restore rule rows ----------------------------------------------

const RUN2: &str = "20261016T100000Z-bbbbbb";

/// A file that exists is replaced by moving the staged copy over it: the new
/// bytes are live, the pre-image is in the snapshot, the collection holds no
/// temp or backup entry, and restore puts the old bytes back.
async fn replace_file(c: &Ctx) {
    c.seed(FILE_A, b"old");
    let before = (c.listing(), tree_names(&c.collection()));
    let batch = c.batch(RUN, &[(FILE_A, b"new")], &[], None);
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert_eq!(report.moved, vec![0]);
    assert!(report.failure.is_none() && !report.stale, "{report:?}");
    assert_eq!(c.listing().get(FILE_A), Some(&sha256_hex(b"new")));
    assert_eq!(
        tree_names(&c.collection()),
        before.1,
        "no temp, backup or dot-named entry appears in the collection"
    );
    assert_eq!(
        std::fs::read(c.snapshot_dir(RUN).join("files").join(FILE_A)).expect("pre-image"),
        b"old"
    );
    let record = c.record(RUN).expect("record");
    let entry = record
        .touched
        .iter()
        .find(|e| e.path == FILE_A)
        .expect("entry");
    assert_eq!(entry.pre_sha256, Some(sha256_hex(b"old")));
    assert_eq!(entry.post_sha256, Some(sha256_hex(b"new")));

    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
}

/// A delete removes the file and each directory it emptied, deepest first,
/// but never `<module>/<type>`; restore recreates both.
async fn delete_file_and_emptied_dirs(c: &Ctx) {
    const DEEP: &str = "ignition/tag-provider/default/sub/deep.json";
    const OTHER: &str = "ignition/tag-provider/other/keep.json";
    const SIBLING: &str = "ignition/perspective/kept/stay.json";
    c.seed(DEEP, b"deep");
    c.seed(OTHER, b"other");
    c.seed(SIBLING, b"sibling");
    let before = (c.listing(), tree_names(&c.collection()));
    let batch = c.batch(RUN, &[], &[DEEP, OTHER], None);
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert_eq!(report.deleted, vec![0, 1]);
    assert_eq!(
        report.removed_dirs,
        vec![
            "ignition/tag-provider/default".to_string(),
            "ignition/tag-provider/default/sub".to_string(),
            "ignition/tag-provider/other".to_string(),
        ]
    );
    let names = tree_names(&c.collection());
    assert!(
        names.contains(&"ignition/tag-provider".to_string()),
        "the <module>/<type> directory stays: {names:?}"
    );
    assert!(
        names.contains(&SIBLING.to_string()),
        "an untouched sibling stays: {names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|n| n.contains("default") || n.contains("other"))
    );

    let record = c.record(RUN).expect("record");
    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
}

/// A prior ledger is snapshotted, replaced by the new one, and put back by
/// restore.
async fn ledger_swap_and_restore(c: &Ctx) {
    let prior = ledger_bytes("20261015T100000Z-cccccc", &[FILE_A]);
    std::fs::create_dir_all(c.deploy_dir()).expect("deploy dir");
    std::fs::write(c.deploy_dir().join("ledger.json"), &prior).expect("ledger");
    c.seed(FILE_A, b"old");
    let before = c.listing();
    let next = ledger_bytes(RUN, &[FILE_A, FILE_B]);
    let batch = c.batch(RUN, &[(FILE_A, b"new"), (FILE_B, b"b")], &[], Some(&next));
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert_eq!(c.ledger_bytes().as_deref(), Some(next.as_slice()));
    assert_eq!(
        std::fs::read(c.snapshot_dir(RUN).join("ledger.json")).expect("snapshot ledger"),
        prior,
        "the prior ledger is snapshotted"
    );

    let record = c.record(RUN).expect("record");
    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.ledger_bytes().as_deref(), Some(prior.as_slice()));
    assert_eq!(c.listing(), before);
}

/// A live file that is not what the plan was made against aborts the commit
/// before the first move: the report is `stale`, the journal and the partial
/// snapshot are gone and the collection and ledger are untouched.
async fn pre_image_mismatch_aborts_before_any_move(c: &Ctx) {
    // A changed file.
    c.seed(FILE_A, b"old");
    let ledger = ledger_bytes(RUN, &[FILE_A, FILE_B]);
    let batch = c.batch(RUN, &[(FILE_A, b"new"), (FILE_B, b"b")], &[], Some(&ledger));
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    c.seed(FILE_A, b"changed behind the plan");
    let before = (c.listing(), tree_names(&c.collection()));

    let report = c.host.commit(&prepared).await.expect("commit");
    assert!(report.stale, "{report:?}");
    assert!(
        report.moved.is_empty() && report.deleted.is_empty(),
        "{report:?}"
    );
    assert!(report.failure.is_some(), "{report:?}");
    assert!(c.journal().is_none(), "the journal is removed");
    assert!(
        !c.snapshot_dir(RUN).exists(),
        "the partial snapshot is removed"
    );
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
    assert_eq!(c.ledger_bytes(), None, "the ledger is untouched");

    // A changed ledger.
    let batch = c.batch(RUN, &[(FILE_A, b"new")], &[], Some(&ledger));
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let other = ledger_bytes("20261015T100000Z-dddddd", &[FILE_B]);
    std::fs::create_dir_all(c.deploy_dir()).expect("deploy dir");
    std::fs::write(c.deploy_dir().join("ledger.json"), &other).expect("ledger");
    let report = c.host.commit(&prepared).await.expect("commit");
    assert!(report.stale, "{report:?}");
    assert!(c.journal().is_none() && !c.snapshot_dir(RUN).exists());
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
    assert_eq!(c.ledger_bytes().as_deref(), Some(other.as_slice()));
}

/// A journal that already exists makes `commit` refuse before any write, so
/// two runs against one mode collection cannot both proceed (DAPL-09).
async fn journal_present_refuses(c: &Ctx) {
    let first = c.batch(RUN, &[(FILE_A, b"a")], &[], None);
    let report = c.apply(&first).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");

    let second = c.batch(RUN2, &[(FILE_B, b"b")], &[], None);
    let prepared = c
        .host
        .prepare(&second.spec, &second.tree)
        .await
        .expect("prepare");
    let before = (c.listing(), tree_names(&c.collection()));
    let journal_before =
        std::fs::read(c.deploy_dir().join("journal.json")).expect("first run's journal");

    let err = c
        .host
        .commit(&prepared)
        .await
        .expect_err("a second run is refused");
    assert_eq!(err.code(), "deploy_journal_present", "{err}");
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
    assert_eq!(
        std::fs::read(c.deploy_dir().join("journal.json")).expect("journal"),
        journal_before,
        "the first run's journal is not touched"
    );
    assert!(
        !c.snapshot_dir(RUN2).exists(),
        "nothing was snapshotted for the refused run"
    );
}

/// A staged copy that is the very file it would replace is never moved onto
/// itself (C7a-4: a self-rename deletes the file). Detecting it needs the
/// inode, which the unix host has; the row is a no-op elsewhere.
async fn self_move_refused(c: &Ctx) {
    #[cfg(unix)]
    {
        c.seed(FILE_A, b"same bytes");
        let batch = c.batch(RUN, &[(FILE_A, b"same bytes")], &[], None);
        let prepared = c
            .host
            .prepare(&batch.spec, &batch.tree)
            .await
            .expect("prepare");
        // The staged copy becomes a second name for the live file.
        let staged = c.staging_dir(RUN).join("files").join(FILE_A);
        std::fs::remove_file(&staged).expect("remove staged copy");
        std::fs::hard_link(c.collection().join(FILE_A), &staged).expect("hard link");

        let report = c.host.commit(&prepared).await.expect("commit");
        assert_eq!(report.phase, JournalPhase::Writing, "{report:?}");
        assert!(report.moved.is_empty(), "{report:?}");
        let failure = report.failure.expect("a failure");
        assert!(failure.contains("write 0"), "{failure}");
        assert_eq!(
            std::fs::read(c.collection().join(FILE_A)).expect("the file survives"),
            b"same bytes"
        );
    }
    #[cfg(not(unix))]
    let _ = c;
}

/// A path the gateway owns, a dot-named component or a path with fewer than
/// three components is refused before any I/O by `prepare` and `restore`
/// (research Pitfall 21).
async fn gateway_owned_file_refused(c: &Ctx) {
    c.seed(FILE_A, b"live");
    let before = (c.listing(), tree_names(&c.collection()));
    for bad in [
        "ignition/tag-provider/config-mode.json",
        "ignition/tag-provider/.hidden/x.json",
        "ignition/tag-provider",
    ] {
        let batch = c.batch(RUN, &[(bad, b"x")], &[], None);
        let err = c
            .host
            .prepare(&batch.spec, &batch.tree)
            .await
            .expect_err("prepare refuses the path");
        assert_eq!(err.code(), "transport_failed", "{bad}: {err}");
        assert!(
            !c.deploy_dir().exists(),
            "{bad}: nothing was written before the refusal"
        );
        assert_eq!((c.listing(), tree_names(&c.collection())), before);
    }

    // `restore` reads the paths from the record, so it refuses them too.
    let good = c.batch(RUN, &[(FILE_B, b"b")], &[], None);
    c.apply(&good).await;
    let mut record = c.record(RUN).expect("record");
    let journal_before = std::fs::read(c.deploy_dir().join("journal.json")).expect("journal");
    let after_commit = (c.listing(), tree_names(&c.collection()));
    record.touched[0].path = "ignition/tag-provider/config-mode.json".to_string();
    let err = c
        .host
        .restore(&record)
        .await
        .expect_err("restore refuses the path");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert_eq!((c.listing(), tree_names(&c.collection())), after_commit);
    assert_eq!(
        std::fs::read(c.deploy_dir().join("journal.json")).expect("journal"),
        journal_before,
        "restore wrote nothing, not even its journal"
    );
}

/// A directory where a file must go is a pre-image mismatch: the commit is
/// `stale`, nothing moves and the directory stays.
async fn directory_where_file_goes_is_stale(c: &Ctx) {
    c.seed(FILE_B, b"other");
    let batch = c.batch(RUN, &[(FILE_A, b"a"), (FILE_B, b"b")], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    // After the plan was made, a directory appears where FILE_A goes.
    c.seed(&format!("{FILE_A}/inner.json"), b"inner");
    let before = (c.listing(), tree_names(&c.collection()));

    let report = c.host.commit(&prepared).await.expect("commit");
    assert!(report.stale, "{report:?}");
    assert!(report.moved.is_empty(), "{report:?}");
    assert!(c.journal().is_none() && !c.snapshot_dir(RUN).exists());
    assert_eq!((c.listing(), tree_names(&c.collection())), before);
    assert!(c.collection().join(FILE_A).is_dir(), "the directory stays");
}

/// A directory of the target path that became a symlink after the plan was
/// made is not followed: the commit is `stale` and nothing is written through
/// the link.
async fn symlinked_target_dir_is_refused(c: &Ctx) {
    #[cfg(unix)]
    {
        let batch = c.batch(RUN, &[(FILE_A, b"a")], &[], None);
        let prepared = c
            .host
            .prepare(&batch.spec, &batch.tree)
            .await
            .expect("prepare");
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), c.collection().join("ignition"))
            .expect("symlink");

        let report = c.host.commit(&prepared).await.expect("commit");
        assert!(report.stale, "{report:?}");
        assert!(report.moved.is_empty(), "{report:?}");
        assert!(c.journal().is_none() && !c.snapshot_dir(RUN).exists());
        assert_eq!(
            std::fs::read_dir(outside.path()).expect("outside").count(),
            0,
            "nothing was written through the link"
        );
    }
    #[cfg(not(unix))]
    let _ = c;
}

/// Running `restore` twice for one run leaves byte-identical collection
/// contents and ledger (DAPL-07 idempotency).
async fn restore_twice_is_idempotent(c: &Ctx) {
    let prior = ledger_bytes("20261015T100000Z-cccccc", &[FILE_A]);
    std::fs::create_dir_all(c.deploy_dir()).expect("deploy dir");
    std::fs::write(c.deploy_dir().join("ledger.json"), &prior).expect("ledger");
    c.seed(FILE_A, b"old a");
    c.seed(FILE_C, b"old c");
    let before = (c.listing(), tree_names(&c.collection()));
    let next = ledger_bytes(RUN, &[FILE_A, FILE_B]);
    let batch = c.batch(
        RUN,
        &[(FILE_A, b"new a"), (FILE_B, b"new b")],
        &[FILE_C],
        Some(&next),
    );
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    let record = c.record(RUN).expect("record");

    let first = c.host.restore(&record).await.expect("first restore");
    assert!(first.failure.is_none(), "{first:?}");
    let once = (c.listing(), tree_names(&c.collection()), c.ledger_bytes());
    assert_eq!((once.0.clone(), once.1.clone()), before);
    assert_eq!(once.2.as_deref(), Some(prior.as_slice()));

    let second = c.host.restore(&record).await.expect("second restore");
    assert!(second.failure.is_none(), "{second:?}");
    let twice = (c.listing(), tree_names(&c.collection()), c.ledger_bytes());
    assert_eq!(twice, once, "a second restore changes nothing");
    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Restoring);
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

#[tokio::test]
async fn conformance_local_fingerprint_is_read_only() {
    run_local("fingerprint_is_read_only").await;
}

#[tokio::test]
async fn conformance_local_finalize_removes_journal_and_staging_keeps_snapshot() {
    run_local("finalize_removes_journal_and_staging_keeps_snapshot").await;
}

#[tokio::test]
async fn conformance_local_discard_interrupted_touches_no_collection_file() {
    run_local("discard_interrupted_touches_no_collection_file").await;
}

#[tokio::test]
async fn conformance_local_fetch_snapshot_copies_with_hashes() {
    run_local("fetch_snapshot_copies_with_hashes").await;
}

#[tokio::test]
async fn conformance_local_read_record_absent_is_none() {
    run_local("read_record_absent_is_none").await;
}

#[tokio::test]
async fn conformance_local_probe_reports_writable_without_touching_the_collection() {
    run_local("probe_reports_writable_without_touching_the_collection").await;
}

#[tokio::test]
async fn conformance_local_replace_file() {
    run_local("replace_file").await;
}

#[tokio::test]
async fn conformance_local_delete_file_and_emptied_dirs() {
    run_local("delete_file_and_emptied_dirs").await;
}

#[tokio::test]
async fn conformance_local_ledger_swap_and_restore() {
    run_local("ledger_swap_and_restore").await;
}

#[tokio::test]
async fn conformance_local_pre_image_mismatch_aborts_before_any_move() {
    run_local("pre_image_mismatch_aborts_before_any_move").await;
}

#[tokio::test]
async fn conformance_local_journal_present_refuses() {
    run_local("journal_present_refuses").await;
}

#[tokio::test]
async fn conformance_local_self_move_refused() {
    run_local("self_move_refused").await;
}

#[tokio::test]
async fn conformance_local_gateway_owned_file_refused() {
    run_local("gateway_owned_file_refused").await;
}

#[tokio::test]
async fn conformance_local_directory_where_file_goes_is_stale() {
    run_local("directory_where_file_goes_is_stale").await;
}

#[tokio::test]
async fn conformance_local_symlinked_target_dir_is_refused() {
    run_local("symlinked_target_dir_is_refused").await;
}

#[tokio::test]
async fn conformance_local_restore_twice_is_idempotent() {
    run_local("restore_twice_is_idempotent").await;
}

// ---- fault rows (local host only) ---------------------------------------------

const FILE_C: &str = "ignition/tag-provider/default/third.json";

/// Three writes, the middle one over an existing file, so a failure at index
/// 1 leaves the first moved and the second still the pre-run bytes.
fn three_writes(c: &Ctx) -> Batch {
    c.seed(FILE_B, b"old b");
    c.batch(
        RUN,
        &[(FILE_A, b"new a"), (FILE_B, b"new b"), (FILE_C, b"new c")],
        &[],
        None,
    )
}

/// A move that fails mid-commit is reported by index at phase `writing`; the
/// snapshot is complete, the journal says `writing`, and `restore` returns
/// the collection to the pre-run listing.
#[tokio::test]
async fn fail_move_at_leaves_a_recoverable_partial_tree() {
    let c = Ctx::local_faulty(ApplyFault::FailMoveAt(1));
    let batch = three_writes(&c);
    let before = c.listing();

    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Writing, "{report:?}");
    assert!(!report.stale, "{report:?}");
    assert_eq!(report.moved, vec![0], "only the first write moved");
    let failure = report.failure.clone().expect("a failure");
    assert!(failure.contains("write 1"), "{failure}");

    let after = c.listing();
    assert_eq!(after.get(FILE_A), Some(&sha256_hex(b"new a")));
    assert_eq!(after.get(FILE_B), Some(&sha256_hex(b"old b")), "not moved");
    assert_eq!(after.get(FILE_C), None, "never reached");
    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Writing);
    assert_eq!(
        std::fs::read(c.snapshot_dir(RUN).join("files").join(FILE_B)).expect("pre-image"),
        b"old b",
        "the snapshot holds the pre-image"
    );

    let record = c.record(RUN).expect("record.json");
    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before, "restore returns the pre-run listing");
}

/// A restore that fails is reported by index with the journal left at
/// `restoring`; a second restore on a clean host finishes the job.
#[tokio::test]
async fn fail_restore_at_reports_the_index() {
    let c = Ctx::local_faulty(ApplyFault::FailRestoreAt(1));
    let batch = three_writes(&c);
    let before = c.listing();
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    let record = c.record(RUN).expect("record.json");

    let failed = c.host.restore(&record).await.expect("restore");
    assert_eq!(failed.phase, JournalPhase::Restoring, "{failed:?}");
    let failure = failed.failure.clone().expect("a failure");
    assert!(failure.contains("restore 1"), "{failure}");
    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Restoring);
    assert_ne!(c.listing(), before, "the restore stopped part way");

    let clean = c.clean_host();
    let restored = clean.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before, "the second restore finishes the job");
}

/// Every row has a test above, so adding a row without wiring it fails here.
#[test]
fn every_row_runs_against_the_local_host() {
    let source = include_str!("deploy_host_conformance.rs");
    for row in scenarios() {
        let wired = format!("run_local(\"{}\")", row.name);
        assert!(
            source.contains(&wired),
            "row {} is not run locally",
            row.name
        );
    }
}
