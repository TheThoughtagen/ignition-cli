//! The conformance table every `ApplyHost` must pass (Phase 23, D-01).
//!
//! `scenarios()` is host-agnostic data: each row is a name and an async body
//! that drives a [`Ctx`] (a host plus the data directory it works on) and
//! asserts on the files it finds. The local host runs every row here
//! (`conformance_local_<row>`); the POSIX script host (23-06, unix) and the
//! PowerShell script host (23-20, windows) supply another `Ctx` and run the
//! same rows (`conformance_sh_<row>`, `conformance_ps_<row>`).

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::Notify;

mod common;
use common::faulty_host::FaultyHost;

use ignition_core::deploy::apply::fingerprint::{LEDGER_KEY, fingerprint};
use ignition_core::deploy::apply::host::{ApplyHost, BatchReport, BatchSpec, DeleteOp, WriteOp};
use ignition_core::deploy::apply::journal::{Journal, JournalPhase};
use ignition_core::deploy::apply::record::{RecordState, RunRecord, TouchedEntry};
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::layout::RelPath;
use ignition_core::deploy::ledger::Ledger;
use ignition_core::deploy::transport::local::{ApplyFault, LocalFsTransport};
#[cfg(any(unix, windows))]
use ignition_core::deploy::transport::process::TokioProcessRunner;
#[cfg(any(unix, windows))]
use ignition_core::deploy::transport::remote::RemoteShell;
#[cfg(any(unix, windows))]
use ignition_core::deploy::transport::sftp::SftpTransport;
#[cfg(unix)]
use ignition_core::deploy::transport::sftp::posix_root;
#[cfg(windows)]
use ignition_core::deploy::transport::sftp::windows_sftp_root;
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

    /// The POSIX script host over ssh, driven through the local `sftp-server`
    /// and a local `sh` (the `local_exec` seam): the same gateway data
    /// directory shape as [`Ctx::local`], with the account home being the data
    /// directory so `ign-deploy` lands where the rows look for it.
    #[cfg(unix)]
    pub fn sh() -> Ctx {
        let data = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(data.path().join("config/resources").join(MODE))
            .expect("collection");
        let host = sh_host(data.path());
        Ctx {
            host: Box::new(host),
            data,
        }
    }

    /// The PowerShell script host, driven through the Win32-OpenSSH
    /// `sftp-server.exe` of the runner and the local `cmd /C powershell` (the
    /// `local_exec` seam): the same gateway data directory shape as
    /// [`Ctx::local`], with the account home being the data directory so
    /// `ign-deploy` lands where the rows look for it.
    #[cfg(windows)]
    pub fn ps() -> Ctx {
        let data = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(data.path().join("config/resources").join(MODE))
            .expect("collection");
        let host = ps_host(data.path());
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
                commit_budget_s: None,
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

/// An `SftpTransport` over `root` as the data directory and the account home.
#[cfg(unix)]
pub fn sh_host(root: &Path) -> SftpTransport {
    let shell = RemoteShell::local_sftp_server(
        &common::sftp_server::sftp_server_path(),
        Arc::new(TokioProcessRunner),
        std::time::Duration::from_secs(120),
    )
    .local_exec();
    let data_dir = posix_root(&root.display().to_string()).expect("posix root");
    SftpTransport::new(shell, data_dir.clone(), "nodes.sh")
        .with_home(&data_dir)
        .with_apply(GW, MODE)
        .expect("apply host")
}

/// An `SftpTransport` for a Windows node over `root` as the data directory and
/// the account home: sftp to the runner's own `sftp-server.exe`, the script
/// through the local `cmd /C powershell`.
#[cfg(windows)]
pub fn ps_host(root: &Path) -> SftpTransport {
    let shell = RemoteShell::local_sftp_server(
        &common::sftp_server::sftp_server_path(),
        Arc::new(TokioProcessRunner),
        std::time::Duration::from_secs(120),
    )
    .local_exec();
    let data_dir = windows_sftp_root(&root.display().to_string()).expect("windows sftp root");
    SftpTransport::new_windows(shell, data_dir.clone(), "nodes.ps")
        .with_home(&data_dir)
        .with_apply(GW, MODE)
        .expect("apply host")
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
                    .to_string()
                    .replace('\\', "/"),
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

// ---- the POSIX script host ---------------------------------------------------

#[cfg(unix)]
async fn run_sh(name: &str) {
    let scenario = scenarios()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no row named {name}"));
    let ctx = Ctx::sh();
    (scenario.run)(&ctx).await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_add_one_file() {
    run_sh("add_one_file").await;
}

/// One byte of the staged script is changed after upload: the stub exits
/// before running anything, the call is `transport_failed` and the collection,
/// the journal and the snapshots are untouched (T-23-25).
#[cfg(unix)]
#[tokio::test]
async fn stub_refuses_a_tampered_script() {
    let c = Ctx::sh();
    c.seed(FILE_B, b"live");
    let before = (c.listing(), tree_names(c.root()));
    let batch = c.batch(RUN, &[(FILE_A, br#"{"a":1}"#)], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let script = c.staging_dir(RUN).join("commit.sh");
    let mut bytes = std::fs::read(&script).expect("the script was uploaded");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    std::fs::write(&script, bytes).expect("tamper");
    let staged = tree_names(c.root());

    let err = c
        .host
        .commit(&prepared)
        .await
        .expect_err("a tampered script must not run");
    assert_eq!(err.code(), "transport_failed", "{err}");
    let text = err.to_string();
    assert!(text.contains("commit"), "names the op: {text}");
    assert!(text.contains("does not match its hash"), "{text}");
    assert_eq!(c.listing(), before.0, "the collection is untouched");
    assert!(c.journal().is_none(), "no journal was written");
    assert!(!c.snapshot_dir(RUN).exists(), "no snapshot was made");
    assert_eq!(tree_names(c.root()), staged, "nothing else changed");
}

/// One byte of `work.tsv` is changed after upload: refused the same way.
#[cfg(unix)]
#[tokio::test]
async fn stub_refuses_a_tampered_work_list() {
    let c = Ctx::sh();
    c.seed(FILE_B, b"live");
    let before = c.listing();
    let batch = c.batch(RUN, &[(FILE_A, br#"{"a":1}"#)], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let work = c.staging_dir(RUN).join("work.tsv");
    let mut bytes = std::fs::read(&work).expect("the work list was uploaded");
    let at = bytes.iter().position(|b| *b == b'a').expect("a letter");
    bytes[at] = b'b';
    std::fs::write(&work, bytes).expect("tamper");

    let err = c
        .host
        .commit(&prepared)
        .await
        .expect_err("a tampered work list must not run");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(err.to_string().contains("does not match its hash"), "{err}");
    assert_eq!(c.listing(), before, "the collection is untouched");
    assert!(c.journal().is_none());
    assert!(!c.snapshot_dir(RUN).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_fingerprint_is_read_only() {
    run_sh("fingerprint_is_read_only").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_finalize_removes_journal_and_staging_keeps_snapshot() {
    run_sh("finalize_removes_journal_and_staging_keeps_snapshot").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_discard_interrupted_touches_no_collection_file() {
    run_sh("discard_interrupted_touches_no_collection_file").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_fetch_snapshot_copies_with_hashes() {
    run_sh("fetch_snapshot_copies_with_hashes").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_read_record_absent_is_none() {
    run_sh("read_record_absent_is_none").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_probe_reports_writable_without_touching_the_collection() {
    run_sh("probe_reports_writable_without_touching_the_collection").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_replace_file() {
    run_sh("replace_file").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_delete_file_and_emptied_dirs() {
    run_sh("delete_file_and_emptied_dirs").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_ledger_swap_and_restore() {
    run_sh("ledger_swap_and_restore").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_pre_image_mismatch_aborts_before_any_move() {
    run_sh("pre_image_mismatch_aborts_before_any_move").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_journal_present_refuses() {
    run_sh("journal_present_refuses").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_self_move_refused() {
    run_sh("self_move_refused").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_gateway_owned_file_refused() {
    run_sh("gateway_owned_file_refused").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_directory_where_file_goes_is_stale() {
    run_sh("directory_where_file_goes_is_stale").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_symlinked_target_dir_is_refused() {
    run_sh("symlinked_target_dir_is_refused").await;
}

#[cfg(unix)]
#[tokio::test]
async fn conformance_sh_restore_twice_is_idempotent() {
    run_sh("restore_twice_is_idempotent").await;
}

/// A moved file is 0644 and a directory the commit created is 0755, even when
/// the staged copy was 0600 (research A6; unproven against a live Linux
/// gateway host, which the README states).
#[cfg(unix)]
#[tokio::test]
async fn sh_moved_files_are_0644() {
    use std::os::unix::fs::PermissionsExt;
    let c = Ctx::sh();
    let batch = c.batch(RUN, &[(FILE_A, br#"{"a":1}"#)], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let staged = c.staging_dir(RUN).join("files").join(FILE_A);
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    let report = c.host.commit(&prepared).await.expect("commit");
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    let mode = |rel: &str| {
        std::fs::metadata(c.collection().join(rel))
            .expect("meta")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode(FILE_A), 0o644, "the moved file");
    for dir in [
        "ignition",
        "ignition/tag-provider",
        "ignition/tag-provider/default",
    ] {
        assert_eq!(mode(dir), 0o755, "{dir}");
    }
}

/// A target made unwritable after `prepare` fails the commit naming that
/// entry's index at phase `writing`; `restore` returns the pre-run listing.
#[cfg(unix)]
#[tokio::test]
async fn sh_failed_move_reports_index() {
    use std::os::unix::fs::PermissionsExt;
    if rustix_is_root() {
        return; // root ignores directory modes
    }
    const A: &str = "ignition/tag-provider/a/config.json";
    const B: &str = "ignition/tag-provider/b/config.json";
    let c = Ctx::sh();
    c.seed(A, b"old a");
    c.seed(B, b"old b");
    let before = c.listing();
    let batch = c.batch(RUN, &[(A, b"new a"), (B, b"new b")], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let locked = c.collection().join("ignition/tag-provider/b");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("lock");

    let report = c.host.commit(&prepared).await.expect("commit");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlock");
    assert_eq!(report.phase, JournalPhase::Writing, "{report:?}");
    assert_eq!(report.moved, vec![0], "{report:?}");
    let failure = report.failure.clone().expect("a failure");
    assert!(!report.deadline, "no budget was set: {report:?}");
    assert!(failure.contains("write 1"), "{failure}");
    assert!(
        !failure.contains("config.json"),
        "no path in the reason: {failure}"
    );
    assert_eq!(c.listing().get(A), Some(&sha256_hex(b"new a")));
    assert_eq!(c.listing().get(B), Some(&sha256_hex(b"old b")));

    let record = c.record(RUN).expect("record");
    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before, "restore returns the pre-run listing");
}

/// Review WR-04: a held target is retried one second apart, and the retries
/// are bounded by the commit's time budget, not only by their count. With a
/// budget of one second the script stops before the second retry sleep, and
/// says so (`deadline`), so the engine can restore under the lock it still
/// holds. Nothing after the stop is moved.
#[cfg(unix)]
#[tokio::test]
async fn sh_commit_stops_when_its_time_budget_runs_out() {
    use std::os::unix::fs::PermissionsExt;
    if rustix_is_root() {
        return; // root ignores directory modes
    }
    const A: &str = "ignition/tag-provider/a/config.json";
    const B: &str = "ignition/tag-provider/b/config.json";
    const C: &str = "ignition/tag-provider/c/config.json";
    let c = Ctx::sh();
    c.seed(A, b"old a");
    c.seed(B, b"old b");
    c.seed(C, b"old c");
    let before = c.listing();
    let mut batch = c.batch(
        RUN,
        &[(A, b"new a"), (B, b"new b"), (C, b"new c")],
        &[],
        None,
    );
    batch.spec.commit_budget_s = Some(1);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let locked = c.collection().join("ignition/tag-provider/b");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("lock");

    let started = std::time::Instant::now();
    let report = c.host.commit(&prepared).await.expect("commit");
    let took = started.elapsed();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("unlock");

    assert!(report.deadline, "{report:?}");
    assert_eq!(report.phase, JournalPhase::Writing, "{report:?}");
    // A slow start can spend the second before the first write, so the stop
    // is at write 0 or write 1; nothing after it is moved either way.
    assert!(report.moved.len() <= 1, "{report:?}");
    let failure = report.failure.clone().expect("a failure");
    assert!(
        (failure.contains("write 0") || failure.contains("write 1"))
            && failure.contains("time budget"),
        "{failure}"
    );
    assert!(!failure.contains("config.json"), "no path: {failure}");
    // Three retries one second apart would take over three seconds.
    assert!(took < std::time::Duration::from_millis(2900), "{took:?}");
    assert_eq!(
        c.listing().get(C),
        Some(&sha256_hex(b"old c")),
        "never reached"
    );

    let record = c.record(RUN).expect("record");
    let restored = c.host.restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before, "restore returns the pre-run listing");
}

// ---- the PowerShell script host (23-20) ---------------------------------------
//
// Runs only on windows-latest: `commit.ps1` is executed by the runner's own
// Windows PowerShell 5.1 through the fixed stub, and the files move over the
// runner's Win32-OpenSSH `sftp-server.exe`. Each row of the shared table runs
// against it, and the rows below are the ones where PowerShell differs from
// `sh`: a target another process holds open, the access list after a move,
// a second name for the live file, and a junction instead of a symlink.

#[cfg(windows)]
async fn run_ps(name: &str) {
    let scenario = scenarios()
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no row named {name}"));
    let ctx = Ctx::ps();
    (scenario.run)(&ctx).await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_add_one_file() {
    run_ps("add_one_file").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_fingerprint_is_read_only() {
    run_ps("fingerprint_is_read_only").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_finalize_removes_journal_and_staging_keeps_snapshot() {
    run_ps("finalize_removes_journal_and_staging_keeps_snapshot").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_discard_interrupted_touches_no_collection_file() {
    run_ps("discard_interrupted_touches_no_collection_file").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_fetch_snapshot_copies_with_hashes() {
    run_ps("fetch_snapshot_copies_with_hashes").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_read_record_absent_is_none() {
    run_ps("read_record_absent_is_none").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_probe_reports_writable_without_touching_the_collection() {
    run_ps("probe_reports_writable_without_touching_the_collection").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_replace_file() {
    run_ps("replace_file").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_delete_file_and_emptied_dirs() {
    run_ps("delete_file_and_emptied_dirs").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_ledger_swap_and_restore() {
    run_ps("ledger_swap_and_restore").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_pre_image_mismatch_aborts_before_any_move() {
    run_ps("pre_image_mismatch_aborts_before_any_move").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_journal_present_refuses() {
    run_ps("journal_present_refuses").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_self_move_refused() {
    run_ps("self_move_refused").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_gateway_owned_file_refused() {
    run_ps("gateway_owned_file_refused").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_directory_where_file_goes_is_stale() {
    run_ps("directory_where_file_goes_is_stale").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_symlinked_target_dir_is_refused() {
    run_ps("symlinked_target_dir_is_refused").await;
}

#[cfg(windows)]
#[tokio::test]
async fn conformance_ps_restore_twice_is_idempotent() {
    run_ps("restore_twice_is_idempotent").await;
}

/// One byte of the staged script is changed after upload: the stub exits 97
/// before running anything, the call is `transport_failed` and the collection,
/// the journal and the snapshots are untouched (T-23-33).
#[cfg(windows)]
#[tokio::test]
async fn ps_stub_refuses_a_tampered_script_or_work_list() {
    for (file, tamper) in [("commit.ps1", 0usize), ("work.tsv", 1usize)] {
        let c = Ctx::ps();
        c.seed(FILE_B, b"live");
        let before = (c.listing(), tree_names(c.root()));
        let batch = c.batch(RUN, &[(FILE_A, br#"{"a":1}"#)], &[], None);
        let prepared = c
            .host
            .prepare(&batch.spec, &batch.tree)
            .await
            .expect("prepare");
        let path = c.staging_dir(RUN).join(file);
        let mut bytes = std::fs::read(&path).expect("the file was uploaded");
        let at = if tamper == 0 {
            bytes.len() / 2
        } else {
            bytes.iter().position(|b| *b == b'a').expect("a letter")
        };
        bytes[at] ^= 0x01;
        std::fs::write(&path, bytes).expect("tamper");
        let staged = tree_names(c.root());

        let err = c
            .host
            .commit(&prepared)
            .await
            .expect_err("a tampered file must not run");
        assert_eq!(err.code(), "transport_failed", "{file}: {err}");
        assert!(
            err.to_string().contains("does not match its hash"),
            "{file}: {err}"
        );
        assert_eq!(c.listing(), before.0, "{file}: the collection is untouched");
        assert!(c.journal().is_none(), "{file}: no journal was written");
        assert!(!c.snapshot_dir(RUN).exists(), "{file}: no snapshot");
        assert_eq!(tree_names(c.root()), staged, "{file}: nothing else changed");
    }
}

/// A target another process holds open without letting go of it. PowerShell
/// differs from `sh` here (C7b): a target held with `FILE_SHARE_READ` can be
/// read but not replaced, so the move fails after the bounded retries, the
/// commit names that entry's index at phase `writing`, and `restore` returns
/// the pre-run listing; a target held with no sharing at all cannot even be
/// hashed, which is a refusal (`held`) before the first move, not a stale plan
/// and not a partial write.
#[cfg(windows)]
#[tokio::test]
async fn ps_held_target_reports_index() {
    use std::os::windows::fs::OpenOptionsExt;
    const A: &str = "ignition/tag-provider/a/config.json";
    const B: &str = "ignition/tag-provider/b/config.json";
    let c = Ctx::ps();
    c.seed(A, b"old a");
    c.seed(B, b"old b");
    let before = c.listing();
    let batch = c.batch(RUN, &[(A, b"new a"), (B, b"new b")], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ
        .open(c.collection().join(B))
        .expect("hold the target");
    let report = c.host.commit(&prepared).await.expect("commit");
    assert_eq!(report.phase, JournalPhase::Writing, "{report:?}");
    assert_eq!(report.moved, vec![0], "{report:?}");
    let failure = report.failure.clone().expect("a failure");
    assert!(failure.contains("write 1"), "{failure}");
    assert!(
        !failure.contains("config.json"),
        "no path in the reason: {failure}"
    );
    assert_eq!(c.listing().get(A), Some(&sha256_hex(b"new a")));
    let record = c.record(RUN).expect("record");
    // the restore works while B is still held: B already is its pre-image
    let restored = c.host.restore(&record).await.expect("restore");
    drop(held);
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before, "restore returns the pre-run listing");

    // no sharing at all: refused before the first move
    std::fs::remove_file(c.deploy_dir().join("journal.json")).expect("drop the journal");
    let batch = c.batch(RUN2, &[(A, b"newer a"), (B, b"newer b")], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare 2");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(c.collection().join(B))
        .expect("hold the target exclusively");
    let err = c
        .host
        .commit(&prepared)
        .await
        .expect_err("an unreadable target is a refusal");
    drop(held);
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(err.to_string().contains("holds it open"), "{err}");
    assert_eq!(c.listing(), before, "nothing moved");
    assert!(c.journal().is_none(), "no journal is left behind");
    assert!(!c.snapshot_dir(RUN2).exists());
}

/// A file moved in takes the collection's inherited access list (C7b: a moved
/// file otherwise keeps the protected list it had in the account home). Read
/// with `icacls`: every entry of the moved file is inherited from the directory
/// it was moved into. (A file made by this test process is no control: a
/// process in an ssh session gets explicit entries from its token's default
/// access list, which is not what the gateway's own files have.)
#[cfg(windows)]
#[tokio::test]
async fn ps_moved_file_has_only_inherited_entries() {
    let c = Ctx::ps();
    let batch = c.batch(RUN, &[(FILE_A, b"x")], &[], None);
    let report = c.apply(&batch).await;
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    let path = c.collection().join(FILE_A);
    let out = std::process::Command::new("icacls")
        .arg(&path)
        .output()
        .expect("icacls");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let shown = path.display().to_string();
    let entries: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .map(|l| l.strip_prefix(shown.as_str()).unwrap_or(l).trim())
        .filter(|l| l.contains(":("))
        .collect();
    assert!(!entries.is_empty(), "icacls listed no entry:\n{text}");
    assert!(
        entries.iter().all(|e| e.contains("(I)")),
        "an explicit entry on a moved file: {entries:?}"
    );
}

/// A staged copy that is a second name (a hard link) for the live file: the
/// script does not refuse it as `sh` does (it has no inode to compare), and the
/// move onto it must never lose the file (C7a-4).
#[cfg(windows)]
#[tokio::test]
async fn ps_self_move_through_a_hard_link_never_loses_the_file() {
    let c = Ctx::ps();
    c.seed(FILE_A, b"same bytes");
    let batch = c.batch(RUN, &[(FILE_A, b"same bytes")], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let staged = c.staging_dir(RUN).join("files").join(FILE_A);
    std::fs::remove_file(&staged).expect("remove staged copy");
    std::fs::hard_link(c.collection().join(FILE_A), &staged).expect("hard link");
    let outcome = c.host.commit(&prepared).await;
    eprintln!("a staged hard link of the live file: {outcome:?}");
    assert_eq!(
        std::fs::read(c.collection().join(FILE_A)).expect("the file survives"),
        b"same bytes"
    );
}

/// A directory of the target path that became a junction after the plan was
/// made is not followed: the commit is `stale` and nothing is written through
/// the link.
#[cfg(windows)]
#[tokio::test]
async fn ps_junction_target_dir_is_refused() {
    let c = Ctx::ps();
    let batch = c.batch(RUN, &[(FILE_A, b"a")], &[], None);
    let prepared = c
        .host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let outside = tempfile::tempdir().expect("outside");
    let link = c.collection().join("ignition");
    // mklink reads a `/` as the start of a switch: native separators only
    let native = |p: &Path| p.display().to_string().replace('/', "\\");
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(native(&link))
        .arg(native(outside.path()))
        .output()
        .expect("mklink");
    assert!(made.status.success(), "{made:?}");

    let report = c.host.commit(&prepared).await.expect("commit");
    assert!(report.stale, "{report:?}");
    assert!(report.moved.is_empty(), "{report:?}");
    assert!(c.journal().is_none() && !c.snapshot_dir(RUN).exists());
    assert_eq!(
        std::fs::read_dir(outside.path()).expect("outside").count(),
        0,
        "nothing was written through the link"
    );
    // remove the junction itself, never its target
    std::fs::remove_dir(&link).expect("drop the junction");
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

/// A commit that hangs after its first move is an await point a test can
/// abort. What is left on disk is what a `kill -9` there leaves: the journal
/// at `writing`, one file moved, nothing else changed.
#[tokio::test]
async fn hang_after_move_can_be_aborted() {
    let c = Ctx::local();
    let batch = three_writes(&c);
    let before = c.listing();
    let notify = Arc::new(Notify::new());
    let host = Arc::new(
        c.clean_host()
            .with_apply_fault(ApplyFault::HangAfterMove(1, notify.clone())),
    );
    let prepared = host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let task = tokio::spawn({
        let host = host.clone();
        async move { host.commit(&prepared).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(30), notify.notified())
        .await
        .expect("the commit reached the hang");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());

    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Writing);
    let after = c.listing();
    assert_eq!(after.get(FILE_A), Some(&sha256_hex(b"new a")), "one moved");
    assert_eq!(after.get(FILE_B), Some(&sha256_hex(b"old b")));
    assert_eq!(after.get(FILE_C), None);
    assert!(c.snapshot_dir(RUN).join("record.json").is_file());

    // Recovery from the aborted run is a restore.
    let record = c.record(RUN).expect("record.json");
    let restored = c.clean_host().restore(&record).await.expect("restore");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before);
}

// ---- FaultyHost -------------------------------------------------------------

/// `FaultyHost` fails the named call on its nth invocation without calling the
/// inner host, delegates every other invocation, and records the calls.
#[tokio::test]
async fn faulty_host_fails_the_nth_call() {
    let c = Ctx::local();
    let host = FaultyHost::new(c.clean_host())
        .fail("restore", 1)
        .fail("finalize", 1)
        .fail("commit", 2);
    let batch = three_writes(&c);
    let before = c.listing();

    let prepared = host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    let report = host.commit(&prepared).await.expect("first commit runs");
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    let written = c.listing();
    let record = c.record(RUN).expect("record");

    let err = host
        .restore(&record)
        .await
        .expect_err("the first restore fails");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert_eq!(c.listing(), written, "a failed call never reached the host");
    assert_eq!(c.journal().expect("journal").phase, JournalPhase::Written);

    let err = host
        .finalize(&record)
        .await
        .expect_err("the first finalize fails");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(c.journal().is_some(), "finalize did not run");

    let err = host
        .commit(&prepared)
        .await
        .expect_err("the second commit fails");
    assert_eq!(err.code(), "transport_failed", "{err}");

    let restored = host.restore(&record).await.expect("second restore runs");
    assert!(restored.failure.is_none(), "{restored:?}");
    assert_eq!(c.listing(), before);
    assert_eq!(
        host.calls(),
        vec![
            "prepare", "commit", "restore", "finalize", "commit", "restore"
        ]
    );
}

/// A hung call signals, never calls the inner host, and is an await point a
/// test can abort.
#[tokio::test]
async fn faulty_host_hangs_the_nth_call() {
    let c = Ctx::local();
    let notify = Arc::new(Notify::new());
    let host = FaultyHost::new(c.clean_host()).hang("finalize", 1, notify.clone());
    let batch = three_writes(&c);
    let prepared = host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    host.commit(&prepared).await.expect("commit");
    let mut record = c.record(RUN).expect("record");
    record.state = RecordState::Applied;
    let post = record.post_map();
    record.post_fingerprint = Some(fingerprint(&post));

    let task = tokio::spawn({
        let host = host.clone();
        async move { host.finalize(&record).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(20), notify.notified())
        .await
        .expect("finalize reached the hang");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());

    assert!(c.journal().is_some(), "finalize never ran");
    assert!(c.staging_dir(RUN).is_dir());
    assert_eq!(host.calls(), vec!["prepare", "commit", "finalize"]);
}

/// A hook registered for a call's nth invocation runs after the inner call
/// returned and before the caller sees the result, once, and only for that
/// invocation.
#[tokio::test]
async fn faulty_host_runs_a_hook_after_the_nth_call() {
    let c = Ctx::local();
    let marker = c.root().join("hook-marker");
    let prepares = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let host = FaultyHost::new(c.clean_host())
        .after("commit", 1, {
            let marker = marker.clone();
            move || {
                let marker = marker.clone();
                async move {
                    tokio::fs::write(&marker, b"ran").await.expect("marker");
                }
            }
        })
        .after("prepare", 1, {
            let prepares = prepares.clone();
            move || {
                let prepares = prepares.clone();
                async move {
                    prepares.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });
    let batch = three_writes(&c);

    let prepared = host
        .prepare(&batch.spec, &batch.tree)
        .await
        .expect("prepare");
    assert_eq!(prepares.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!marker.exists(), "the commit hook has not run yet");

    let report = host.commit(&prepared).await.expect("commit");
    assert!(marker.is_file(), "the hook ran before commit returned");
    assert_eq!(report.phase, JournalPhase::Written, "{report:?}");
    assert_eq!(
        report.moved,
        vec![0, 1, 2],
        "the result is the inner host's"
    );

    // A second prepare is invocation 2: the hook does not run again.
    host.prepare(&batch.spec, &batch.tree)
        .await
        .expect("second prepare");
    assert_eq!(prepares.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(host.calls(), vec!["prepare", "commit", "prepare"]);
}

/// Every row also runs against the POSIX script host on unix.
#[cfg(unix)]
#[test]
fn every_row_runs_against_the_sh_host() {
    let source = include_str!("deploy_host_conformance.rs");
    for row in scenarios() {
        let wired = format!("run_sh(\"{}\")", row.name);
        assert!(
            source.contains(&wired),
            "row {} is not run against the sh host",
            row.name
        );
    }
}

/// Every row also runs against the PowerShell script host on windows. The test
/// reads the source, so it checks the wiring on every platform and a row added
/// without a `conformance_ps_` test fails on the machine that adds it.
#[test]
fn every_row_runs_against_the_powershell_host() {
    let source = include_str!("deploy_host_conformance.rs");
    for row in scenarios() {
        let wired = format!("run_ps(\"{}\")", row.name);
        assert!(
            source.contains(&wired),
            "row {} is not run against the PowerShell host",
            row.name
        );
        let test = format!("async fn conformance_ps_{}()", row.name);
        assert!(source.contains(&test), "no test for {}", row.name);
    }
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
