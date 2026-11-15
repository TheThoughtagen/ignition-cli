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

use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::git_advance::{AdvanceFailure, AdvancePrep, GitWriteName, finish_advance};
use super::git_remote::HostGit;
use super::git_state::{GitState, GitStateKind, classify, redact_text};
use super::ledger::{LedgerRoot, git_save_location};
use super::transport::projects::{GitEntry, GitHostRead, GitManifest, GitStateWriter};
use crate::error::CoreError;

/// The reason words a failed handover carries. Every word has a producer in
/// [`restore_and_advance`].
pub const RESTORE_REASONS: &[&str] = &[
    "restore_failed",
    "advance_failed",
    "status_unexpected",
    "git_recreated",
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
/// `refs/ign-discarded/<run_id>` naming `abandoned_head`, so they stay
/// reachable once the branch moves to the release.
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
/// that the copy reads back as the manifest the writer reported.
pub async fn save(
    read: &dyn GitHostRead,
    write: &dyn GitStateWriter,
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
        path: git_save_location(LedgerRoot::DataDir, gateway, project, run_id),
        manifest,
    })
}

/// What `.git` is now, classified the way the prepare classified it.
async fn classify_live(
    read: &dyn GitHostRead,
    git: &HostGit,
    project: &str,
) -> Result<GitState, CoreError> {
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
    Ok(classify(&files, config.as_ref(), refs.as_ref()))
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
/// `.git` is `git_recreated` too. (c) the writes, lock-and-rename, through
/// `write`. (d) each written file's sha256 read back.
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

    // (a) Everything that can fail without touching the gateway.
    let plan = match finish_advance(git, checkout, prep, export_zip).await {
        Ok(mut plan) => {
            // The abandoned HEAD stays reachable through a ref the advance
            // writes next to the branch (27-15). The ref write is part of the
            // plan, so it is shape-checked with the rest before any `.git`
            // write.
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
            let now = classify_live(read, git, project)
                .await
                .map_err(|e| core("restore_failed", "the restored .git cannot be read", e))?;
            if &now != state {
                return Err(fail(
                    "restore_failed",
                    Vec::new(),
                    "the restored .git is not the repo the advance was prepared against \
                     (it changed between the prepare and the save)"
                        .to_string(),
                ));
            }
        }
        GitEntry::Directory => {
            let now = classify_live(read, git, project)
                .await
                .map_err(|e| core("restore_failed", "the .git probe failed", e))?;
            if now.kind != GitStateKind::Managed || &now != state {
                return Err(fail(
                    "git_recreated",
                    vec![".git".to_string()],
                    "the .git the import left is not the repo the advance was prepared against; \
                     it is left as it is"
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

    // (c) The advance.
    write
        .write_git_files(project, &plan.writes)
        .await
        .map_err(|e| core("advance_failed", "the advance could not be written", e))?;

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
