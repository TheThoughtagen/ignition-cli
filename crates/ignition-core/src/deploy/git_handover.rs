//! Keep a gateway project's `.git` across an import that deletes it, and
//! advance it to the release commit (Phase 27).
//!
//! The git module deletes `data/projects/<p>/.git` when an import replaces a
//! module-managed project (R-3). The handover is three steps around that
//! import:
//!
//! 1. [`prepare_advance`](super::git_advance::prepare_advance) (before the
//!    import, on ign's machine) computes the pack and the release blobs.
//! 2. [`save`] copies the `.git` host-locally, through the confined
//!    [`GitStateWriter`], to a path no import reaches.
//! 3. [`restore_and_advance`] (after the import) builds the advance from the
//!    read-back export with
//!    [`finish_advance`](super::git_advance::finish_advance) **before any
//!    write to `.git`**, then either restores the save (the import deleted
//!    `.git`) or advances the `.git` that survived (R-6), and reads the
//!    written files back.
//!
//! Nothing here replaces, deletes or renames an existing `.git`: a `.git`
//! that is not the one the advance was prepared against stops the handover
//! with reason `git_recreated` and is left as it is. A failure keeps the save.
//!
//! Each write of the advance also carries what it expects to replace
//! ([`OldValue`], filled from the `.git` the pre-advance check read), and a
//! host that compares under the file's lock refuses a target that changed
//! meanwhile: reason `git_changed` (27-45).

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::git_advance::{
    AdvanceFailure, AdvancePrep, GitWrite, GitWriteName, OldValue, config_digest, finish_advance,
};
use super::git_remote::HostGit;
use super::git_state::{GitFiles, GitRefs, GitState, GitStateKind, classify, redact_text};
use super::ledger::{LedgerRoot, git_save_location};
use super::transport::projects::{
    GIT_CHANGED_OPERATION, GitEntry, GitHostRead, GitManifest, GitStateWriter,
};
use crate::error::CoreError;

/// The reason words a failed handover carries. Every word has a producer in
/// [`restore_and_advance`].
pub const RESTORE_REASONS: &[&str] = &[
    "restore_failed",
    "advance_failed",
    "status_unexpected",
    "git_recreated",
    "git_changed",
];

/// A finished [`save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSaveReport {
    /// The gateway the save belongs to.
    pub gateway: String,
    /// The project.
    pub project: String,
    /// The run that took it.
    pub run_id: String,
    /// Where the copy lives, relative to the ledger root:
    /// `ign-deploy/<gw>/projects/<p>/git-save/<run-id>/repo`.
    pub path: String,
    /// What the copy holds (equal to the source's before the copy).
    pub manifest: GitManifest,
}

/// A finished [`restore_and_advance`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRestoreReport {
    /// The save was copied back because the import had deleted `.git`.
    pub restored: bool,
    /// The advance was written.
    pub advanced: bool,
    /// Every written file was read back and equals the planned bytes.
    pub verified: bool,
    /// What a git status over the advanced repo lists, byte-sorted.
    pub expected_drift: Vec<String>,
    /// In-scope artifact members the gateway did not keep (assume-unchanged).
    pub dropped: Vec<String>,
    /// Where the save is (kept; the caller removes it once everything held).
    pub save_path: String,
    /// The advance abandoned gateway commits (27-15, D-05): the save holds the
    /// repo as the module left it, so the caller keeps it after a green run
    /// instead of removing it.
    pub keep_save: bool,
}

/// The gateway commits an advance abandons (27-15, D-05): the advance writes
/// `refs/ign-discarded/<run_id>` naming `abandoned_head` before the branch ref
/// moves, so they stay reachable once the branch moves to the release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Discarding<'a> {
    /// The run that discards them (the ref's name).
    pub run_id: &'a str,
    /// The gateway's HEAD commit before the advance.
    pub abandoned_head: &'a str,
}

/// Why a handover stopped. The save is kept in every case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRestoreFailure {
    /// One of [`RESTORE_REASONS`].
    pub reason: &'static str,
    /// The save had already been copied back to `.git` when the handover
    /// stopped.
    pub restored: bool,
    /// The paths the reason names (the differing paths of a
    /// `status_unexpected`), at most 200, byte-sorted.
    pub paths: Vec<String>,
    /// A redacted description.
    pub detail: String,
    /// Where the save is.
    pub save_path: String,
}

impl fmt::Display for GitRestoreFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.reason, self.detail)
    }
}

impl std::error::Error for GitRestoreFailure {}

/// Copy the project's `.git` to the host-local save of `run_id`, and check
/// that the copy reads back as the manifest the writer reported. `root` is
/// the node's ledger root ([`LedgerRoot::for_node`]), under which the writer
/// keeps the save; the report's `path` names that location.
pub async fn save(
    read: &dyn GitHostRead,
    write: &dyn GitStateWriter,
    root: LedgerRoot,
    gateway: &str,
    project: &str,
    run_id: &str,
) -> Result<GitSaveReport, CoreError> {
    let manifest = write.save_git(gateway, project, run_id).await?;
    let seen = read.save_manifest(gateway, project, run_id).await?;
    if seen != manifest {
        return Err(CoreError::Internal(
            "the saved .git does not read back as the copy that was written".to_string(),
        ));
    }
    Ok(GitSaveReport {
        gateway: gateway.to_string(),
        project: project.to_string(),
        run_id: run_id.to_string(),
        path: git_save_location(root, gateway, project, run_id),
        manifest,
    })
}

/// What `.git` is now, classified the way the prepare classified it, with the
/// refs it was classified from and the files the answer was read from: the
/// advance's expected old values come from these, not from a second read.
async fn classify_live(
    read: &dyn GitHostRead,
    git: &HostGit,
    project: &str,
) -> Result<(GitState, Option<GitRefs>, GitFiles), CoreError> {
    let files = read.read_git_state(project).await?;
    files.check_caps()?;
    let config = match (&files.kind, &files.config) {
        (GitEntry::Directory, Some(bytes)) => Some(git.parse_config(bytes).await?),
        _ => None,
    };
    let refs = if files.kind == GitEntry::Directory {
        git.parse_refs(&files).await?
    } else {
        None
    };
    let state = classify(&files, config.as_ref(), refs.as_ref());
    Ok((state, refs, files))
}

/// The old value of a file read as `bytes`: its digest, or absent.
fn old_of(bytes: Option<&[u8]>) -> OldValue {
    match bytes {
        Some(bytes) => OldValue::Sha256(format!("{:x}", Sha256::digest(bytes))),
        None => OldValue::Absent,
    }
}

/// The ref names a `packed-refs` file lists (`<id> <name>` lines; comment and
/// peeled lines carry no name).
fn packed_ref_names(packed_refs: &[u8]) -> BTreeSet<&str> {
    std::str::from_utf8(packed_refs)
        .unwrap_or("")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with('^'))
        .filter_map(|line| line.split_once(' ').map(|(_, name)| name.trim_end()))
        .collect()
}

/// Give every write the value it expects to replace (27-45, F4), taken from
/// `files`, the `.git` the pre-advance check read, and `index`, the digest of
/// the index read right after it. HEAD, config and a loose ref carry the
/// digest of their file or `Absent`; a ref that is not loose is `Absent`, and
/// when `packed-refs` lists it the write also names the state of
/// `packed-refs`; the discard ref is `Absent` (its name is the run's own);
/// only the content-addressed pack and idx stay [`OldValue::Unchecked`].
fn fill_old_values(writes: &mut [GitWrite], files: &GitFiles, index: &OldValue) {
    let packed = files.packed_refs.as_deref();
    let listed = packed.map(packed_ref_names).unwrap_or_default();
    for write in writes {
        write.packed_refs_old = None;
        write.old = match &write.name {
            GitWriteName::PackFile(_) | GitWriteName::PackIndex(_) => OldValue::Unchecked,
            GitWriteName::Index => index.clone(),
            GitWriteName::Head => old_of(files.head.as_deref()),
            GitWriteName::Config => old_of(files.config.as_deref()),
            GitWriteName::DiscardRef(_) => OldValue::Absent,
            GitWriteName::BranchRef(_) | GitWriteName::RemoteRef { .. } => {
                let rel = write.name.rel();
                let loose = files
                    .loose_refs
                    .iter()
                    .find(|(name, _)| *name == rel)
                    .map(|(_, bytes)| bytes.as_slice());
                if loose.is_none() && listed.contains(rel.as_str()) {
                    write.packed_refs_old = Some(old_of(packed));
                }
                old_of(loose)
            }
        };
    }
}

/// The pack bases of `prep` that no ref of the live repo names, byte-sorted.
/// The pack leaves out every object reachable from its bases, so a base the
/// repo no longer names (a local branch the prepared repo had and this one
/// lacks) may be a commit whose objects are not there: [`GitState`] compares
/// HEAD, the upstream and the remote's refs, not every local branch.
fn unnamed_bases(prep: &AdvancePrep, refs: Option<&GitRefs>) -> Vec<String> {
    let named: BTreeSet<&str> = refs
        .into_iter()
        .flat_map(|r| {
            r.head
                .commit()
                .into_iter()
                .chain(r.heads.values().map(String::as_str))
                .chain(r.remotes.values().map(String::as_str))
        })
        .collect();
    let mut missing: Vec<String> = prep
        .pack_bases
        .iter()
        .filter(|base| !named.contains(base.as_str()))
        .cloned()
        .collect();
    missing.sort();
    missing
}

/// After the import: build the advance, put `.git` back if the import deleted
/// it, write the advance and read it back.
///
/// In order: (a) [`finish_advance`] builds the assume-unchanged set and the
/// index from the read-back `export_zip` and checks host git's status against
/// the prediction, all on ign's machine and before any `.git` write; a
/// mismatch is `status_unexpected` and a host-git failure `advance_failed`,
/// with nothing restored or written. (b) `.git` absent: the save must still
/// equal what was saved, is restored, and the restored repo must be the one the
/// advance was prepared against (`state`); `.git` present: it must be that
/// same repo, else `git_recreated` and it is left alone; anything else at
/// `.git` is `git_recreated` too. Either way, some ref of the repo must still
/// name every commit the pack was built against ([`AdvancePrep::pack_bases`]),
/// or the pack may leave objects missing. Every write then gets the value it
/// expects to replace, from the state that check read ([`OldValue`], 27-45).
/// (c) the writes, lock-and-rename, through
/// `write`, in the plan's order: the index before every ref, config and HEAD
/// write, so a write that fails part way leaves HEAD and the branch at the old
/// commit over an index that holds the release (27-26, WR-A1). A write whose
/// target is no longer what was read is refused by the host under the file's
/// lock and ends the handover `git_changed`; the writes before it stay and
/// nothing is overwritten. (d) each written file's sha256 read back.
///
/// With `discard` the writes of (c) include `refs/ign-discarded/<run id>`
/// naming the abandoned HEAD (27-15, D-05), and the report says `keep_save`:
/// the caller keeps the save of the repo it abandoned.
// The handover needs the whole context at once; bundling it in a struct would
// only rename the arguments.
#[allow(clippy::too_many_arguments)]
pub async fn restore_and_advance(
    read: &dyn GitHostRead,
    write: &dyn GitStateWriter,
    git: &HostGit,
    checkout: &Path,
    state: &GitState,
    save: &GitSaveReport,
    prep: &AdvancePrep,
    export_zip: &[u8],
    discard: Option<Discarding<'_>>,
) -> Result<GitRestoreReport, GitRestoreFailure> {
    let restored_now = std::sync::atomic::AtomicBool::new(false);
    let fail = |reason: &'static str, paths: Vec<String>, detail: String| GitRestoreFailure {
        reason,
        restored: restored_now.load(std::sync::atomic::Ordering::Relaxed),
        paths,
        detail: redact_text(&detail),
        save_path: save.path.clone(),
    };
    let core = |reason: &'static str, what: &str, err: CoreError| {
        fail(reason, Vec::new(), format!("{what}: {err}"))
    };
    let project = save.project.as_str();

    // (a) Everything that can fail without touching the gateway. The node's
    // disk after the import is what the prediction is made from (27-40), read
    // on every node kind (27-41).
    let disk = read
        .project_file_digests(project)
        .await
        .map_err(|e| core("advance_failed", "the project's files could not be read", e))?;
    let mut plan = match finish_advance(git, checkout, prep, export_zip, &disk).await {
        Ok(mut plan) => {
            // The abandoned HEAD stays reachable through a ref the advance
            // writes right before the branch ref moves (27-15, 27-26). The ref
            // write is part of the plan, so it is shape-checked with the rest
            // before any `.git` write.
            if let Some(discard) = discard
                && let Err(why) = plan.keep_discarded(discard.run_id, discard.abandoned_head)
            {
                return Err(fail("advance_failed", Vec::new(), why.to_string()));
            }
            plan
        }
        Err(AdvanceFailure::Mismatch(m)) => {
            return Err(fail(
                "status_unexpected",
                m.unexpected.clone(),
                format!(
                    "{} path(s) differ from the predicted status; nothing was restored or written",
                    m.count
                ),
            ));
        }
        Err(AdvanceFailure::HostGit { detail }) => {
            return Err(fail("advance_failed", Vec::new(), detail));
        }
    };

    // (b) Restore, or confirm the surviving repo is the one prepared against.
    let files = read
        .read_git_state(project)
        .await
        .map_err(|e| core("restore_failed", "the .git probe failed", e))?;
    let mut restored = false;
    // The `.git` the checks below classify: the writes' old values come from it.
    let live_files: GitFiles;
    match files.kind {
        GitEntry::Absent => {
            let still = read
                .save_manifest(&save.gateway, project, &save.run_id)
                .await
                .map_err(|e| core("restore_failed", "the save cannot be read", e))?;
            if still != save.manifest {
                return Err(fail(
                    "restore_failed",
                    Vec::new(),
                    "the save no longer matches what was saved; nothing was restored".to_string(),
                ));
            }
            let back = write
                .restore_git(&save.gateway, project, &save.run_id)
                .await
                .map_err(|e| core("restore_failed", "the save could not be restored", e))?;
            if back != save.manifest {
                return Err(fail(
                    "restore_failed",
                    Vec::new(),
                    "the restored .git does not match the save".to_string(),
                ));
            }
            restored = true;
            restored_now.store(true, std::sync::atomic::Ordering::Relaxed);
            let (now, refs, read_files) = classify_live(read, git, project)
                .await
                .map_err(|e| core("restore_failed", "the restored .git cannot be read", e))?;
            live_files = read_files;
            let unnamed = unnamed_bases(prep, refs.as_ref());
            if !unnamed.is_empty() {
                return Err(fail(
                    "restore_failed",
                    Vec::new(),
                    format!(
                        "no ref of the restored .git names {}, which the pack was built \
                         against (it changed between the prepare and the save)",
                        unnamed.join(", ")
                    ),
                ));
            }
            if &now != state {
                return Err(fail(
                    "restore_failed",
                    Vec::new(),
                    "the restored .git is not the repo the advance was prepared against \
                     (it changed between the prepare and the save)"
                        .to_string(),
                ));
            }
            if !config_is_base(read, project, prep).await.map_err(|e| {
                core(
                    "restore_failed",
                    "the restored .git config cannot be read",
                    e,
                )
            })? {
                return Err(fail(
                    "restore_failed",
                    vec![".git/config".to_string()],
                    "the restored .git config is not the config the advance was prepared \
                     against (it changed between the prepare and the save)"
                        .to_string(),
                ));
            }
        }
        GitEntry::Directory => {
            let (now, refs, read_files) = classify_live(read, git, project)
                .await
                .map_err(|e| core("restore_failed", "the .git probe failed", e))?;
            live_files = read_files;
            // A managed repo is advanced and an adoptable one is adopted; the
            // prepare's classification must be exactly what is there now.
            if !matches!(now.kind, GitStateKind::Managed | GitStateKind::Adoptable) || &now != state
            {
                return Err(fail(
                    "git_recreated",
                    vec![".git".to_string()],
                    "the .git the import left is not the repo the advance was prepared against; \
                     it is left as it is"
                        .to_string(),
                ));
            }
            let unnamed = unnamed_bases(prep, refs.as_ref());
            if !unnamed.is_empty() {
                return Err(fail(
                    "git_recreated",
                    vec![".git".to_string()],
                    format!(
                        "no ref of the .git the import left names {}, which the pack was \
                         built against, so it may lack those objects; it is left as it is",
                        unnamed.join(", ")
                    ),
                ));
            }
            if !config_is_base(read, project, prep)
                .await
                .map_err(|e| core("restore_failed", "the .git config cannot be read", e))?
            {
                return Err(fail(
                    "git_recreated",
                    vec![".git/config".to_string()],
                    "the .git config is not the config the advance was prepared against; it is \
                     left as it is"
                        .to_string(),
                ));
            }
        }
        GitEntry::File | GitEntry::Symlink | GitEntry::Other => {
            return Err(fail(
                "git_recreated",
                vec![".git".to_string()],
                format!(
                    ".git is now a {}, not a directory; it is left as it is",
                    files.kind.as_str()
                ),
            ));
        }
    }

    // Every write carries what it expects to replace (27-45, F4), from the
    // `.git` the checks above classified. The index is hashed once, right
    // after them; no other file is read again.
    let index_old = if live_files.index_present {
        let digest = read
            .git_file_digest(project, &GitWriteName::Index)
            .await
            .map_err(|e| core("advance_failed", "the .git index cannot be read", e))?;
        match digest {
            Some(digest) => OldValue::Sha256(digest),
            None => OldValue::Absent,
        }
    } else {
        OldValue::Absent
    };
    fill_old_values(&mut plan.writes, &live_files, &index_old);

    // (c) The advance.
    write
        .write_git_files(project, &plan.writes)
        .await
        .map_err(|e| match &e {
            // A target moved between the read and its lock: the writes before
            // it are in place and nothing was overwritten (27-45, F4).
            CoreError::TransportFailed {
                operation, detail, ..
            } if operation == GIT_CHANGED_OPERATION => fail(
                "git_changed",
                Vec::new(),
                format!(
                    "a file of .git changed after the run read it, and the advance stopped \
                     there ({detail}); the writes before it, the index first, are in place \
                     and nothing was overwritten"
                ),
            ),
            _ => core("advance_failed", "the advance could not be written", e),
        })?;

    // (d) Read every written file back.
    for planned in &plan.writes {
        let want = format!("{:x}", Sha256::digest(&planned.bytes));
        let seen = read
            .git_file_digest(project, &planned.name)
            .await
            .map_err(|e| {
                core(
                    "advance_failed",
                    "a written .git file cannot be read back",
                    e,
                )
            })?;
        if seen.as_deref() != Some(want.as_str()) {
            return Err(fail(
                "advance_failed",
                vec![describe(&planned.name)],
                format!(
                    "{} does not read back as the bytes that were written",
                    describe(&planned.name)
                ),
            ));
        }
    }
    Ok(GitRestoreReport {
        restored,
        advanced: true,
        verified: true,
        expected_drift: plan.expected_drift,
        dropped: plan.dropped,
        save_path: save.path.clone(),
        keep_save: discard.is_some(),
    })
}

/// Whether the live `.git` config is the one an adoption's config was built
/// from (27-17): the adopted config replaces the whole file, so it is written
/// only over the bytes it was derived from. True when the advance carries no
/// config.
async fn config_is_base(
    read: &dyn GitHostRead,
    project: &str,
    prep: &AdvancePrep,
) -> Result<bool, CoreError> {
    let Some(base) = &prep.config_base else {
        return Ok(true);
    };
    let files = read.read_git_state(project).await?;
    Ok(files.config.as_deref().map(config_digest).as_deref() == Some(base.as_str()))
}

/// After an import that did not read back as the artifact (K11): put the
/// saved `.git` back exactly as it was if the import removed it, and never
/// advance it.
///
/// The `.git` is probed: absent, the save must still equal what was saved, is
/// restored, and the restored copy must read back as the save; present (the
/// import left a repo or the module recreated one), nothing is written and
/// the repo is left as it is. A `.git` that is not a directory is
/// `git_recreated` and is left alone. The save is kept in every case: the
/// caller removes it once it knows the repo equals it.
pub async fn restore_unchanged(
    read: &dyn GitHostRead,
    write: &dyn GitStateWriter,
    gateway: &str,
    project: &str,
    save: &GitSaveReport,
) -> Result<GitRestoreReport, GitRestoreFailure> {
    let fail = |reason: &'static str, restored: bool, paths: Vec<String>, detail: String| {
        GitRestoreFailure {
            reason,
            restored,
            paths,
            detail: redact_text(&detail),
            save_path: save.path.clone(),
        }
    };
    let files = read.read_git_state(project).await.map_err(|e| {
        fail(
            "restore_failed",
            false,
            Vec::new(),
            format!("the .git probe failed: {e}"),
        )
    })?;
    let report = |restored: bool| GitRestoreReport {
        restored,
        advanced: false,
        verified: restored,
        expected_drift: Vec::new(),
        dropped: Vec::new(),
        save_path: save.path.clone(),
        keep_save: false,
    };
    match files.kind {
        GitEntry::Absent => {
            let still = read
                .save_manifest(gateway, project, &save.run_id)
                .await
                .map_err(|e| {
                    fail(
                        "restore_failed",
                        false,
                        Vec::new(),
                        format!("the save cannot be read: {e}"),
                    )
                })?;
            if still != save.manifest {
                return Err(fail(
                    "restore_failed",
                    false,
                    Vec::new(),
                    "the save no longer matches what was saved; nothing was restored".to_string(),
                ));
            }
            let back = write
                .restore_git(gateway, project, &save.run_id)
                .await
                .map_err(|e| {
                    fail(
                        "restore_failed",
                        false,
                        Vec::new(),
                        format!("the save could not be restored: {e}"),
                    )
                })?;
            if back != save.manifest {
                return Err(fail(
                    "restore_failed",
                    false,
                    Vec::new(),
                    "the restored .git does not match the save".to_string(),
                ));
            }
            Ok(report(true))
        }
        GitEntry::Directory => Ok(report(false)),
        GitEntry::File | GitEntry::Symlink | GitEntry::Other => Err(fail(
            "git_recreated",
            false,
            vec![".git".to_string()],
            format!(
                ".git is now a {}, not a directory; it is left as it is",
                files.kind.as_str()
            ),
        )),
    }
}

fn describe(name: &GitWriteName) -> String {
    format!(".git/{}", name.rel())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::git_advance::tests::{CODE, Fixture, git, put, read_git_files};
    use crate::deploy::git_advance::{GitWrite, OldValue, prepare_advance};
    use crate::deploy::git_objects::{GitObjectFile, GitObjectList};
    use crate::deploy::transport::projects::{GitSize, ImportDir};
    use std::path::PathBuf;

    /// A host whose project `.git` is a directory on this machine; the save
    /// is never read back here, so it answers one fixed manifest.
    struct DirHost {
        git_dir: PathBuf,
        /// Every write the advance handed to `write_git_files`, in order.
        seen: std::sync::Mutex<Vec<GitWrite>>,
    }

    fn manifest() -> GitManifest {
        GitManifest {
            files: 1,
            bytes: 1,
            digest: "0".repeat(64),
        }
    }

    fn not_used() -> CoreError {
        CoreError::Internal("this host call is not part of the handover under test".to_string())
    }

    #[async_trait::async_trait]
    impl GitHostRead for DirHost {
        async fn project_file_digests(
            &self,
            _project: &str,
        ) -> Result<crate::deploy::git_advance::ProjectDisk, CoreError> {
            let project_dir = self.git_dir.parent().ok_or_else(not_used)?;
            Ok(crate::deploy::git_advance::tests::disk_below(project_dir))
        }
        async fn read_git_state(
            &self,
            _project: &str,
        ) -> Result<super::super::git_state::GitFiles, CoreError> {
            Ok(read_git_files(&self.git_dir))
        }
        async fn git_manifest(&self, _project: &str) -> Result<GitManifest, CoreError> {
            Err(not_used())
        }
        async fn save_manifest(
            &self,
            _gateway: &str,
            _project: &str,
            _run_id: &str,
        ) -> Result<GitManifest, CoreError> {
            Ok(manifest())
        }
        async fn git_file_digest(
            &self,
            _project: &str,
            name: &GitWriteName,
        ) -> Result<Option<String>, CoreError> {
            Ok(std::fs::read(self.git_dir.join(name.rel()))
                .ok()
                .map(|bytes| format!("{:x}", Sha256::digest(bytes))))
        }
        async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
            Err(not_used())
        }
        async fn project_dir_entry(
            &self,
            _project: &str,
            _dir: ImportDir,
        ) -> Result<GitEntry, CoreError> {
            Err(not_used())
        }
        async fn free_bytes(&self, _gateway: &str, _project: &str) -> Result<u64, CoreError> {
            Err(not_used())
        }
        async fn git_size(&self, _project: &str) -> Result<GitSize, CoreError> {
            Err(not_used())
        }
        async fn list_git_objects(&self, _project: &str) -> Result<GitObjectList, CoreError> {
            Err(not_used())
        }
        async fn fetch_git_objects(
            &self,
            _project: &str,
            _files: &[GitObjectFile],
            _dest: &Path,
        ) -> Result<(), CoreError> {
            Err(not_used())
        }
    }

    #[async_trait::async_trait]
    impl GitStateWriter for DirHost {
        async fn save_git(
            &self,
            _gateway: &str,
            _project: &str,
            _run_id: &str,
        ) -> Result<GitManifest, CoreError> {
            Ok(manifest())
        }
        async fn restore_git(
            &self,
            _gateway: &str,
            _project: &str,
            _run_id: &str,
        ) -> Result<GitManifest, CoreError> {
            Err(not_used())
        }
        async fn write_git_files(
            &self,
            _project: &str,
            writes: &[GitWrite],
        ) -> Result<(), CoreError> {
            self.seen
                .lock()
                .expect("seen")
                .extend(writes.iter().cloned());
            for write in writes {
                let path = self.git_dir.join(write.name.rel());
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(path, &write.bytes).expect("write");
            }
            Ok(())
        }
        async fn remove_git_save(
            &self,
            _gateway: &str,
            _project: &str,
            _run_id: &str,
        ) -> Result<(), CoreError> {
            Err(not_used())
        }
        async fn probe_write(&self, _project: &str, _run_id: &str) -> Result<(), CoreError> {
            Err(not_used())
        }
    }

    /// The save's path is named under the node's own ledger root: an `ssh`
    /// node keeps it in the account home, a `local` node in its data dir.
    #[tokio::test]
    async fn save_path_follows_the_ledger_root() {
        let host = DirHost {
            git_dir: PathBuf::from("unused"),
            seen: Default::default(),
        };
        let home = save(
            &host,
            &host,
            LedgerRoot::AccountHome,
            "gw-a",
            "CapApp",
            "r1",
        )
        .await
        .expect("save");
        assert_eq!(
            home.path,
            "~/ign-deploy/gw-a/projects/CapApp/git-save/r1/repo"
        );
        let data = save(&host, &host, LedgerRoot::DataDir, "gw-a", "CapApp", "r1")
            .await
            .expect("save");
        assert_eq!(
            data.path,
            "ign-deploy/gw-a/projects/CapApp/git-save/r1/repo"
        );
    }

    /// What `old` the engine must give a write of `rel`: the digest of the
    /// loose file on disk, or `Absent` when there is none.
    fn disk_old(git_dir: &Path, rel: &str) -> OldValue {
        match std::fs::read(git_dir.join(rel)) {
            Ok(bytes) => OldValue::Sha256(format!("{:x}", Sha256::digest(bytes))),
            Err(_) => OldValue::Absent,
        }
    }

    /// Run a green advance of the fixture's managed repo against a recording
    /// host and return the writes it made, with the `.git` as it was before.
    async fn advance_writes_of(f: &Fixture) -> Vec<GitWrite> {
        let git_host = HostGit::new();
        let (state, refs) = f.gateway_state(&git_host).await;
        let members = f.members(&git_host, &[]).await;
        let prep = prepare_advance(&git_host, &f.seed, &state, &refs, &f.c2, &members)
            .await
            .expect("prepare");
        let export = f.import(&git_host, &[], &[], None).await;
        let host = DirHost {
            git_dir: f.git_dir(),
            seen: Default::default(),
        };
        let saved = save(&host, &host, LedgerRoot::DataDir, "gw", "CapApp", "r1")
            .await
            .expect("save");
        // The index the gateway holds before the advance.
        std::fs::write(f.git_dir().join("index"), b"the gateway's index").expect("index");
        restore_and_advance(
            &host, &host, &git_host, &f.seed, &state, &saved, &prep, &export, None,
        )
        .await
        .expect("a managed repo advances");
        host.seen.lock().expect("seen").clone()
    }

    /// 27-45 (F4): every write of the advance carries what it expects to
    /// replace, taken from the state the pre-advance check read. HEAD, config
    /// and the refs carry the digest of their loose file (or `Absent`), the
    /// index the digest of the index, and only the pack and its idx are
    /// unchecked.
    #[tokio::test]
    async fn advance_writes_carry_their_old_values() {
        let f = Fixture::new();
        let before = std::fs::read(f.git_dir().join("refs/heads/production")).expect("loose");
        assert!(!before.is_empty(), "the fixture's branch ref is loose");
        let (head, index) = (
            disk_old(&f.git_dir(), "HEAD"),
            OldValue::Sha256(format!("{:x}", Sha256::digest(b"the gateway's index"))),
        );
        let want_branch = disk_old(&f.git_dir(), "refs/heads/production");
        let want_remote = disk_old(&f.git_dir(), "refs/remotes/origin/production");
        let writes = advance_writes_of(&f).await;
        assert!(!writes.is_empty());
        let mut seen_pack = false;
        let mut seen_index = false;
        for write in &writes {
            match &write.name {
                GitWriteName::PackFile(_) | GitWriteName::PackIndex(_) => {
                    seen_pack = true;
                    assert_eq!(write.old, OldValue::Unchecked, "{:?}", write.name);
                    assert_eq!(write.packed_refs_old, None);
                }
                GitWriteName::Index => {
                    seen_index = true;
                    assert_eq!(write.old, index, "the index carries its digest");
                    assert_eq!(write.packed_refs_old, None);
                }
                GitWriteName::Head => assert_eq!(write.old, head),
                GitWriteName::BranchRef(_) => {
                    assert_eq!(write.old, want_branch, "a loose branch ref");
                    assert_eq!(
                        write.packed_refs_old, None,
                        "a loose ref names no packed-refs"
                    );
                }
                GitWriteName::RemoteRef { .. } => assert_eq!(write.old, want_remote),
                GitWriteName::Config => unreachable!("a managed repo's config is not written"),
                GitWriteName::DiscardRef(_) => unreachable!("no discard here"),
            }
            if !matches!(
                write.name,
                GitWriteName::PackFile(_) | GitWriteName::PackIndex(_)
            ) {
                assert_ne!(
                    write.old,
                    OldValue::Unchecked,
                    "{:?} is compared",
                    write.name
                );
            }
        }
        assert!(seen_pack && seen_index);
    }

    /// 27-45: a branch that lives only in `packed-refs` has no loose file, so
    /// its old value is `Absent` and it also names the digest of
    /// `packed-refs`, the file a Designer's or the module's `pack-refs` would
    /// rewrite.
    #[tokio::test]
    async fn advance_old_value_of_a_packed_branch_names_packed_refs() {
        let f = Fixture::new();
        git(&f.gw, &["pack-refs", "--all", "--prune"]);
        assert!(
            !f.git_dir().join("refs/heads/production").exists(),
            "the branch is packed"
        );
        let packed = std::fs::read(f.git_dir().join("packed-refs")).expect("packed-refs");
        let want = OldValue::Sha256(format!("{:x}", Sha256::digest(&packed)));
        let writes = advance_writes_of(&f).await;
        let branch = writes
            .iter()
            .find(|w| matches!(w.name, GitWriteName::BranchRef(_)))
            .expect("a branch write");
        assert_eq!(branch.old, OldValue::Absent);
        assert_eq!(branch.packed_refs_old, Some(want.clone()));
        let remote = writes
            .iter()
            .find(|w| matches!(w.name, GitWriteName::RemoteRef { .. }))
            .expect("a remote ref write");
        assert_eq!(remote.old, OldValue::Absent);
        assert_eq!(remote.packed_refs_old, Some(want));
        let head = writes
            .iter()
            .find(|w| matches!(w.name, GitWriteName::Head))
            .expect("HEAD");
        assert_eq!(head.packed_refs_old, None, "HEAD is never in packed-refs");
    }

    /// The surviving `.git` has the prepared HEAD, upstream and remote refs,
    /// but no longer the local branch whose commit the pack was built
    /// against, nor that commit's objects. The advance is refused and nothing
    /// is written; before, it wrote a branch over a missing parent.
    #[tokio::test]
    async fn advance_refused_when_a_pack_base_is_gone() {
        let mut f = Fixture::new();
        let git_host = HostGit::new();
        // The gateway holds C2 on a local branch, and the release C3 follows C2.
        let seed = f.seed.to_str().expect("utf8").to_string();
        git(&f.gw, &["fetch", "-q", &seed, "production:side"]);
        put(&f.seed, CODE, "def f():\n    return 3\n");
        git(&f.seed, &["commit", "-q", "-am", "three"]);
        let c3 = git(&f.seed, &["rev-parse", "HEAD"]);
        let c2 = std::mem::replace(&mut f.c2, c3.clone());

        let (state, refs) = f.gateway_state(&git_host).await;
        let members = f.members(&git_host, &[]).await;
        let prep = prepare_advance(&git_host, &f.seed, &state, &refs, &c3, &members)
            .await
            .expect("prepare");
        let mut want_bases = vec![f.c1.clone(), c2.clone()];
        want_bases.sort();
        assert_eq!(prep.pack_bases, want_bases);

        // The branch and C2's objects go; HEAD, upstream and remote refs stay.
        git(&f.gw, &["branch", "-q", "-D", "side"]);
        git(&f.gw, &["reflog", "expire", "--expire=now", "--all"]);
        git(&f.gw, &["-c", "gc.auto=0", "gc", "-q", "--prune=now"]);
        let (now, _) = f.gateway_state(&git_host).await;
        assert_eq!(now, state, "the classification alone cannot tell");

        let export = f.import(&git_host, &[], &[], None).await;
        let host = DirHost {
            git_dir: f.git_dir(),
            seen: Default::default(),
        };
        let saved = save(&host, &host, LedgerRoot::DataDir, "gw", "CapApp", "r1")
            .await
            .expect("save");
        let head_before = std::fs::read(f.git_dir().join("HEAD")).expect("HEAD");
        let failure = restore_and_advance(
            &host, &host, &git_host, &f.seed, &state, &saved, &prep, &export, None,
        )
        .await
        .expect_err("a .git without a pack base is not advanced");
        assert_eq!(failure.reason, "git_recreated");
        assert!(failure.detail.contains(&c2), "{}", failure.detail);
        assert!(!failure.restored);
        assert_eq!(
            git(&f.gw, &["rev-parse", "HEAD"]),
            f.c1,
            "the branch did not move"
        );
        assert_eq!(
            std::fs::read(f.git_dir().join("HEAD")).expect("HEAD"),
            head_before
        );
    }
}
