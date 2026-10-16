//! `impl ApplyHost for SftpTransport` for a Linux ssh node (Phase 23, 23-06).
//!
//! Everything under the scan lock is one `ssh` call that runs the uploaded
//! POSIX `sh` script (`scripts/commit.sh`) through a fixed, hash-checking stub
//! (D-01). Files are uploaded with `sftp` before the lock and nothing inside
//! it is a per-file round trip.
//!
//! Layout below the service account's home (Decision B(i)), the same as the
//! local host's below the data directory:
//!
//! ```text
//! <home>/ign-deploy/<gateway>/<mode>/ledger.json | journal.json
//! <home>/ign-deploy/<gateway>/<mode>/snapshots/<run>/...
//! <home>/ign-deploy/<gateway>/<mode>/staging/<run>/   commit.sh, work.tsv,
//!                                                      journal.<phase>.json,
//!                                                      record.applying.json,
//!                                                      ledger.json, files/<path>
//! <home>/.ign-ctl-<nonce>/                             commit.sh, work.tsv and
//!                                                      the files of an
//!                                                      operation that belongs to
//!                                                      no staged run; removed
//!                                                      when the call returns
//! ```
//!
//! The remote command carries only fixed text, the script path, the
//! operation, three absolute roots and two SHA-256 values. Paths, hashes and
//! reasons travel inside `work.tsv`; the script answers with indices into it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::Ordering;

use serde_json::Value;

use super::{Seen, SftpTransport};
use crate::deploy::apply::fingerprint::LEDGER_KEY;
use crate::deploy::apply::host::{
    ApplyHost, BatchReport, BatchSpec, Prepared, ProbeReport, restoring_journal,
};
use crate::deploy::apply::journal::{Journal, JournalPhase};
use crate::deploy::apply::record::{RECORD_MAX_BYTES, RunRecord};
use crate::deploy::apply::run_store::RunId;
use crate::deploy::layout::RelPath;
use crate::deploy::ledger::{
    LEDGER_FILE, LedgerRoot, journal_location, location_of, record_rel, staging_rel,
};
use crate::deploy::transport::batch::{
    POSIX_STUB, SCRIPT_SH, SCRIPT_SH_FILE, SeenJournal, WORK_FILE, WorkList, WorkOp, journal_file,
    record_file, script_sh_sha256,
};
use crate::deploy::transport::remote::{RemoteCommand, SftpOp, safe_remote_path};
use crate::deploy::transport::{
    DeleteSet, MirrorReport, READ_MAX_FILE_BYTES, READ_MAX_TOTAL_BYTES, TreeReader,
    absolute_components, read_staged_copy,
};
use crate::deploy::tree::{RenderedTree, sha256_hex};
use crate::error::CoreError;

/// What the apply half needs: the target gateway and mode.
#[derive(Debug, Clone)]
pub(super) struct ApplySetup {
    pub(super) gateway: String,
    pub(super) mode: String,
}

/// The longest script-supplied text kept in an error.
const SCRIPT_TEXT_MAX: usize = 200;

/// A script-supplied sentence, reduced to printable ASCII and capped. The
/// script prints only fixed words and indices, but the host controls its
/// stdout, so nothing is trusted.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(SCRIPT_TEXT_MAX)
        .collect()
}

/// The first `levels` components of `path`.
fn first_components(path: &str, levels: usize) -> String {
    path.split('/').take(levels).collect::<Vec<_>>().join("/")
}

impl SftpTransport {
    /// Bind the apply half ([`ApplyHost`]) to `gateway` and `mode`. Fails
    /// when either is not a name a run may write under.
    pub fn with_apply(mut self, gateway: &str, mode: &str) -> Result<Self, String> {
        if let Some(why) = component_problem_text(gateway) {
            return Err(format!("the gateway {why}"));
        }
        if let Some(why) = super::mirror_mode_problem(mode) {
            return Err(why);
        }
        self.apply = Some(ApplySetup {
            gateway: gateway.to_string(),
            mode: mode.to_string(),
        });
        Ok(self)
    }

    /// Test seam: take `home` as the account's home instead of asking the
    /// server. With `sftp -D <sftp-server>` the server's working directory is
    /// the test process's, which is not a place a test may write.
    #[doc(hidden)]
    pub fn with_home(self, home: &str) -> Self {
        let _ = self.home.set(home.trim_end_matches('/').to_string());
        self
    }

    fn apply_setup(&self, op: &str) -> Result<&ApplySetup, CoreError> {
        if self.windows {
            return Err(self.mirror_fail(
                op,
                "apply over ssh to a Windows host is not supported by this build yet",
            ));
        }
        self.apply.as_ref().ok_or_else(|| {
            self.mirror_fail(
                op,
                "this transport has no apply half; build it with apply_host_for_node",
            )
        })
    }

    /// The `ign-deploy/<gateway>/<mode>` directory below the home, the mode
    /// collection, both checked for a batch line.
    async fn apply_roots(&self, op: &str) -> Result<(String, String, String), CoreError> {
        let setup = self.apply_setup(op)?;
        let home = self.account_home().await?;
        let deploy = format!("{home}/ign-deploy/{}/{}", setup.gateway, setup.mode);
        let coll = format!("{}/config/resources/{}", self.data_dir, setup.mode);
        for root in [&deploy, &coll] {
            safe_remote_path(root)
                .map_err(|why| self.mirror_fail(op, format!("a root is unsafe: {why}")))?;
        }
        Ok((home, deploy, coll))
    }

    fn staging_abs(&self, home: &str, run: &str) -> Result<String, CoreError> {
        let setup = self.apply_setup("prepare")?;
        let root = format!("{home}/{}", staging_rel(&setup.gateway, &setup.mode, run));
        safe_remote_path(&root).map_err(|why| {
            self.mirror_fail("prepare", format!("the staging root is unsafe: {why}"))
        })?;
        Ok(root)
    }

    /// Upload `entries` (a path below `root` and its size) from `local` into
    /// the staging directory `root`, creating the directories that are
    /// missing. Returns `false`, having written nothing, when `root` already
    /// exists and `must_be_new` is set.
    async fn put_tree(
        &self,
        op: &str,
        root: &str,
        local: &Path,
        entries: &[(String, u64)],
        must_be_new: bool,
    ) -> Result<bool, CoreError> {
        let parts = absolute_components(root).map_err(|why| self.mirror_fail(op, why))?;
        let base = format!("/{}", parts[0]);
        let rest: Vec<String> = parts[1..].to_vec();
        let mut targets: Vec<Vec<String>> = Vec::new();
        let mut file_chains: Vec<Vec<String>> = Vec::new();
        for (rel, _) in entries {
            let mut chain = rest.clone();
            chain.extend(rel.split('/').map(str::to_string));
            file_chains.push(chain.clone());
            targets.push(chain);
        }
        targets.push(rest.clone());
        // the root is below the account home, whose listing may hold names a
        // batch line cannot carry; only the fixed names of the root are
        // looked up
        let mut budget = super::Budget::lenient();
        let seen = self.survey(op, &base, &targets, &mut budget).await?;
        if must_be_new && seen.get(&rest) == Some(&Seen::Dir) {
            return Ok(false);
        }
        for (chain, (rel, _)) in file_chains.iter().zip(entries) {
            if matches!(seen.get(chain), Some(Seen::Dir)) {
                return Err(self.mirror_fail(
                    op,
                    format!("{rel} exists in the staging root and is a directory"),
                ));
            }
        }
        if matches!(seen.get(&rest), Some(Seen::File(_))) {
            return Err(self.mirror_fail(
                op,
                format!("the staging root {root:?} is a file, not a directory"),
            ));
        }
        let mut missing = Self::dirs_to_make(&seen, &file_chains);
        for len in 1..=rest.len() {
            let chain = rest[..len].to_vec();
            if seen.get(&chain) == Some(&Seen::Missing) {
                missing.insert(chain);
            }
        }
        self.make_dirs(op, &base, &missing).await?;
        let puts: Vec<(SftpOp, u64)> = entries
            .iter()
            .map(|(rel, size)| {
                (
                    SftpOp::Put {
                        local: rel
                            .split('/')
                            .fold(local.to_path_buf(), |path, part| path.join(part)),
                        remote: format!("{root}/{rel}"),
                    },
                    *size,
                )
            })
            .collect();
        self.run_chunked(op, puts).await?;
        Ok(true)
    }

    /// The remote command that runs the staged script's `op`.
    fn script_command(
        &self,
        op: &str,
        stage: &str,
        coll: &str,
        deploy: &str,
        work_sha: &str,
    ) -> Result<RemoteCommand, CoreError> {
        let script = format!("{stage}/{SCRIPT_SH_FILE}");
        let script_sha = script_sh_sha256();
        RemoteCommand::posix(&[
            "sh",
            "-c",
            POSIX_STUB,
            "ign-stub",
            &script,
            op,
            stage,
            coll,
            deploy,
            &script_sha,
            work_sha,
        ])
        .map_err(|why| self.mirror_fail(op, why))
    }

    /// Run `op` from the staging directory `stage`, which holds the script and
    /// the work list hashing to `work_sha`.
    async fn exec_script(
        &self,
        op: &str,
        stage: &str,
        coll: &str,
        deploy: &str,
        work_sha: &str,
    ) -> Result<Value, CoreError> {
        let command = self.script_command(op, stage, coll, deploy, work_sha)?;
        self.shell.exec_posix(op, &command).await
    }

    /// A fresh directory name below the home for one operation's files.
    fn ctl_name(&self) -> String {
        let seq = self.ctl_seq.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!(".ign-ctl-{:x}-{seq:x}-{nanos:x}", std::process::id())
    }

    /// Run `op` for `work` from a throw-away directory below the home: upload
    /// the script, the work list and `extra` files (a name may hold `/`), run,
    /// remove them. The directory is removed whether or not the run succeeded.
    async fn run_ctl(
        &self,
        op: &str,
        work: &WorkList,
        extra: &[(String, Vec<u8>)],
        coll: &str,
        deploy: &str,
    ) -> Result<Value, CoreError> {
        let home = self.account_home().await?;
        let ctl = format!("{home}/{}", self.ctl_name());
        self.run_ctl_in(op, &ctl, work, extra, coll, deploy).await
    }

    /// [`run_ctl`](Self::run_ctl) in the directory `ctl`, which the work list
    /// may already name (a mirror's `S` record).
    async fn run_ctl_in(
        &self,
        op: &str,
        ctl: &str,
        work: &WorkList,
        extra: &[(String, Vec<u8>)],
        coll: &str,
        deploy: &str,
    ) -> Result<Value, CoreError> {
        safe_remote_path(ctl).map_err(|why| {
            self.mirror_fail(op, format!("the control directory is unsafe: {why}"))
        })?;
        let lines = work.to_lines();
        let work_sha = sha256_hex(&lines);
        let mut files: Vec<(String, Vec<u8>)> = vec![
            (SCRIPT_SH_FILE.to_string(), SCRIPT_SH.as_bytes().to_vec()),
            (WORK_FILE.to_string(), lines),
        ];
        files.extend(extra.iter().cloned());
        let temp = tempfile::Builder::new()
            .prefix("ign-ctl-")
            .tempdir()
            .map_err(|e| {
                self.mirror_fail(op, format!("cannot create a temp dir ({:?})", e.kind()))
            })?;
        let local = |name: &str| {
            name.split('/')
                .fold(temp.path().to_path_buf(), |path, part| path.join(part))
        };
        let mut dirs: std::collections::BTreeSet<(usize, String)> =
            std::collections::BTreeSet::new();
        for (name, bytes) in &files {
            let target = local(name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    self.mirror_fail(op, format!("cannot stage {name} ({:?})", e.kind()))
                })?;
            }
            std::fs::write(&target, bytes).map_err(|e| {
                self.mirror_fail(op, format!("cannot stage {name} ({:?})", e.kind()))
            })?;
            let parts: Vec<&str> = name.split('/').collect();
            for len in 1..parts.len() {
                dirs.insert((len, parts[..len].join("/")));
            }
        }
        let mut upload = vec![SftpOp::Mkdir {
            path: ctl.to_string(),
        }];
        for (_, dir) in &dirs {
            upload.push(SftpOp::Mkdir {
                path: format!("{ctl}/{dir}"),
            });
        }
        for (name, _) in &files {
            upload.push(SftpOp::Put {
                local: local(name),
                remote: format!("{ctl}/{name}"),
            });
        }
        let ran = async {
            self.run_batch(op, upload).await?;
            self.exec_script(op, ctl, coll, deploy, &work_sha).await
        }
        .await;
        if ran.is_err() {
            // The script removes its own control directory when it exits. When
            // it never ran (a refused upload, a stub that refused, a lost
            // connection) every file is still where it was put, so they are
            // removed here, best effort: a leftover control directory is
            // harmless and the run's own error is what the caller needs.
            let mut cleanup: Vec<SftpOp> = files
                .iter()
                .map(|(name, _)| SftpOp::Rm {
                    path: format!("{ctl}/{name}"),
                })
                .collect();
            for (_, dir) in dirs.iter().rev() {
                cleanup.push(SftpOp::Rmdir {
                    path: format!("{ctl}/{dir}"),
                });
            }
            cleanup.push(SftpOp::Rmdir {
                path: ctl.to_string(),
            });
            let _ = self.run_batch(op, cleanup).await;
        }
        ran
    }

    /// `ok` of a script result: `Some(true)` for 1, `Some(false)` for 0.
    fn result_ok(&self, op: &str, value: &Value) -> Result<bool, CoreError> {
        match value.get("ok").and_then(Value::as_i64) {
            Some(1) => Ok(true),
            Some(0) => Ok(false),
            _ => Err(self.mirror_fail(op, "the script result is malformed (ok)")),
        }
    }

    /// The error a refusing script result (`ok` 0) stands for.
    async fn refusal(&self, op: &str, value: &Value, work: Option<&WorkList>) -> CoreError {
        let code = value.get("err").and_then(Value::as_str).unwrap_or("");
        let detail = plain(value.get("detail").and_then(Value::as_str).unwrap_or(""));
        match code {
            "journal_present" => self.journal_present().await,
            "staged" => {
                let bad: Vec<String> = value
                    .get("bad")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|item| staged_name(item, work))
                            .collect()
                    })
                    .unwrap_or_default();
                self.mirror_fail(
                    op,
                    format!(
                        "the staged copy on the host does not match its SHA-256 for {}; nothing was changed",
                        super::named(&bad)
                    ),
                )
            }
            _ => self.mirror_fail(op, format!("the host refused: {detail} ({})", plain(code))),
        }
    }

    /// `deploy_journal_present`, naming the run that holds the journal.
    async fn journal_present(&self) -> CoreError {
        let (gateway, mode) = match &self.apply {
            Some(setup) => (setup.gateway.clone(), setup.mode.clone()),
            None => (String::new(), String::new()),
        };
        let location = journal_location(LedgerRoot::AccountHome, &gateway, &mode);
        let run_id = match self.read_journal(&gateway, &mode).await {
            Ok(Some(bytes)) => Journal::parse(&location, &bytes, &gateway, &mode)
                .map(|j| j.run_id)
                .unwrap_or_else(|_| "unknown".to_string()),
            _ => "unknown".to_string(),
        };
        CoreError::DeployJournalPresent {
            run_id,
            env: "<env>".to_string(),
            location,
        }
    }

    /// The journal in place, as the work list names it: absent, or the hash of
    /// the one that belongs to `run_id`. A journal of another run is
    /// `deploy_journal_present`; it is never overwritten or removed.
    async fn journal_of_run(&self, run_id: &str) -> Result<SeenJournal, CoreError> {
        let Some(setup) = &self.apply else {
            return Err(self.mirror_fail("journal", "this transport has no apply half"));
        };
        let location = journal_location(LedgerRoot::AccountHome, &setup.gateway, &setup.mode);
        match self.read_journal(&setup.gateway, &setup.mode).await? {
            None => Ok(SeenJournal::Absent),
            Some(bytes) => {
                let theirs = Journal::parse(&location, &bytes, &setup.gateway, &setup.mode)?;
                if theirs.run_id != run_id {
                    return Err(CoreError::DeployJournalPresent {
                        run_id: theirs.run_id,
                        env: "<env>".to_string(),
                        location,
                    });
                }
                Ok(SeenJournal::Hash(sha256_hex(&bytes)))
            }
        }
    }

    /// The report of a commit or restore result; its indices point into
    /// `work`.
    fn batch_report(
        &self,
        op: &str,
        value: &Value,
        work: &WorkList,
    ) -> Result<BatchReport, CoreError> {
        let restoring = work.op == WorkOp::Restore;
        let malformed =
            |what: &str| self.mirror_fail(op, format!("the script result is malformed ({what})"));
        let phase = match value.get("phase").and_then(Value::as_str) {
            Some("snapshotting") => JournalPhase::Snapshotting,
            Some("ready") => JournalPhase::Ready,
            Some("writing") => JournalPhase::Writing,
            Some("written") => JournalPhase::Written,
            Some("restoring") => JournalPhase::Restoring,
            _ => return Err(malformed("phase")),
        };
        let (moved_limit, deleted_limit) = if restoring {
            (work.restores.len(), work.restores.len())
        } else {
            (work.writes.len(), work.deletes.len())
        };
        let indices = |key: &str, limit: usize| -> Result<Vec<usize>, CoreError> {
            let items = value
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| malformed(key))?;
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let n = item
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n < limit)
                    .ok_or_else(|| malformed(key))?;
                out.push(n);
            }
            Ok(out)
        };
        let moved = indices("moved", moved_limit)?;
        let deleted = indices("deleted", deleted_limit)?;
        let mut removed = std::collections::BTreeSet::new();
        for pair in value
            .get("dirs")
            .and_then(Value::as_array)
            .ok_or_else(|| malformed("dirs"))?
        {
            let pair = pair
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or_else(|| malformed("dirs"))?;
            let index = pair[0]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n < deleted_limit)
                .ok_or_else(|| malformed("dirs"))?;
            let path = if restoring {
                work.restores[index].path.as_str()
            } else {
                work.deletes[index].path.as_str()
            };
            let levels = pair[1]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n >= crate::deploy::transport::MIRROR_MIN_COMPONENTS)
                .filter(|n| *n < path.split('/').count())
                .ok_or_else(|| malformed("dirs"))?;
            removed.insert(first_components(path, levels));
        }
        let mut stale_paths: Vec<String> = Vec::new();
        for item in value
            .get("stale")
            .and_then(Value::as_array)
            .ok_or_else(|| malformed("stale"))?
        {
            let item = item.as_str().ok_or_else(|| malformed("stale"))?;
            stale_paths.push(stale_path(item, work).ok_or_else(|| malformed("stale"))?);
        }
        let failure = match value.get("fail") {
            Some(Value::Null) | None => None,
            Some(Value::String(text)) => Some(plain(text)),
            Some(_) => return Err(malformed("fail")),
        };
        let mut report = BatchReport {
            phase,
            stale: false,
            moved,
            deleted,
            removed_dirs: removed.into_iter().collect(),
            failure,
        };
        if !stale_paths.is_empty() {
            stale_paths.sort();
            let named: Vec<&str> = stale_paths.iter().take(10).map(String::as_str).collect();
            let more = stale_paths.len().saturating_sub(10);
            report.stale = true;
            report.failure = Some(format!(
                "{} path(s) are not what the plan was made against: {}{}",
                stale_paths.len(),
                named.join(", "),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            ));
        }
        Ok(report)
    }
}

impl SftpTransport {
    /// `TreeMirror::commit` of a Linux node (D-01): the staged copies, the
    /// delete set and the collection go into one work list and the uploaded
    /// script's `mirror` op does every check, move, delete and emptied-directory
    /// removal in ONE call. `Separate` moves the copies staged at `remote_root`;
    /// `InPlace` has no copy on the host, so the local staged copies are
    /// uploaded with the script into a control directory and moved from there.
    pub(super) async fn mirror_commit_script(
        &self,
        local: &Path,
        files: &[(RelPath, String)],
        remote_root: Option<&str>,
        owner: &str,
        delete_set: &DeleteSet,
    ) -> Result<MirrorReport, CoreError> {
        const OP: &str = "commit";
        let (setup, _sizes) = self
            .mirror_precheck(local, files, remote_root, owner, delete_set)
            .await?;
        let mode = setup.mode.clone();
        let coll = format!("{}/config/resources/{mode}", self.data_dir);
        safe_remote_path(&coll)
            .map_err(|why| self.mirror_fail(OP, format!("the collection is unsafe: {why}")))?;
        let home = self.account_home().await?;
        let mut deletes: Vec<RelPath> = delete_set.files.to_vec();
        deletes.sort();

        let ctl = format!("{home}/{}", self.ctl_name());
        let (source, extra): (String, Vec<(String, Vec<u8>)>) = match remote_root {
            Some(root) => (root.to_string(), Vec::new()),
            None => {
                let (base, copies) = (local.to_path_buf(), files.to_vec());
                let read = tokio::task::spawn_blocking(move || {
                    let mut out = Vec::with_capacity(copies.len());
                    for (path, sha) in &copies {
                        out.push((format!("files/{path}"), read_staged_copy(&base, path, sha)?));
                    }
                    Ok::<_, String>(out)
                })
                .await
                .map_err(|e| CoreError::Internal(format!("sftp commit task failed: {e}")))?
                .map_err(|why| self.mirror_fail(OP, why))?;
                (format!("{ctl}/files"), read)
            }
        };
        let work = WorkList::for_mirror(&mode, &source, files, &deletes)
            .map_err(|why| self.mirror_fail(OP, why))?;
        // the script's operation is `mirror`; every error of this commit is
        // still named `commit`
        let value = self
            .run_ctl_in("mirror", &ctl, &work, &extra, &coll, &home)
            .await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.mirror_refusal(&value, &work, &mode, remote_root));
        }
        self.mirror_report(&value, &work, &mode)
    }

    /// The path an `at` (`W3`, `X1`, `R`, `C`) of a mirror refusal names.
    fn mirror_at(work: &WorkList, mode: &str, at: &str) -> String {
        match at {
            "R" => "config/resources".to_string(),
            "C" => format!("the {mode} collection"),
            _ => {
                let (kind, number) =
                    at.split_at(at.find(|c: char| c.is_ascii_digit()).unwrap_or(at.len()));
                let n: Option<usize> = number.parse().ok();
                match (kind, n) {
                    ("W", Some(n)) => work.writes.get(n).map(|w| w.path.as_str().to_string()),
                    ("X", Some(n)) => work.mirror_deletes.get(n).map(|p| p.as_str().to_string()),
                    _ => None,
                }
                .unwrap_or_else(|| "a path".to_string())
            }
        }
    }

    fn mirror_refusal(
        &self,
        value: &Value,
        work: &WorkList,
        mode: &str,
        root: Option<&str>,
    ) -> CoreError {
        const OP: &str = "commit";
        let code = value.get("err").and_then(Value::as_str).unwrap_or("");
        let at = value.get("at").and_then(Value::as_str).unwrap_or("");
        let shown = Self::mirror_at(work, mode, at);
        let detail = match code {
            "staged" => {
                let bad: Vec<String> = value
                    .get("bad")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|item| Self::mirror_at(work, mode, item))
                            .collect()
                    })
                    .unwrap_or_default();
                let removed = value.get("removed").and_then(Value::as_u64).unwrap_or(0);
                let place = root.unwrap_or("the upload directory");
                format!(
                    "the staged copy on the host is missing or does not match its SHA-256 for {}; nothing was changed. {removed} staged copies were removed from {place} (empty directories remain).",
                    super::named(&bad)
                )
            }
            "symlink" => format!(
                "{shown} is or is below a symlink; symlinks are refused; nothing was changed"
            ),
            "not_regular" => format!(
                "{shown} is not a regular file, or a file stands where a directory must be; nothing was changed"
            ),
            "not_file" => format!(
                "{shown} is not a regular file in the {mode} collection; nothing was changed"
            ),
            "is_dir" => {
                format!("{shown} is a directory in the {mode} collection; nothing was changed")
            }
            "resources" => {
                "config/resources is not a directory; is data_dir a gateway data directory?"
                    .to_string()
            }
            "coll_not_dir" => format!("the {mode} collection is not a directory"),
            _ => format!(
                "the host refused: {} ({})",
                plain(value.get("detail").and_then(Value::as_str).unwrap_or("")),
                plain(code)
            ),
        };
        self.mirror_fail(OP, detail)
    }

    /// The report of a mirror result, or the error of a commit that stopped
    /// part-way.
    fn mirror_report(
        &self,
        value: &Value,
        work: &WorkList,
        mode: &str,
    ) -> Result<MirrorReport, CoreError> {
        const OP: &str = "commit";
        let malformed =
            |what: &str| self.mirror_fail(OP, format!("the script result is malformed ({what})"));
        let indices = |key: &str, limit: usize| -> Result<Vec<usize>, CoreError> {
            value
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| malformed(key))?
                .iter()
                .map(|item| {
                    item.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|n| *n < limit)
                        .ok_or_else(|| malformed(key))
                })
                .collect()
        };
        let moved = indices("moved", work.writes.len())?;
        let deleted = indices("deleted", work.mirror_deletes.len())?;
        if let Some(fail) = value.get("fail").filter(|v| !v.is_null()) {
            let reason = plain(fail.as_str().ok_or_else(|| malformed("fail"))?);
            let stopped = reason
                .strip_prefix("write ")
                .and_then(|r| r.split_once(':'))
                .and_then(|(n, _)| n.parse::<usize>().ok())
                .and_then(|n| work.writes.get(n).map(|w| w.path.as_str().to_string()))
                .or_else(|| {
                    reason
                        .strip_prefix("delete ")
                        .and_then(|r| r.split_once(':'))
                        .and_then(|(n, _)| n.parse::<usize>().ok())
                        .and_then(|n| work.mirror_deletes.get(n).map(|p| p.as_str().to_string()))
                })
                .unwrap_or_else(|| "a path".to_string());
            let detail = if reason.contains("does not match its hash") {
                format!(
                    "content changed on the host during commit: the {mode} collection holds content that does not match the staged SHA-256 for {stopped}; the {} written files were not rolled back and no delete was run",
                    moved.len()
                )
            } else {
                format!(
                    "the commit stopped at {stopped}: {reason}; {} of {} files were written and {} deleted, and nothing was rolled back",
                    moved.len(),
                    work.writes.len(),
                    deleted.len()
                )
            };
            return Err(self.mirror_fail(OP, detail));
        }
        let mut removed = std::collections::BTreeSet::new();
        for pair in value
            .get("dirs")
            .and_then(Value::as_array)
            .ok_or_else(|| malformed("dirs"))?
        {
            let pair = pair
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or_else(|| malformed("dirs"))?;
            let index = pair[0]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n < work.mirror_deletes.len())
                .ok_or_else(|| malformed("dirs"))?;
            let path = work.mirror_deletes[index].as_str();
            let levels = pair[1]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n >= crate::deploy::transport::MIRROR_MIN_COMPONENTS)
                .filter(|n| *n < path.split('/').count())
                .ok_or_else(|| malformed("dirs"))?;
            removed.insert(first_components(path, levels));
        }
        Ok(MirrorReport {
            written: moved
                .iter()
                .map(|i| work.writes[*i].path.as_str().to_string())
                .collect(),
            deleted: deleted
                .iter()
                .map(|i| work.mirror_deletes[*i].as_str().to_string())
                .collect(),
            removed_dirs: removed.into_iter().collect(),
        })
    }
}

/// `jiff`-free helper: the text of a `component_problem`.
fn component_problem_text(name: &str) -> Option<&'static str> {
    crate::deploy::component_problem(name)
}

/// The path a stale or bad index names, as `W3`, `D1`, `L`.
fn stale_path(item: &str, work: &WorkList) -> Option<String> {
    if item == "L" {
        return Some(LEDGER_KEY.to_string());
    }
    let (kind, number) = item.split_at(item.find(|c: char| c.is_ascii_digit())?);
    let n: usize = number.parse().ok()?;
    match kind {
        "W" => work.writes.get(n).map(|w| w.path.as_str().to_string()),
        "D" => work.deletes.get(n).map(|d| d.path.as_str().to_string()),
        _ => None,
    }
}

/// The name of a staged entry a script reported, for an error.
fn staged_name(item: &str, work: Option<&WorkList>) -> String {
    if let Some(file) = item.strip_prefix('F') {
        return plain(file);
    }
    work.and_then(|w| stale_path(item, w))
        .unwrap_or_else(|| "a staged file".to_string())
}

/// What `probe` says for each problem word the script reports.
fn probe_problem(code: &str) -> String {
    match code {
        "coll_symlink" => "the mode collection is a symlink",
        "coll_unwritable" => "the mode collection is not writable",
        "coll_not_dir" => "the mode collection is not a directory",
        "resources_unwritable" => {
            "config/resources is not writable, so the mode collection cannot be created"
        }
        "resources_missing" => {
            "config/resources does not exist; is data_dir a gateway data directory?"
        }
        "deploy_symlink" => "the ign-deploy directory is or is below a symlink",
        "deploy_not_dir" => "a level of the ign-deploy directory is not a directory",
        "deploy_uncreatable" => "the ign-deploy directory cannot be created or opened",
        "deploy_unwritable" => "the ign-deploy directory is not writable",
        _ => "the host reported a problem this build does not know",
    }
    .to_string()
}

impl SftpTransport {
    /// The path an index of a fingerprint list names.
    fn fingerprint_hashes(
        &self,
        value: &Value,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<String>>, CoreError> {
        const OP: &str = "fingerprint";
        let items = value
            .get("hashes")
            .and_then(Value::as_array)
            .filter(|items| items.len() == paths.len())
            .ok_or_else(|| self.mirror_fail(OP, "the script result is malformed (hashes)"))?;
        let mut out = BTreeMap::new();
        for (path, item) in paths.iter().zip(items) {
            let text = item
                .as_str()
                .ok_or_else(|| self.mirror_fail(OP, "the script result is malformed (hash)"))?;
            let hash = if text == "-" {
                None
            } else if text.len() == 64
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                Some(text.to_string())
            } else {
                return Err(self.mirror_fail(OP, "the script result is malformed (hash)"));
            };
            out.insert(path.clone(), hash);
        }
        Ok(out)
    }

    /// Download the regular files below the host's directory `chain` (below
    /// the account home) into `out`, level by level, and hash each one here.
    /// Symlinks and special files are refused; the caps of the read side apply.
    async fn download_snapshot(
        &self,
        op: &str,
        remote: &str,
        out: &Path,
    ) -> Result<BTreeMap<String, String>, CoreError> {
        let mut hashes = BTreeMap::new();
        let mut files: Vec<(String, u64)> = Vec::new();
        let mut frontier: Vec<String> = vec![String::new()];
        let mut budget = super::Budget::default();
        let mut total: u64 = 0;
        while !frontier.is_empty() {
            let dirs: Vec<String> = frontier
                .iter()
                .map(|rel| {
                    if rel.is_empty() {
                        remote.to_string()
                    } else {
                        format!("{remote}/{rel}")
                    }
                })
                .collect();
            let listings = self.list_level_as(op, &dirs, &mut budget).await?;
            let mut next = Vec::new();
            for (rel, entries) in frontier.iter().zip(listings) {
                for entry in entries {
                    if let Some(why) = crate::deploy::component_problem(&entry.name) {
                        return Err(self.mirror_fail(op, format!("a snapshot entry name {why}")));
                    }
                    let child = if rel.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{rel}/{}", entry.name)
                    };
                    match entry.kind {
                        super::EntryKind::Dir => {
                            std::fs::create_dir(
                                child
                                    .split('/')
                                    .fold(out.to_path_buf(), |path, part| path.join(part)),
                            )
                            .map_err(|e| {
                                self.mirror_fail(
                                    op,
                                    format!("cannot create {child} ({})", e.kind()),
                                )
                            })?;
                            next.push(child);
                        }
                        super::EntryKind::File => {
                            if entry.size > READ_MAX_FILE_BYTES {
                                return Err(self.mirror_fail(
                                    op,
                                    format!("{child} in the snapshot is over the file cap"),
                                ));
                            }
                            total = total.saturating_add(entry.size);
                            if total > READ_MAX_TOTAL_BYTES {
                                return Err(
                                    self.mirror_fail(op, "the snapshot exceeds the total size cap")
                                );
                            }
                            files.push((child, entry.size));
                        }
                        _ => {
                            return Err(self.mirror_fail(
                                op,
                                format!(
                                    "{child} in the snapshot is not a regular file or directory"
                                ),
                            ));
                        }
                    }
                }
            }
            frontier = next;
        }
        let mut next = 0usize;
        let mut downloaded = 0u64;
        while next < files.len() {
            let mut ops = Vec::new();
            let mut chunk = 0u64;
            let start = next;
            while next < files.len() && ops.len() < super::DOWNLOAD_CHUNK_FILES {
                let (rel, size) = &files[next];
                if !ops.is_empty() && chunk + size > super::DOWNLOAD_CHUNK_BYTES {
                    break;
                }
                ops.push(SftpOp::Get {
                    remote: format!("{remote}/{rel}"),
                    local: rel
                        .split('/')
                        .fold(out.to_path_buf(), |path, part| path.join(part)),
                });
                chunk += size;
                next += 1;
            }
            let limit = downloaded + chunk + super::DOWNLOAD_SLACK_BYTES;
            self.run_download(op, &ops, out, limit).await?;
            let root = out.to_path_buf();
            let wanted: Vec<(String, u64)> = files[start..next].to_vec();
            let got =
                tokio::task::spawn_blocking(move || -> Result<Vec<(String, String)>, String> {
                    let mut got = Vec::new();
                    for (rel, size) in &wanted {
                        let bytes = super::read_downloaded(
                            &root,
                            Path::new(rel),
                            *size,
                            READ_MAX_FILE_BYTES,
                        )
                        .map_err(|_| format!("{rel} could not be read back as listed"))?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            std::fs::set_permissions(
                                root.join(rel),
                                std::fs::Permissions::from_mode(0o600),
                            )
                            .map_err(|e| format!("cannot restrict {rel} ({})", e.kind()))?;
                        }
                        got.push((rel.clone(), sha256_hex(&bytes)));
                    }
                    Ok(got)
                })
                .await
                .map_err(|e| CoreError::Internal(format!("sftp snapshot task failed: {e}")))?
                .map_err(|why| self.mirror_fail(op, why))?;
            hashes.extend(got);
            downloaded = tokio::task::spawn_blocking({
                let root = out.to_path_buf();
                move || super::dir_bytes(&root)
            })
            .await
            .map_err(|e| CoreError::Internal(format!("sftp snapshot task failed: {e}")))?;
        }
        Ok(hashes)
    }
}

#[async_trait::async_trait]
impl ApplyHost for SftpTransport {
    async fn probe(&self) -> Result<ProbeReport, CoreError> {
        const OP: &str = "probe";
        let setup = self.apply_setup(OP)?;
        let work = WorkList::for_probe(&setup.gateway, &setup.mode)
            .map_err(|why| self.mirror_fail(OP, why))?;
        let (_, deploy, coll) = self.apply_roots(OP).await?;
        let value = self.run_ctl(OP, &work, &[], &coll, &deploy).await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.refusal(OP, &value, None).await);
        }
        let problems: Vec<String> = value
            .get("problems")
            .and_then(Value::as_array)
            .ok_or_else(|| self.mirror_fail(OP, "the script result is malformed (problems)"))?
            .iter()
            .map(|item| item.as_str().map(probe_problem))
            .collect::<Option<_>>()
            .ok_or_else(|| self.mirror_fail(OP, "the script result is malformed (problems)"))?;
        Ok(ProbeReport {
            writable: problems.is_empty(),
            problems,
        })
    }

    async fn prepare(&self, spec: &BatchSpec, tree: &RenderedTree) -> Result<Prepared, CoreError> {
        const OP: &str = "prepare";
        let setup = self.apply_setup(OP)?;
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
            let Some(bytes) = tree.files.get(&op.path) else {
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
        let (home, deploy, coll) = self.apply_roots(OP).await?;
        let staging = self.staging_abs(&home, spec.run_id.as_str())?;
        let lines = work.to_lines();
        let work_sha = sha256_hex(&lines);

        // everything the staging directory holds, built locally first
        let mut files: Vec<(String, Vec<u8>)> = vec![
            (SCRIPT_SH_FILE.to_string(), SCRIPT_SH.as_bytes().to_vec()),
            (WORK_FILE.to_string(), lines),
        ];
        for phase in crate::deploy::apply::host::PRERENDERED_PHASES {
            files.push((journal_file(phase), spec.journal_at(phase).to_bytes()));
        }
        files.push((
            record_file(crate::deploy::apply::record::RecordState::Applying),
            spec.record.to_bytes(),
        ));
        if let Some(ledger) = &spec.ledger_new {
            files.push((LEDGER_FILE.to_string(), ledger.clone()));
        }
        for op in &spec.writes {
            files.push((format!("files/{}", op.path), tree.files[&op.path].clone()));
        }
        let temp = tempfile::Builder::new()
            .prefix("ign-apply-")
            .tempdir()
            .map_err(|e| {
                self.mirror_fail(OP, format!("cannot create a temp dir ({:?})", e.kind()))
            })?;
        let mut entries: Vec<(String, u64)> = Vec::with_capacity(files.len());
        for (rel, bytes) in &files {
            let target = rel
                .split('/')
                .fold(temp.path().to_path_buf(), |path, part| path.join(part));
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    self.mirror_fail(OP, format!("cannot stage {rel} ({:?})", e.kind()))
                })?;
            }
            std::fs::write(&target, bytes).map_err(|e| {
                self.mirror_fail(OP, format!("cannot stage {rel} ({:?})", e.kind()))
            })?;
            entries.push((rel.clone(), bytes.len() as u64));
        }

        // A stale staging directory of this run (an earlier attempt) is
        // replaced.
        if !self
            .put_tree(OP, &staging, temp.path(), &entries, true)
            .await?
        {
            let unstage = WorkList::for_unstage(spec.run_id.as_str(), &spec.gateway, &spec.mode)
                .map_err(|why| self.mirror_fail(OP, why))?;
            let value = self
                .run_ctl("unstage", &unstage, &[], &coll, &deploy)
                .await?;
            if !self.result_ok(OP, &value)? {
                return Err(self.refusal(OP, &value, None).await);
            }
            if !self
                .put_tree(OP, &staging, temp.path(), &entries, true)
                .await?
            {
                return Err(self.mirror_fail(OP, "a stale staging directory could not be removed"));
            }
        }

        // The upload is checked on the host before the lock is taken.
        let value = self
            .exec_script("verify", &staging, &coll, &deploy, &work_sha)
            .await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.refusal(OP, &value, Some(&work)).await);
        }
        self.lists
            .lock()
            .map_err(|_| self.mirror_fail(OP, "the prepared batches are unavailable"))?
            .insert(work_sha.clone(), work);
        Ok(Prepared::new(spec.run_id.clone(), staging, work_sha))
    }

    async fn commit(&self, prepared: &Prepared) -> Result<BatchReport, CoreError> {
        const OP: &str = "commit";
        self.apply_setup(OP)?;
        let (home, deploy, coll) = self.apply_roots(OP).await?;
        let staging = self.staging_abs(&home, prepared.run_id().as_str())?;
        if prepared.staging() != staging {
            return Err(self.mirror_fail(OP, "the prepared batch was staged for a different host"));
        }
        let work = self
            .lists
            .lock()
            .map_err(|_| self.mirror_fail(OP, "the prepared batches are unavailable"))?
            .get(prepared.work_sha256())
            .cloned()
            .ok_or_else(|| {
                self.mirror_fail(
                    OP,
                    "the prepared batch was staged by a different host handle",
                )
            })?;
        let value = self
            .exec_script(OP, &staging, &coll, &deploy, prepared.work_sha256())
            .await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.refusal(OP, &value, Some(&work)).await);
        }
        self.batch_report(OP, &value, &work)
    }

    async fn restore(&self, record: &RunRecord) -> Result<BatchReport, CoreError> {
        const OP: &str = "restore";
        let setup = self.apply_setup(OP)?;
        if record.gateway != setup.gateway || record.mode != setup.mode {
            return Err(self.mirror_fail(
                OP,
                "the record names a different gateway or mode than this host",
            ));
        }
        let mut work = WorkList::for_restore(record).map_err(|why| self.mirror_fail(OP, why))?;
        let (_, deploy, coll) = self.apply_roots(OP).await?;
        work.journal_seen = Some(self.journal_of_run(&record.run_id).await?);
        let journal = restoring_journal(record).to_bytes();
        let value = self
            .run_ctl(
                OP,
                &work,
                &[(journal_file(JournalPhase::Restoring), journal)],
                &coll,
                &deploy,
            )
            .await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.refusal(OP, &value, Some(&work)).await);
        }
        self.batch_report(OP, &value, &work)
    }

    async fn fingerprint(
        &self,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<String>>, CoreError> {
        const OP: &str = "fingerprint";
        let setup = self.apply_setup(OP)?;
        let work = WorkList::for_fingerprint(&setup.gateway, &setup.mode, paths)
            .map_err(|why| self.mirror_fail(OP, why))?;
        let (_, deploy, coll) = self.apply_roots(OP).await?;
        let value = self.run_ctl(OP, &work, &[], &coll, &deploy).await?;
        if !self.result_ok(OP, &value)? {
            if value.get("err").and_then(Value::as_str) == Some("not_regular") {
                let at = value
                    .get("at")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .and_then(|n| work.restores.get(n));
                return Err(match at {
                    Some(entry) if entry.path == LEDGER_KEY => {
                        self.mirror_fail(OP, "the ledger is not a regular file")
                    }
                    Some(entry) => {
                        self.mirror_fail(OP, format!("{} is not a regular file", entry.path))
                    }
                    None => self.mirror_fail(OP, "a path is not a regular file"),
                });
            }
            return Err(self.refusal(OP, &value, Some(&work)).await);
        }
        self.fingerprint_hashes(&value, paths)
    }

    async fn finalize(&self, record: &RunRecord) -> Result<(), CoreError> {
        const OP: &str = "finalize";
        let setup = self.apply_setup(OP)?;
        if record.gateway != setup.gateway || record.mode != setup.mode {
            return Err(self.mirror_fail(
                OP,
                "the record names a different gateway or mode than this host",
            ));
        }
        RunId::parse(&record.run_id)?;
        RunRecord::parse("record", &record.to_bytes(), &setup.gateway, &setup.mode)?;
        let seen = self.journal_of_run(&record.run_id).await?;
        let work = WorkList::for_finalize(record, seen).map_err(|why| self.mirror_fail(OP, why))?;
        let (_, deploy, coll) = self.apply_roots(OP).await?;
        let value = self
            .run_ctl(
                OP,
                &work,
                &[(record_file(record.state), record.to_bytes())],
                &coll,
                &deploy,
            )
            .await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.refusal(OP, &value, Some(&work)).await);
        }
        Ok(())
    }

    async fn read_record(&self, run_id: &RunId) -> Result<Option<RunRecord>, CoreError> {
        const OP: &str = "read_record";
        let setup = self.apply_setup(OP)?;
        let location = location_of(
            LedgerRoot::AccountHome,
            &record_rel(&setup.gateway, &setup.mode, run_id.as_str()),
        );
        let invalid = |problem: String| CoreError::DeployRunInvalid {
            run_id: run_id.as_str().to_string(),
            problem: format!("{location}: {problem}"),
        };
        let bytes = self
            .read_home_file(
                &record_rel(&setup.gateway, &setup.mode, run_id.as_str()),
                RECORD_MAX_BYTES,
                "the run record",
                (&setup.gateway, &setup.mode),
                &invalid,
            )
            .await?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let record = RunRecord::parse(&location, &bytes, &setup.gateway, &setup.mode)?;
        if record.run_id != run_id.as_str() {
            return Err(invalid("the record names a different run".to_string()));
        }
        Ok(Some(record))
    }

    async fn fetch_snapshot(
        &self,
        run_id: &RunId,
        dest: &Path,
    ) -> Result<BTreeMap<String, String>, CoreError> {
        const OP: &str = "fetch_snapshot";
        let setup = self.apply_setup(OP)?;
        let home = self.account_home().await?;
        let mut budget = super::Budget::lenient();
        let found = self
            .descend_from(
                &home,
                &[
                    "ign-deploy",
                    &setup.gateway,
                    &setup.mode,
                    "snapshots",
                    run_id.as_str(),
                ],
                &mut budget,
            )
            .await
            .map_err(|problem| match problem {
                super::Descend::Failed(err) => err,
                super::Descend::Symlink(_) => self.mirror_fail(
                    OP,
                    "a path component of the snapshot is a symlink; symlinks are refused",
                ),
                super::Descend::NotDir(_) => {
                    self.mirror_fail(OP, "a path component of the snapshot is not a directory")
                }
            })?;
        let Some((remote, entry)) = found else {
            return Err(self.mirror_fail(
                OP,
                format!("run {} has no snapshot on this host", run_id.as_str()),
            ));
        };
        if entry.kind != super::EntryKind::Dir {
            return Err(self.mirror_fail(OP, "the snapshot is not a directory"));
        }
        safe_remote_path(&remote)
            .map_err(|why| self.mirror_fail(OP, format!("the snapshot path is unsafe: {why}")))?;
        let out = dest.join("snapshot");
        std::fs::create_dir(&out).map_err(|e| {
            self.mirror_fail(
                OP,
                format!("cannot create the snapshot copy directory ({})", e.kind()),
            )
        })?;
        self.download_snapshot(OP, &remote, &out).await
    }

    async fn discard_interrupted(&self, run_id: &RunId) -> Result<(), CoreError> {
        const OP: &str = "discard";
        let setup = self.apply_setup(OP)?;
        let seen = self.journal_of_run(run_id.as_str()).await?;
        if let SeenJournal::Hash(_) = &seen {
            let location = journal_location(LedgerRoot::AccountHome, &setup.gateway, &setup.mode);
            let bytes = self
                .read_journal(&setup.gateway, &setup.mode)
                .await?
                .ok_or_else(|| self.mirror_fail(OP, "the journal vanished while it was read"))?;
            let journal = Journal::parse(&location, &bytes, &setup.gateway, &setup.mode)?;
            if !journal.phase.before_first_move() {
                return Err(CoreError::DeployRunInvalid {
                    run_id: run_id.as_str().to_string(),
                    problem: "the run may have moved files; restore it instead of discarding it"
                        .to_string(),
                });
            }
        }
        let work = WorkList::for_discard(run_id.as_str(), &setup.gateway, &setup.mode, seen)
            .map_err(|why| self.mirror_fail(OP, why))?;
        let (_, deploy, coll) = self.apply_roots(OP).await?;
        let value = self.run_ctl(OP, &work, &[], &coll, &deploy).await?;
        if !self.result_ok(OP, &value)? {
            return Err(self.refusal(OP, &value, Some(&work)).await);
        }
        Ok(())
    }
}
