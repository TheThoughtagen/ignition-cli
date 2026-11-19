//! `ign deploy projects` and a gateway project the git module manages
//! (Phase 27, 27-12): the published `data.git` block, the gate and reason
//! words and the three steps around the import.
//!
//! The steps, in the order a run takes them:
//!
//! 1. [`prepare_git_host`] (right after the `.git` probe): read the gateway
//!    repo's files, classify them, check the release checkout, ask the remote
//!    for its heads and build the advance input ([`prepare_advance_with`]) on
//!    ign's machine. Nothing is written; a failure here refuses before any
//!    write, in `--check` and in the apply alike (B5). A partial repo the
//!    module left behind (adoptable, D-13) takes the same steps with its
//!    branch named explicitly: `git.yaml` `repo_branch`, else the target's
//!    `git_branch`, else the run refuses `branch_required` (D-23).
//! 2. [`prepare_git_live`] (after the live export): the gate facts against the
//!    live project, the itemised dirty list (from the gateway's own objects
//!    when ign's store lacks the gateway HEAD, D-24) and the refusals.
//! 3. [`git_before_import`] (right before the import): the repo is read again
//!    and any change since the prepare is `plan_stale`; then the `.git` is
//!    saved host-locally. [`git_after_verify`] (after a green verify) restores
//!    the save, advances the repo to the release commit and proves the result
//!    (D-25).
//!
//! `expected_drift` has one definition (checker W-6): it is known only after
//! an import, because the import rewrites every artifact member. Until a run
//! advanced the master it is `null` with `expected_drift_complete` false.
//!
//! No message, event or envelope field carries URL userinfo, a `git.yaml`
//! value or repo file content: remote URLs are redacted, and every list is
//! paths, ids and counts.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use super::{
    LoadedProject, ProjectAction, ProjectGateway, ProjectsEvent, ProjectsInput, ProjectsSurfaces,
};
use crate::deploy::apply::hold::SystemClock;
use crate::deploy::apply::preflight::{
    DesignersMode, DesignersPolicy, DesignersVerdict, designers_detail, designers_gate,
};
use crate::deploy::git_advance::{AdvanceMode, AdvancePrep, config_digest, prepare_advance_with};
use crate::deploy::git_gate::{
    GitGateFailure, LIST_CAP, NodeGates, ReleaseCheckout, RemoteView, check_release_checkout,
    gate_node, remote_view,
};
use crate::deploy::git_handover::{
    Discarding, GitRestoreFailure, GitSaveReport, restore_and_advance, restore_unchanged, save,
};
use crate::deploy::git_objects::dirty_from_gateway;
use crate::deploy::git_remote::HostGit;
use crate::deploy::git_state::{GitFiles, GitHead, GitRefs, GitState, GitStateKind, classify};
use crate::deploy::git_yaml::{
    GIT_YAML_CHECK, Hazard, entry_for, hazards_for, parse_git_yaml, refuses,
};
use crate::deploy::ledger::{LedgerRoot, git_save_location};
use crate::deploy::project_artifact::CanonicalDigest;
use crate::deploy::tag_converge::BackupTags;
use crate::deploy::transport::projects::{
    GitEntry, GitHostRead, GitManifest, GitStateWriter, ImportDir, ProjectHost, git_entry_location,
    git_state_writer_for_node, local_data_dir_for_node, project_host_for_node,
};
use crate::error::{CoreError, PreflightFailure};

/// Every published reason word with the plan that produces it (checkpoint
/// 27-12, checker W4). The unit test `reason_words_have_producers` asserts
/// that each word is a [`HOST_REASONS`] or [`RESTORE_REASONS`] word, or is
/// named in [`PRODUCED_HERE`], or is produced by a later plan of the phase,
/// and that no word of the first two lists is missing from this table.
///
/// [`RESTORE_REASONS`]: crate::deploy::git_handover::RESTORE_REASONS
pub const REASON_PRODUCERS: &[(&str, &str)] = &[
    ("not_directory", "git_state::classify (27-01)"),
    ("symlink", "git_state::classify (27-01)"),
    ("no_head", "git_state::classify (27-01)"),
    ("detached_head", "git_state::classify (27-01)"),
    (
        "unborn_head",
        "git_state::classify (27-01, narrowed in 27-17)",
    ),
    ("mid_operation", "git_state::classify (27-01)"),
    ("locked", "git_state::classify (27-01)"),
    ("no_remote", "git_state::classify (27-01)"),
    ("multiple_remotes", "git_state::classify (27-01)"),
    ("no_upstream", "git_state::classify (27-01)"),
    ("shallow", "git_state::classify (27-01)"),
    ("alternates", "git_state::classify (27-01)"),
    (
        "remote_mismatch",
        "prepare_git_live via check_release_checkout (27-12)",
    ),
    (
        "release_not_on_branch",
        "prepare_git_live via gate_node (27-12)",
    ),
    ("branch_required", "adoption in prepare_git_host (27-17)"),
    (
        "nodes_disagree",
        "prepare_git_pair, agreement of the two nodes' repositories (27-22)",
    ),
    (
        "restore_failed",
        "git_handover::restore_and_advance (27-02)",
    ),
    (
        "advance_failed",
        "git_handover::restore_and_advance (27-02)",
    ),
    (
        "status_unexpected",
        "git_handover::restore_and_advance via finish_advance (27-01, 27-02)",
    ),
    ("git_recreated", "git_handover::restore_and_advance (27-02)"),
    (
        "git_changed",
        "git_handover::restore_and_advance, write under lock (27-45)",
    ),
    (
        "backup_unsynced",
        "git_after_backup, the wait for the backup's project (27-22)",
    ),
    (
        "post_status_mismatch",
        "git_after_verify, post-advance disk status on a local node (27-12, D-25)",
    ),
    (
        "designer_attached",
        "git_restore_designer_gate (27-13, D-07), called before every post-import .git write; 27-21 adds git_after_rollback",
    ),
    (
        "backup_project_json_missing",
        "advance_after and settle_backup_git, the backup's project.json check after it converged (27-37)",
    ),
];

/// The words [`REASON_PRODUCERS`] names this module as the producer of.
pub const PRODUCED_HERE: &[&str] = &[
    "remote_mismatch",
    "release_not_on_branch",
    "post_status_mismatch",
    "designer_attached",
    "backup_project_json_missing",
];

/// The restore reason of a restore that was not attempted because a Designer
/// was attached (D-07, checker W-4).
pub const REASON_DESIGNER_ATTACHED: &str = "designer_attached";

/// `nodes.master`, the first entry of `data.git.nodes`.
pub const MASTER: &str = "nodes.master";
/// `nodes.backup`, the second entry on a redundant pair (27-22).
pub const BACKUP: &str = "nodes.backup";

/// The restore reason of a backup whose project did not converge to the
/// artifact within `--backup-timeout` (27-22, D-G5).
pub const REASON_BACKUP_UNSYNCED: &str = "backup_unsynced";

/// The restore reason of a backup whose `data/projects/<p>/project.json` is
/// absent or not a regular file after its project converged (27-37, user
/// decision 2026-11-15, checkpoint 27-26 option-a). Redundancy project sync
/// removes that file after the git module registered pseudo-resources on the
/// master; the backup then cannot load the project at its next restart. ign
/// only detects it: it never repairs the file and never hides the deletion
/// from the index it writes.
pub const REASON_BACKUP_PROJECT_JSON_MISSING: &str = "backup_project_json_missing";

/// The restore reason of an advance whose write found its target changed under
/// the lock (27-45, CodeRabbit #55 F4; checkpoint option-a): a Designer or the
/// module moved a ref, `HEAD`, `config` or `index` after the run read the
/// repository. The writes before it, the index first, are in place and
/// nothing was overwritten, so the repository is the other writer's: the
/// verdict never sends the operator to put the saved copy over it.
pub const REASON_GIT_CHANGED: &str = "git_changed";

/// The manifest file the finding is about, as a status path.
const PROJECT_JSON: &str = "project.json";

/// How long the Designer gate before a post-import `.git` write waits for an
/// attached session to close when the run's own `--designers` mode is not
/// `wait` (D-07, checker W-4). Under `--designers wait` the run's timeout
/// applies instead.
pub const GIT_RESTORE_DESIGNER_WAIT_S: u64 = 30;

/// The restore reason of a post-advance disk status that differs from
/// `expected_drift` (D-25).
pub const REASON_POST_STATUS_MISMATCH: &str = "post_status_mismatch";

/// The note an `ssh` node's `restore.post_status_note` carries (D-25).
pub const POST_STATUS_NOTE_SSH: &str = "D-25: no git on the gateway host (D-02); verified by the \
     pre-write status over the read-back export and the read-back of every written file";

/// Paths a reported differing list names before it counts the rest.
const SHOWN: usize = 20;

/// How a run treats what the git gates find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMode {
    /// The gates refuse (the apply and `--check`).
    Enforce,
    /// The gates only report (a later plan's rehearsal).
    Report,
}

/// How the post-advance status of a node is proved (D-25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostStatusMode {
    /// A `local` node: a status over the project directory on disk.
    Disk(PathBuf),
    /// An `ssh` node (POSIX or Windows): no git runs on the gateway host.
    NotChecked,
}

impl PostStatusMode {
    /// The mode for a node whose local data directory is `local`.
    pub fn of(local: Option<PathBuf>) -> PostStatusMode {
        match local {
            Some(dir) => PostStatusMode::Disk(dir),
            None => PostStatusMode::NotChecked,
        }
    }

    /// The `post_status` word a successful restore reports without running
    /// the disk status, and its note.
    pub fn not_checked_words(&self) -> Option<(&'static str, &'static str)> {
        match self {
            PostStatusMode::Disk(_) => None,
            PostStatusMode::NotChecked => Some(("not_checked", POST_STATUS_NOTE_SSH)),
        }
    }
}

/// What a saved `.git` holds, as published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaveInfo {
    /// Where the copy is, relative to the node's ledger root.
    pub path: String,
    /// Regular files in the copy.
    pub files: usize,
    /// Their total size in bytes.
    pub bytes: u64,
}

/// What the restore and the advance did, as published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreInfo {
    /// The save was copied back because the import had deleted `.git`.
    pub restored: bool,
    /// The advance was written.
    pub advanced: bool,
    /// Every written file was read back and equals the planned bytes.
    pub verified: bool,
    /// `matched` or `mismatch` on a `local` node, `not_checked` on an `ssh`
    /// node; null when the run stopped before the advance (D-25).
    pub post_status: Option<&'static str>,
    /// Null, or why the status was not checked (citing D-25), or the paths a
    /// mismatch named.
    pub post_status_note: Option<String>,
    /// In-scope artifact members the gateway did not keep (assume-unchanged,
    /// checker B3), byte-sorted, capped at 200.
    pub dropped: Vec<String>,
    /// The full number of dropped members.
    pub dropped_count: usize,
    /// Null on success; else one of the restore reason words.
    pub reason: Option<String>,
    /// The save is still on the host.
    pub save_kept: bool,
}

/// One node's git facts in `data.git.nodes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeGitReport {
    /// `nodes.master` or `nodes.backup`.
    pub node: String,
    /// `absent`, `managed` or `adoptable`.
    pub state: &'static str,
    /// Why the state is not managed, when it names a reason.
    pub reason: Option<String>,
    /// The tracked branch.
    pub branch: Option<String>,
    /// `config`, `git_yaml` or `target`.
    pub branch_source: Option<&'static str>,
    /// The remote's name in the gateway repo.
    pub remote: Option<String>,
    /// The remote's URL without userinfo.
    pub remote_url: Option<String>,
    /// The gateway HEAD commit.
    pub head: Option<String>,
    /// The repo was adopted (27-17) rather than managed.
    pub adopted: bool,
    /// Commit ids on no remote branch, newest first, capped.
    pub unpushed: Vec<String>,
    /// `R..G` commit ids, newest first, capped.
    pub not_merged: Vec<String>,
    /// The full size of `R..G`.
    pub not_merged_count: usize,
    /// Members the live project changed since the gateway HEAD (D-24: always
    /// a list for a managed node), byte-sorted and capped at 200.
    pub dirty: Option<Vec<String>>,
    /// The full number of dirty members; null with `dirty`.
    pub dirty_count: Option<usize>,
    /// `release_store` or `gateway_objects` (D-24); null with `dirty`.
    pub dirty_source: Option<&'static str>,
    /// Sync hazards found in `git.yaml` (27-18).
    pub hazards: Vec<String>,
    /// The host-local save, once taken.
    pub save: Option<SaveInfo>,
    /// The restore and advance, once attempted.
    pub restore: Option<RestoreInfo>,
}

/// `data.git`: the published block for a project the git module manages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitReport {
    /// R: the commit the release tag peels to.
    pub release_commit: String,
    /// One entry per node, `master` first.
    pub nodes: Vec<NodeGitReport>,
    /// The paths the module will list as uncommitted after the deploy: null
    /// until a run advanced the master (W-6), then byte-sorted and capped.
    pub expected_drift: Option<Vec<String>>,
    /// The full number of drift paths; null with `expected_drift`.
    pub expected_drift_count: Option<usize>,
    /// True once `expected_drift` is the post-import prediction.
    pub expected_drift_complete: bool,
    /// What `--discard-gateway-changes` would discard (27-15, D-05): reported
    /// by a `--check` and by the plan; null when the override is not given or
    /// nothing would go. An apply's result carries `discarded` instead.
    pub would_discard: Option<WouldDiscard>,
    /// What the run discarded (27-15, D-05); null when nothing was.
    pub discarded: Option<DiscardedReport>,
    /// The `--allow-git-sync-hazards` reason (27-18); null until then.
    pub hazards_acknowledged: Option<String>,
}

impl GitReport {
    /// The master's entry.
    pub fn master(&self) -> &NodeGitReport {
        &self.nodes[0]
    }

    /// The master's entry, mutable.
    pub fn master_mut(&mut self) -> &mut NodeGitReport {
        &mut self.nodes[0]
    }

    /// Record a finished save on the master's entry.
    pub fn set_save(&mut self, saved: &GitSaveReport) {
        self.master_mut().save = Some(SaveInfo {
            path: saved.path.clone(),
            files: saved.manifest.files,
            bytes: saved.manifest.bytes,
        });
    }
}

/// Whether the dirty list of a discard names paths (D-05, D-24): the word is
/// `none` for a clean repo and `listed` when the paths are in the list. No
/// third word exists: the paths are always known, from ign's store or from
/// the gateway's own objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscardDirty {
    /// No live change since the gateway HEAD is lost.
    None,
    /// The paths of the live changes are listed.
    Listed,
}

/// `data.git.would_discard`: what the override passes, before anything is
/// written (27-15, D-05). Paths and commit ids only: never a commit message,
/// file content or URL userinfo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WouldDiscard {
    /// `none` or `listed`.
    pub dirty: DiscardDirty,
    /// The live paths that changed since the gateway HEAD, byte-sorted and
    /// capped at 200.
    pub paths: Vec<String>,
    /// The full number of dirty paths.
    pub paths_count: usize,
    /// The commits the branch move abandons: unpushed ones, then the ones not
    /// merged into the release (newest first, capped).
    pub commits: Vec<String>,
    /// The run-directory file that holds the live content (the pre-image
    /// export), relative to the run directory.
    pub evidence: String,
}

/// `data.git.discarded`: what the run discarded (27-15, D-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscardedReport {
    /// The operator's reason (operator text: data, never a command).
    pub reason: String,
    /// `none` or `listed`.
    pub dirty: DiscardDirty,
    /// The live paths the import replaced, as in `would_discard`.
    pub paths: Vec<String>,
    /// The full number of dirty paths.
    pub paths_count: usize,
    /// The commits the advance abandoned; empty until the advance wrote.
    pub commits: Vec<String>,
    /// `refs/ign-discarded/<run-id>` once the advance wrote it, else null.
    #[serde(rename = "ref")]
    pub keep_ref: Option<String>,
    /// The run-directory file holding the live content, relative to the run
    /// directory.
    pub evidence: String,
}

/// What an apply with `--discard-gateway-changes` discards, decided in
/// [`prepare_git_live`] (27-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardPlan {
    /// The operator's reason.
    pub reason: String,
    /// The tracked branch.
    pub branch: Option<String>,
    /// The gateway HEAD before the run. The keep-alive ref names it when
    /// commits are abandoned; it is always known then.
    pub abandoned_head: Option<String>,
    /// What is reported beforehand.
    pub would: WouldDiscard,
}

/// The keep-alive ref's name for a run.
pub fn discard_ref_name(run_id: &str) -> String {
    format!("refs/ign-discarded/{run_id}")
}

/// Where an adoption's branch came from, and which branch it is (27-17, D-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adoption {
    /// The branch the partial repo is adopted onto.
    pub branch: String,
    /// `git_yaml` or `target`; there is no implicit source.
    pub source: &'static str,
}

/// What `git.yaml` says about the module's start sync for one node (27-18,
/// D-21): words and flags only, never a value of the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HazardFacts {
    /// The flags `git.yaml` sets true for the project, in [`Hazard`] order.
    pub found: Vec<Hazard>,
    /// `tags/` exists below the project directory on the node (D-G3). Probed
    /// only when a tag import flag is set; false otherwise.
    pub tags_present: bool,
    /// Why the hazards are unknown: `absent` (no `git.yaml`) or `unreadable`
    /// (it could not be read or parsed). `None` when the file was read.
    pub unknown: Option<&'static str>,
}

/// Everything the prepare learned about one managed gateway repo.
#[derive(Debug, Clone)]
pub struct PreparedGit {
    /// How the gates are applied.
    pub mode: GitMode,
    /// The repo as classified at the prepare.
    pub state: GitState,
    /// Its refs as parsed at the prepare.
    pub refs: GitRefs,
    /// The release checkout that passed its checks.
    pub checkout: ReleaseCheckout,
    /// The remote's heads, with their objects in ign's store.
    pub view: RemoteView,
    /// The advance input, built before any write; `None` only for a state
    /// that will not be handed over.
    pub prep: Option<AdvancePrep>,
    /// The gate facts, once [`prepare_git_live`] ran. `None` for an adopted
    /// repo: it holds no commit, so there is nothing to gate (D-13).
    pub gates: Option<NodeGates>,
    /// The branch and its source when the repo is adopted (27-17); `None` for
    /// a managed repo.
    pub adopt: Option<Adoption>,
    /// The `--discard-gateway-changes` reason, when given (27-15).
    pub discard_reason: Option<String>,
    /// The module start-sync hazards `git.yaml` sets for the project (27-18).
    pub hazards: HazardFacts,
    /// The `--allow-git-sync-hazards` reason, when given (27-18).
    pub hazard_ack: Option<String>,
    /// What the override discards, once [`prepare_git_live`] found something
    /// the gates would otherwise refuse; `None` otherwise (27-15).
    pub discard: Option<DiscardPlan>,
    /// The published block.
    pub report: GitReport,
    /// The node this repo belongs to: `nodes.master` or `nodes.backup`.
    pub node: String,
    /// That node's ledger root ([`LedgerRoot::for_node`]): where its `.git`
    /// save is kept, so where the reports say it is.
    pub root: LedgerRoot,
    /// The `.git` entry kind (`directory`, ...), for refusals.
    kind: String,
    /// The gateway repo's `logs/HEAD` tail (untrusted text).
    head_log_tail: Option<Vec<u8>>,
}

fn preflight(check: &str, detail: impl Into<String>) -> CoreError {
    CoreError::DeployPreflightFailed {
        failures: Box::new([PreflightFailure {
            check: check.to_string(),
            detail: detail.into(),
        }]),
    }
}

/// The host read's refusal of `git.yaml` (`deploy_run_invalid` naming
/// `git.yaml`) as the preflight failure of the adoption; any other error (an
/// unreadable host is `transport_failed`) is unchanged.
fn git_yaml_unreadable(err: CoreError) -> CoreError {
    match err {
        CoreError::DeployRunInvalid { problem, .. } => {
            let detail = if problem.starts_with("git.yaml") {
                problem
            } else {
                format!("git.yaml: {problem}")
            };
            preflight(GIT_YAML_CHECK, detail)
        }
        other => other,
    }
}

fn managed(project: &str, kind: &str, reason: &str) -> CoreError {
    CoreError::ProjectGitManaged {
        project: project.to_string(),
        location: git_entry_location(project),
        kind: kind.to_string(),
        reason: reason.to_string(),
    }
}

/// Map a gate failure to its refusal (redacted details only).
fn gate_failure(err: GitGateFailure, project: &str, kind: &str) -> CoreError {
    match err {
        GitGateFailure::HostGit { detail } => preflight("git_host", detail),
        GitGateFailure::Checkout { detail } => preflight("git_checkout", detail),
        GitGateFailure::RemoteUnreachable { detail, .. } => preflight("git_remote", detail),
        GitGateFailure::GatewayObjects { detail } => preflight("git_gateway_objects", detail),
        GitGateFailure::RemoteMismatch { .. } => managed(project, kind, "remote_mismatch"),
    }
}

/// What a read of the gateway repo showed.
pub struct StateRead {
    /// The classified state.
    pub state: GitState,
    /// Its refs; `None` when `HEAD` is absent or unparseable.
    pub refs: Option<GitRefs>,
    /// The raw files.
    pub files: GitFiles,
}

/// Read and classify the gateway repo through the host's git surface.
pub async fn read_state(
    read: &dyn GitHostRead,
    git: &HostGit,
    project: &str,
) -> Result<StateRead, CoreError> {
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
    Ok(StateRead { state, refs, files })
}

fn state_word(kind: GitStateKind) -> &'static str {
    match kind {
        GitStateKind::Absent => "absent",
        GitStateKind::Managed => "managed",
        GitStateKind::Adoptable => "adoptable",
        GitStateKind::Unsupported => "unsupported",
    }
}

fn head_commit(state: &GitState) -> Option<String> {
    match state.head.as_ref()? {
        GitHead::Branch { commit, .. } => commit.clone(),
        GitHead::Detached { commit } => Some(commit.clone()),
    }
}

/// The reason word a refusal of `state` carries.
fn reason_of(state: &GitState) -> String {
    state
        .reason
        .clone()
        .unwrap_or_else(|| state_word(state.kind).to_string())
}

/// The reason word for the `--rollback-run` refusal of a present `.git`
/// (27-12): the classified state word, or the entry's kind when the repo
/// cannot be read.
pub async fn refusal_reason(surfaces: &ProjectsSurfaces<'_>, project: &str, kind: &str) -> String {
    let Some(read) = surfaces.host.git() else {
        return kind.to_string();
    };
    match read_state(read, &surfaces.host.host_git(), project).await {
        Ok(found) => reason_of(&found.state),
        Err(_) => kind.to_string(),
    }
}

/// Step 1: read, classify and prepare the advance of the gateway repo.
///
/// `None` when the entry is absent: no host git and no network call is made.
/// A `.git` the deploy cannot hand over refuses `project_git_managed` with
/// the state's reason; a missing host git, a checkout that is not at the
/// release, an unreadable remote or an advance that cannot be built refuses
/// `deploy_preflight_failed` (checks `git_host`, `git_checkout`,
/// `git_remote`). Nothing is written.
///
/// In [`GitMode::Report`] (a rehearsal, 27-20) a refusal that is not a
/// transport failure is not returned: it comes back as
/// [`HostPrep::Refused`], a report that names the refusal word, and a warning
/// in `findings` that says the apply would refuse.
pub async fn prepare_git_host_with(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    entry: GitEntry,
    mode: GitMode,
    findings: &mut Vec<String>,
) -> Result<HostPrep, CoreError> {
    let mut seen = HostSeen::default();
    let git = surfaces.host.host_git();
    match prepare_git_host_inner(
        loaded,
        input,
        surfaces.host,
        git,
        entry,
        mode,
        &mut seen,
        MASTER,
        LedgerRoot::for_node(&loaded.target.nodes.master),
    )
    .await
    {
        Ok(Some(prepared)) => Ok(HostPrep::Ready(Box::new(prepared))),
        Ok(None) => Ok(HostPrep::Absent),
        Err(err) if mode == GitMode::Report && is_refusal(&err) => {
            let project = loaded.file.project.as_str();
            let (word, text) = describe_refusal(&err);
            findings.push(format!("project {project}: the apply would refuse: {text}"));
            Ok(HostPrep::Refused(Box::new(seen.report(word))))
        }
        Err(err) => Err(err),
    }
}

/// [`prepare_git_host_with`] in [`GitMode::Enforce`]: every refusal is
/// returned.
pub async fn prepare_git_host(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    entry: GitEntry,
) -> Result<Option<PreparedGit>, CoreError> {
    match prepare_git_host_with(
        loaded,
        input,
        surfaces,
        entry,
        GitMode::Enforce,
        &mut Vec::new(),
    )
    .await?
    {
        HostPrep::Ready(prepared) => Ok(Some(*prepared)),
        HostPrep::Absent | HostPrep::Refused(_) => Ok(None),
    }
}

/// What [`prepare_git_host_with`] decided.
#[derive(Debug, Clone)]
pub enum HostPrep {
    /// The master holds no `.git`: no host git and no network call was made.
    Absent,
    /// The repo was read, gated and prepared.
    Ready(Box<PreparedGit>),
    /// Report mode only: the prepare would have refused. What was learned
    /// before the refusal, with the refusal word in `nodes[0].reason`.
    Refused(Box<GitReport>),
}

/// What the host half of the prepare learned before it stopped (report mode
/// builds the refused report from it).
#[derive(Debug, Default)]
struct HostSeen {
    state: Option<&'static str>,
    branch: Option<String>,
    remote: Option<String>,
    remote_url: Option<String>,
    head: Option<String>,
    release_commit: Option<String>,
}

impl HostSeen {
    fn report(self, word: String) -> GitReport {
        GitReport {
            release_commit: self.release_commit.unwrap_or_default(),
            nodes: vec![NodeGitReport {
                node: "nodes.master".to_string(),
                state: self.state.unwrap_or("managed"),
                reason: Some(word),
                branch: self.branch,
                branch_source: None,
                remote: self.remote,
                remote_url: self.remote_url,
                head: self.head,
                adopted: false,
                unpushed: Vec::new(),
                not_merged: Vec::new(),
                not_merged_count: 0,
                dirty: None,
                dirty_count: None,
                dirty_source: None,
                hazards: Vec::new(),
                save: None,
                restore: None,
            }],
            expected_drift: None,
            expected_drift_count: None,
            expected_drift_complete: false,
            would_discard: None,
            discarded: None,
            hazards_acknowledged: None,
        }
    }
}

/// Whether `err` is a git refusal a rehearsal reports instead of returning:
/// a reasoned `project_git_managed`, a gate, or a failed preflight check. A
/// transport failure and everything else is returned in every mode.
fn is_refusal(err: &CoreError) -> bool {
    matches!(
        err,
        CoreError::ProjectGitManaged { .. }
            | CoreError::DestructiveChangeRefused { .. }
            | CoreError::DeployPreflightFailed { .. }
    )
}

/// The word a refusal is known by (the gate, the reason or the check) and the
/// sentence that names it. Counts and words only; a preflight detail is
/// already redacted.
fn describe_refusal(err: &CoreError) -> (String, String) {
    match err {
        CoreError::DestructiveChangeRefused { gate, count, .. } => (
            gate.clone(),
            format!("destructive_change_refused, gate {gate} ({count})"),
        ),
        CoreError::ProjectGitManaged { reason, .. } => (
            reason.clone(),
            format!("project_git_managed, reason {reason}"),
        ),
        CoreError::DeployPreflightFailed { failures } => {
            let check = failures
                .first()
                .map_or_else(|| "git_host".to_string(), |f| f.check.clone());
            let detail = crate::deploy::git_gate::detail_of(err);
            (
                check.clone(),
                format!("deploy_preflight_failed, check {check}: {detail}"),
            )
        }
        other => (other.code().to_string(), other.code().to_string()),
    }
}

/// A refusal the apply would give: returned in enforce mode, recorded in
/// report mode (a warning, and the word in the master's `reason`).
fn refuse(
    prepared: &mut PreparedGit,
    warnings: &mut Vec<String>,
    project: &str,
    err: CoreError,
) -> Result<(), CoreError> {
    if prepared.mode == GitMode::Enforce {
        return Err(err);
    }
    record_finding(prepared, warnings, project, &err);
    Ok(())
}

fn record_finding(
    prepared: &mut PreparedGit,
    warnings: &mut Vec<String>,
    project: &str,
    err: &CoreError,
) {
    let (word, text) = describe_refusal(err);
    warnings.push(format!("project {project}: the apply would refuse: {text}"));
    let node = prepared.report.master_mut();
    if node.reason.is_none() {
        node.reason = Some(word);
    }
}

#[allow(clippy::too_many_arguments)]
async fn prepare_git_host_inner(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    host: &dyn ProjectHost,
    git: HostGit,
    entry: GitEntry,
    git_mode: GitMode,
    seen: &mut HostSeen,
    node: &str,
    root: LedgerRoot,
) -> Result<Option<PreparedGit>, CoreError> {
    if !entry.is_present() {
        return Ok(None);
    }
    let project = loaded.file.project.as_str();
    let kind = entry.as_str();
    let Some(read) = host.git() else {
        return Err(CoreError::TransportFailed {
            operation: "git".to_string(),
            node: node.to_string(),
            detail: format!(
                "{} is present but this host cannot read it",
                git_entry_location(project)
            ),
        });
    };
    let found = read_state(read, &git, project).await?;
    seen.state = Some(state_word(found.state.kind));
    seen.branch.clone_from(&found.state.tracked);
    seen.head = head_commit(&found.state);
    if let Some(remote) = &found.state.remote {
        seen.remote = Some(remote.name.clone());
        seen.remote_url = Some(remote.url_redacted.clone());
    }
    let adoptable = found.state.kind == GitStateKind::Adoptable;
    if found.state.kind != GitStateKind::Managed && !adoptable {
        return Err(managed(project, kind, &reason_of(&found.state)));
    }
    let (Some(refs), Some(remote)) = (found.refs.clone(), found.state.remote.clone()) else {
        return Err(managed(project, kind, "no_head"));
    };
    git.version()
        .await
        .map_err(|e| preflight("git_host", crate::deploy::git_gate::detail_of(&e)))?;
    let checkout = check_release_checkout(&git, &input.repo, &input.release, &remote.url_key)
        .await
        .map_err(|e| gate_failure(e, project, kind))?;
    seen.release_commit = Some(checkout.commit.clone());
    // The adoption branch is decided from an explicit source before the remote
    // is asked anything: JGit's default `master` in an unborn HEAD is never
    // taken as intent (D-23, T-27-39).
    let adopt = if adoptable {
        Some(adoption_branch(loaded, read, &git, project, kind).await?)
    } else {
        None
    };
    let view = remote_view(&git, &checkout)
        .await
        .map_err(|e| gate_failure(e, project, kind))?;
    let members: BTreeSet<String> = loaded.artifact.digest.members.keys().cloned().collect();
    let mode = match (&adopt, &found.files.config) {
        (Some(adopt), Some(config)) => AdvanceMode::Adopt {
            branch: adopt.branch.clone(),
            remote: remote.name.clone(),
            config: config.clone(),
        },
        (Some(_), None) => {
            return Err(managed(project, kind, "no_remote"));
        }
        (None, _) => AdvanceMode::FastForward,
    };
    // B5: the pack, its base, the release blobs and (for an adoption) the
    // adopted config are built here, before any write, in `--check` and in the
    // apply alike.
    let prep = prepare_advance_with(
        &git,
        &input.repo,
        &found.state,
        &refs,
        &checkout.commit,
        &members,
        mode,
    )
    .await
    .map_err(|e| preflight("git_host", crate::deploy::git_gate::detail_of(&e)))?;
    let hazards = read_hazards(read, project).await?;
    let report = GitReport {
        release_commit: checkout.commit.clone(),
        nodes: vec![NodeGitReport {
            node: node.to_string(),
            state: state_word(found.state.kind),
            reason: None,
            branch: match &adopt {
                Some(adopt) => Some(adopt.branch.clone()),
                None => found.state.tracked.clone(),
            },
            branch_source: Some(adopt.as_ref().map_or("config", |a| a.source)),
            remote: Some(remote.name.clone()),
            remote_url: Some(remote.url_redacted.clone()),
            head: head_commit(&found.state),
            // True once the advance wrote (`advance_after`): a plan that has
            // not run, or a run that stopped, adopted nothing.
            adopted: false,
            unpushed: Vec::new(),
            not_merged: Vec::new(),
            not_merged_count: 0,
            dirty: None,
            dirty_count: None,
            dirty_source: None,
            hazards: hazards.found.iter().map(|h| h.word().to_string()).collect(),
            save: None,
            restore: None,
        }],
        expected_drift: None,
        expected_drift_count: None,
        expected_drift_complete: false,
        would_discard: None,
        discarded: None,
        hazards_acknowledged: None,
    };
    Ok(Some(PreparedGit {
        mode: git_mode,
        state: found.state,
        refs,
        checkout,
        view,
        prep: Some(prep),
        gates: None,
        adopt,
        discard_reason: input.discard_gateway_changes.clone(),
        hazards,
        hazard_ack: input.allow_git_sync_hazards.clone(),
        discard: None,
        report,
        node: node.to_string(),
        root,
        kind: kind.to_string(),
        head_log_tail: found.files.head_log_tail,
    }))
}

/// The branch a partial repo is adopted onto and where it came from (D-23):
/// the module's `git.yaml` `repo_branch` for the project, else the target's
/// `git_branch`, else the run refuses `project_git_managed` reason
/// `branch_required`. No other source exists: the unborn `HEAD` of a partial
/// repo names JGit's default (`master`), which is never intent.
///
/// `git.yaml` is untrusted data that holds a credential: it is read with its
/// size cap, parsed through the allow-list ([`parse_git_yaml`]) and dropped; no
/// value of it reaches a message. The chosen name must pass ign's pure branch
/// check and git's own (T-27-38).
async fn adoption_branch(
    loaded: &LoadedProject,
    read: &dyn GitHostRead,
    git: &HostGit,
    project: &str,
    kind: &str,
) -> Result<Adoption, CoreError> {
    // A git.yaml the host read refuses (a symlink, a non-regular file, over the
    // cap) and one that does not parse are the same refusal: a preflight
    // failure with the check word `git_yaml`. No run exists and no run record
    // is damaged, which is what a `deploy_run_invalid` hint would say (27-26,
    // WR-A4). The text names `git.yaml` and why, never a value of the file.
    let entries = match read.read_git_yaml().await.map_err(git_yaml_unreadable)? {
        Some(bytes) => parse_git_yaml(&bytes)?,
        None => Vec::new(),
    };
    let from_yaml = entry_for(&entries, project)
        .and_then(|e| e.repo_branch.as_deref())
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let (branch, source, label) = match (from_yaml, loaded.target.git_branch.as_deref()) {
        (Some(branch), _) => (branch.to_string(), "git_yaml", "git.yaml repo_branch"),
        (None, Some(branch)) => (branch.to_string(), "target", "the target's git_branch"),
        (None, None) => return Err(managed(project, kind, "branch_required")),
    };
    git.check_branch_name(&branch).await.map_err(|e| {
        preflight(
            "git_host",
            format!(
                "the adoption branch from {label} is not usable: {}",
                crate::deploy::git_gate::detail_of(&e)
            ),
        )
    })?;
    Ok(Adoption { branch, source })
}

/// The module start-sync hazards of one node (27-18, D-21): `git.yaml` read
/// through its allow-list for the flags the module's next start would act on
/// ([`hazards_for`]), and, only when a tag import flag is set, whether `tags/`
/// is present below the project directory (D-G3: the next start overwrites the
/// gateway's tag providers only when there is a `tags/` to import).
///
/// A `git.yaml` that is absent, cannot be read or does not parse leaves the
/// hazards unknown: a warning, never a refusal, and never an echo of the file
/// (its error text names `git.yaml` and at most a line). A failed probe of
/// `tags/` is a transport failure (fail closed: presence is never guessed).
async fn read_hazards(read: &dyn GitHostRead, project: &str) -> Result<HazardFacts, CoreError> {
    let unknown = |why: &'static str| HazardFacts {
        unknown: Some(why),
        ..HazardFacts::default()
    };
    let entries = match read.read_git_yaml().await {
        Ok(Some(bytes)) => match parse_git_yaml(&bytes) {
            Ok(entries) => entries,
            Err(_) => return Ok(unknown("unreadable")),
        },
        Ok(None) => return Ok(unknown("absent")),
        Err(_) => return Ok(unknown("unreadable")),
    };
    let found = hazards_for(&entries, project);
    let tags_present = if found.contains(&Hazard::ImportTags) {
        read.project_dir_entry(project, ImportDir::Tags)
            .await?
            .is_present()
    } else {
        false
    };
    Ok(HazardFacts {
        found,
        tags_present,
        unknown: None,
    })
}

/// The warning that reports a hazard that does not refuse (27-18). Fixed text
/// and words: no value of `git.yaml` and no operator text.
fn hazard_warning(project: &str, hazard: Hazard, tags_present: bool) -> String {
    let word = hazard.word();
    match hazard {
        Hazard::ImportTags if !tags_present => format!(
            "project {project}: git.yaml sets {word}; tags/ is absent on the node, so the git \
             module's next start imports no tags"
        ),
        _ => format!(
            "project {project}: git.yaml sets {word}; at its next start the git module will {}",
            hazard.effect()
        ),
    }
}

/// Report the hazards and refuse the damaging ones (27-18, D-21, D-G3).
///
/// Every flag `git.yaml` sets is in `data.git.nodes[].hazards` (the prepare
/// put the words there). Of those, the ones G3 recorded damage for
/// ([`refuses`]) refuse `destructive_change_refused` gate `git_sync_hazard`
/// (keys: the hazard words) unless `--allow-git-sync-hazards <reason>` is
/// given; the reason is then recorded in `data.git.hazards_acknowledged`. The
/// others, and the unknown state of an absent or unreadable `git.yaml`, are
/// warnings. Only an `import` or a `record` hands the kept `.git` over, so a
/// `skip` warns and never refuses; in report mode the refusal is recorded (a
/// warning, 27-20).
fn apply_hazards(
    prepared: &mut PreparedGit,
    action: ProjectAction,
    project: &str,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    if let Some(why) = prepared.hazards.unknown {
        warnings.push(format!(
            "project {project}: git.yaml is {why}, so the git module's start-sync hazards are \
             unknown"
        ));
    }
    let tags = prepared.hazards.tags_present;
    let (refusing, advisory): (Vec<Hazard>, Vec<Hazard>) = prepared
        .hazards
        .found
        .iter()
        .copied()
        .partition(|h| refuses(*h, tags));
    for hazard in &advisory {
        warnings.push(hazard_warning(project, *hazard, tags));
    }
    if refusing.is_empty() {
        return Ok(());
    }
    let enforce = matches!(action, ProjectAction::Import | ProjectAction::Record);
    if !enforce {
        for hazard in &refusing {
            warnings.push(hazard_warning(project, *hazard, tags));
        }
        return Ok(());
    }
    match &prepared.hazard_ack {
        Some(reason) => {
            for hazard in &refusing {
                warnings.push(format!(
                    "{} (acknowledged with --allow-git-sync-hazards)",
                    hazard_warning(project, *hazard, tags)
                ));
            }
            prepared.report.hazards_acknowledged = Some(reason.clone());
            Ok(())
        }
        None => {
            // A rehearsal reports the hazards it names and what the apply
            // would do about them; the apply refuses.
            if prepared.mode == GitMode::Report {
                for hazard in &refusing {
                    warnings.push(hazard_warning(project, *hazard, tags));
                }
            }
            refuse(
                prepared,
                warnings,
                project,
                CoreError::DestructiveChangeRefused {
                    gate: "git_sync_hazard".to_string(),
                    count: refusing.len(),
                    keys: refusing.iter().map(|h| h.word().to_string()).collect(),
                },
            )
        }
    }
}

/// What `--discard-gateway-changes <reason>` discards, from the gates' facts
/// (27-15, D-05): the dirty paths (itemised for every managed node, D-24), the
/// unpushed commits, then the commits not merged into the release. The pre-image
/// export is the evidence of the live content. A `record` leaves the project
/// as it is, so it discards commits only.
///
/// Fails closed with `deploy_preflight_failed` check `git_gateway_objects`
/// when an import's dirty list was never itemised, and with `git_host` when
/// commits would be abandoned but the gateway HEAD that keeps them reachable
/// is unknown: nothing is discarded that cannot be listed and kept.
fn discard_plan(
    reason: &str,
    gates: &NodeGates,
    action: ProjectAction,
    project: &str,
    state: &GitState,
) -> Result<DiscardPlan, CoreError> {
    let (paths, paths_count) = if action == ProjectAction::Import {
        let Some(list) = gates.dirty.clone() else {
            return Err(preflight(
                "git_gateway_objects",
                "the live changes since the gateway HEAD could not be itemised, so they cannot \
                 be listed before they are discarded",
            ));
        };
        (list, gates.dirty_count)
    } else {
        (Vec::new(), 0)
    };
    let mut commits = gates.unpushed.clone();
    for id in &gates.not_merged {
        if !commits.contains(id) {
            commits.push(id.clone());
        }
    }
    let abandoned_head = gates.head.clone();
    if !commits.is_empty() && abandoned_head.is_none() {
        return Err(preflight(
            "git_host",
            "the gateway HEAD commit is unknown, so the commits the override would discard \
             cannot be kept reachable",
        ));
    }
    Ok(DiscardPlan {
        reason: reason.to_string(),
        branch: state.tracked.clone(),
        abandoned_head,
        would: WouldDiscard {
            dirty: if paths_count == 0 {
                DiscardDirty::None
            } else {
                DiscardDirty::Listed
            },
            paths,
            paths_count,
            commits,
            evidence: format!("export/{project}.zip"),
        },
    })
}

/// Step 2: the gate facts against the live project, and the refusals.
///
/// The dirty list is itemised for every managed node: from ign's store when
/// it holds the gateway HEAD, else from the gateway's own objects (D-24); a
/// failure to read them refuses `deploy_preflight_failed` check
/// `git_gateway_objects`. An `import` is gated: an unpushed, unmerged or
/// dirty repo refuses `destructive_change_refused` (gates `git_unpushed`,
/// `git_not_merged`, `git_dirty`) and a release off the tracked branch
/// refuses `project_git_managed` reason `release_not_on_branch`. A `record`
/// (which advances the repo in place, 27-13) is refused for the same
/// unpushed, not-merged and off-branch findings but not for dirty work. A
/// `skip` whose HEAD is not the release adds a warning. `expected_drift` stays null
/// (W-6).
///
/// With `--discard-gateway-changes <reason>` the three gates `git_unpushed`,
/// `git_not_merged` and `git_dirty` do not refuse: they become the
/// [`WouldDiscard`] of `data.git` and a [`DiscardPlan`] the run acts on
/// (27-15, D-05). `release_not_on_branch`, `project_git_managed` and every
/// preflight failure still refuse (D-11).
///
/// In [`GitMode::Report`] every refusal above except a transport failure is
/// recorded (a warning that the apply would refuse, and the word in the
/// master's `reason`) and the facts stay in `data.git` (27-20).
pub async fn prepare_git_live(
    prepared: &mut PreparedGit,
    live: Option<&CanonicalDigest>,
    action: ProjectAction,
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    let git = surfaces.host.host_git();
    prepare_git_live_on(prepared, live, action, loaded, surfaces.host, git, warnings).await
}

/// [`prepare_git_live`] over an explicit host: the backup node's own read
/// surface (27-22, the 27-18 hand-off: hazards and the D-24 object read go
/// through the node that holds the repository) with ign's host git.
pub async fn prepare_git_live_on(
    prepared: &mut PreparedGit,
    live: Option<&CanonicalDigest>,
    action: ProjectAction,
    loaded: &LoadedProject,
    host: &dyn ProjectHost,
    git: HostGit,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    match prepare_git_live_inner(prepared, live, action, loaded, host, git, warnings).await {
        Err(err) if prepared.mode == GitMode::Report && is_refusal(&err) => {
            record_finding(prepared, warnings, loaded.file.project.as_str(), &err);
            Ok(())
        }
        other => other,
    }
}

async fn prepare_git_live_inner(
    prepared: &mut PreparedGit,
    live: Option<&CanonicalDigest>,
    action: ProjectAction,
    loaded: &LoadedProject,
    host: &dyn ProjectHost,
    git: HostGit,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    let project = loaded.file.project.as_str();
    let kind = prepared.kind.clone();
    let Some(live) = live else {
        return Err(preflight(
            "git_host",
            format!(
                "the gateway does not list project {project} but its directory holds a git \
                 repository; the project cannot be deployed over it"
            ),
        ));
    };
    let Some(read) = host.git() else {
        return Err(CoreError::TransportFailed {
            operation: "git".to_string(),
            node: prepared.node.clone(),
            detail: format!(
                "{} is present but this host cannot read it",
                git_entry_location(project)
            ),
        });
    };
    // The module start-sync hazards of the kept `.git` (27-18): `git.yaml` and
    // `tags/` were read in the prepare; the refusal needs the action.
    apply_hazards(prepared, action, project, warnings)?;
    if let Some(adopt) = prepared.adopt.clone() {
        return prepare_adoption_live(prepared, &adopt, action, project, &git, warnings).await;
    }
    let mut gates = gate_node(
        &git,
        &prepared.checkout,
        &prepared.view,
        &prepared.state,
        prepared.head_log_tail.as_deref(),
        live,
    )
    .await
    .map_err(|e| gate_failure(e, project, &kind))?;
    dirty_from_gateway(
        &git,
        &prepared.checkout.path,
        read,
        project,
        &mut gates,
        live,
    )
    .await
    .map_err(|e| gate_failure(e, project, &kind))?;

    let node = prepared.report.master_mut();
    node.unpushed.clone_from(&gates.unpushed);
    node.not_merged.clone_from(&gates.not_merged);
    node.not_merged_count = gates.not_merged_count;
    node.dirty.clone_from(&gates.dirty);
    node.dirty_count = gates.dirty.as_ref().map(|_| gates.dirty_count);
    node.dirty_source = gates.dirty_source.map(|s| s.as_str());
    let head = gates.head.clone();
    let release = prepared.checkout.commit.clone();

    if action == ProjectAction::Skip && head.as_deref() != Some(release.as_str()) {
        warnings.push(format!(
            "project {project}: the gateway repo's HEAD is not the release commit; nothing is \
             deployed, so it was left as it is"
        ));
    }
    // An import discards what the gateway repo holds beyond the release, and
    // a `record` moves the branch to the release in place (27-13): both refuse
    // a release off the tracked branch and commits no remote has. Dirty work
    // refuses only an import: a `record` leaves the project as it is.
    let enforce = matches!(action, ProjectAction::Import | ProjectAction::Record);
    if enforce {
        // `release_not_on_branch` (like `project_git_managed` and every
        // preflight failure) has no override (D-11). In report mode the
        // refusals are recorded and the facts above stay in the report.
        if !gates.release_on_branch {
            refuse(
                prepared,
                warnings,
                project,
                managed(project, &kind, "release_not_on_branch"),
            )?;
        }
        let dirty_refuses = action == ProjectAction::Import && gates.dirty_count > 0;
        let refuses = !gates.unpushed.is_empty() || gates.not_merged_count > 0 || dirty_refuses;
        match (prepared.discard_reason.clone(), refuses) {
            // `--discard-gateway-changes` passes exactly the three C1 refusals
            // and turns them into what `--check` reports (27-15, D-05).
            (Some(reason), true) => {
                let plan = discard_plan(&reason, &gates, action, project, &prepared.state)?;
                prepared.report.would_discard = Some(plan.would.clone());
                prepared.discard = Some(plan);
            }
            // The override has nothing to pass: a clean repo is a plain run.
            (Some(_), false) => {}
            (None, _) => {
                if !gates.unpushed.is_empty() {
                    refuse(
                        prepared,
                        warnings,
                        project,
                        CoreError::DestructiveChangeRefused {
                            gate: "git_unpushed".to_string(),
                            count: gates.unpushed.len(),
                            keys: gates.unpushed.clone(),
                        },
                    )?;
                }
                if gates.not_merged_count > 0 {
                    refuse(
                        prepared,
                        warnings,
                        project,
                        CoreError::DestructiveChangeRefused {
                            gate: "git_not_merged".to_string(),
                            count: gates.not_merged_count,
                            keys: gates.not_merged.clone(),
                        },
                    )?;
                }
                if dirty_refuses {
                    refuse(
                        prepared,
                        warnings,
                        project,
                        CoreError::DestructiveChangeRefused {
                            gate: "git_dirty".to_string(),
                            count: gates.dirty_count,
                            keys: gates.dirty.clone().unwrap_or_default(),
                        },
                    )?;
                }
            }
        }
    }
    prepared.gates = Some(gates);
    Ok(())
}

/// [`prepare_git_live`] for an adopted repo (27-17, D-13): the partial repo
/// holds no commit, so there is no unpushed, not-merged or dirty fact to gate
/// and the report keeps those empty (`dirty: null`). Only the release must be
/// on the adoption branch of the remote (D-11): an import or a `record` of a
/// release the branch does not hold refuses `project_git_managed` reason
/// `release_not_on_branch`, with no override (like every other reason). A
/// `skip` adopts nothing and only warns.
async fn prepare_adoption_live(
    prepared: &mut PreparedGit,
    adopt: &Adoption,
    action: ProjectAction,
    project: &str,
    git: &HostGit,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    let release = prepared.checkout.commit.clone();
    let on_branch = match prepared.view.heads.get(&adopt.branch) {
        Some(tip) => git
            .is_ancestor(&prepared.checkout.path, &release, tip)
            .await
            .map_err(|e| preflight("git_host", crate::deploy::git_gate::detail_of(&e)))?,
        None => false,
    };
    match action {
        ProjectAction::Skip => warnings.push(format!(
            "project {project}: the gateway repo is a partial repository; nothing is deployed, so \
             it was left as it is"
        )),
        ProjectAction::Import | ProjectAction::Record if !on_branch => {
            let err = managed(project, &prepared.kind, "release_not_on_branch");
            refuse(prepared, warnings, project, err)?;
        }
        _ => {}
    }
    Ok(())
}

/// Step 3a: right before the import, read the repo again and save it.
///
/// Any change of HEAD, refs or config since the prepare is `plan_stale`
/// naming the project, with nothing saved or imported.
pub async fn git_before_import(
    prepared: &PreparedGit,
    read: &dyn GitHostRead,
    write: &dyn GitStateWriter,
    git: &HostGit,
    gateway: &str,
    project: &str,
    run_id: &str,
) -> Result<GitSaveReport, CoreError> {
    git_unchanged_since_plan(prepared, read, git, project).await?;
    save(read, write, prepared.root, gateway, project, run_id).await
}

/// The repo read again and compared with the prepare's: any change of HEAD,
/// refs or config is `plan_stale` naming the project. Run right before the
/// save, and again after a Designer wait that came after the save
/// (CodeRabbit #57: a commit made while the Designer was attached would be
/// removed by the import, and the save predates it).
pub async fn git_unchanged_since_plan(
    prepared: &PreparedGit,
    read: &dyn GitHostRead,
    git: &HostGit,
    project: &str,
) -> Result<(), CoreError> {
    let now = read_state(read, git, project).await?;
    // An adoption replaces the whole config: it is built from the bytes the
    // prepare read, so any other config is a change since the plan.
    let config_changed = prepared
        .prep
        .as_ref()
        .and_then(|prep| prep.config_base.as_deref())
        .is_some_and(|base| {
            now.files.config.as_deref().map(config_digest).as_deref() != Some(base)
        });
    if now.state != prepared.state || now.refs.as_ref() != Some(&prepared.refs) || config_changed {
        return Err(CoreError::PlanStale {
            expected: head_commit(&prepared.state).unwrap_or_else(|| "none".to_string()),
            actual: head_commit(&now.state).unwrap_or_else(|| "none".to_string()),
            detail: format!(
                "the git repository of project {project} changed since the plan (HEAD, refs or \
                 config)"
            ),
        });
    }
    Ok(())
}

/// Slack the free-space need adds on top of the copies and the pack: room for
/// the stage directory, logs and whatever the host writes meanwhile (D-22).
pub const GIT_FREE_SLACK: u64 = 64 * 1024 * 1024;

/// The free bytes the handover of one node needs: the save and the restore's
/// stage are each a copy of the live `.git` (`2 x`), plus the bytes of the
/// pack the advance writes (`pack` and `idx`), plus [`GIT_FREE_SLACK`].
pub fn git_free_need(git_bytes: u64, prep: Option<&AdvancePrep>) -> u64 {
    let pack = prep
        .and_then(|prep| prep.pack.as_ref())
        .map_or(0, |pack| (pack.pack.len() + pack.idx.len()) as u64);
    git_bytes
        .saturating_mul(2)
        .saturating_add(pack)
        .saturating_add(GIT_FREE_SLACK)
}

/// The text of a transport failure without the envelope wording: its detail
/// alone (paths and causes, never content), or the whole message for another
/// error.
fn failure_text(err: &CoreError) -> String {
    match err {
        CoreError::TransportFailed { detail, .. } => detail.clone(),
        other => crate::deploy::git_gate::detail_of(other),
    }
}

/// D-22, step 0: before the run directory exists and before anything is
/// written, prove on the node that the deploy account can write what the
/// handover writes and that there is room for it.
///
/// 1. [`GitStateWriter::probe_write`] creates and removes a probe file in
///    `projects/<project>` and in its `.git`; a refusal is
///    `deploy_preflight_failed` check `git_write_access` naming the path the
///    host reported. On a POSIX host whose gateway account is neither root nor
///    the deploy account it also proves the ACL step of the restore can give
///    that account write access (D-G1): an explicit named entry with an
///    effective `rw-`, read back with `getfacl`.
/// 2. [`GitHostRead::git_size`] (a listing, nothing hashed, checker W-5) and
///    [`GitHostRead::free_bytes`] against [`git_free_need`]; too little room
///    is check `git_free_space` with both figures.
///
/// It never calls `git_manifest`: the manifest hashes every non-object file,
/// which the save and the restore verification already pay for inside their
/// own budgets. `probe_id` names the probe file (`.ign-probe-<probe_id>`); it
/// must be a fresh id that is not a run directory.
pub async fn git_preflight(
    write: &dyn GitStateWriter,
    read: &dyn GitHostRead,
    gateway: &str,
    project: &str,
    prep: Option<&AdvancePrep>,
    probe_id: &str,
) -> Result<(), CoreError> {
    git_preflight_on(MASTER, write, read, gateway, project, prep, probe_id).await
}

/// [`git_preflight`] for one node of a pair (27-22): the same probe and the
/// same room check on that node's host, and a refusal that names a node other
/// than the master.
pub async fn git_preflight_on(
    node: &str,
    write: &dyn GitStateWriter,
    read: &dyn GitHostRead,
    gateway: &str,
    project: &str,
    prep: Option<&AdvancePrep>,
    probe_id: &str,
) -> Result<(), CoreError> {
    let named = |detail: String| {
        if node == MASTER {
            detail
        } else {
            format!("{node}: {detail}")
        }
    };
    write.probe_write(project, probe_id).await.map_err(|err| {
        preflight(
            "git_write_access",
            named(format!(
                "the deploy account cannot write where the git restore needs to ({}); grant it \
                 write access on {} only (README: Write access and the grant)",
                failure_text(&err),
                git_entry_location(project).trim_end_matches("/.git"),
            )),
        )
    })?;
    let size = read.git_size(project).await.map_err(|err| {
        preflight(
            "git_free_space",
            named(format!(
                "the size of {} cannot be read ({})",
                git_entry_location(project),
                failure_text(&err)
            )),
        )
    })?;
    let need = git_free_need(size.bytes, prep);
    let free = read.free_bytes(gateway, project).await.map_err(|err| {
        preflight(
            "git_free_space",
            named(format!(
                "the free space for the save of {} cannot be read ({})",
                git_entry_location(project),
                failure_text(&err)
            )),
        )
    })?;
    if free < need {
        return Err(preflight(
            "git_free_space",
            named(format!(
                "{free} bytes are free for the save and the restore of {} and {need} are \
                 needed (2 x {} bytes of .git, the prepared pack and {} bytes of slack)",
                git_entry_location(project),
                size.bytes,
                GIT_FREE_SLACK,
            )),
        ));
    }
    Ok(())
}

/// What a post-import step of the handover concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct GitAfter {
    /// The restore or the advance failed (or was refused); the outcome of a
    /// verified run is `git_restore_failed`.
    pub failed: bool,
    /// Lines for the run's warnings (redacted).
    pub warnings: Vec<String>,
    /// The events of the step, in order: names, counts, ids and the save path
    /// only (never repo content). The caller logs them.
    pub events: Vec<ProjectsEvent>,
}

impl GitAfter {
    fn new() -> GitAfter {
        GitAfter {
            failed: false,
            warnings: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// The inputs of the post-import steps that are not the report.
#[derive(Clone, Copy)]
pub struct AfterInputs<'a> {
    /// The host's read surface.
    pub read: &'a dyn GitHostRead,
    /// The host's git writer.
    pub write: &'a dyn GitStateWriter,
    /// Host git on ign's machine.
    pub git: &'a HostGit,
    /// The gateway the save belongs to.
    pub gateway: &'a str,
    /// The project.
    pub project: &'a str,
    /// The read-back export the verify used (the live export of a `record`).
    pub export_zip: &'a [u8],
    /// How the post-advance status is proved.
    pub post_status: &'a PostStatusMode,
    /// The gateway's REST surface, for the Designer gate (D-07).
    pub api: &'a dyn ProjectGateway,
    /// The run's `--designers` policy; the gate derives its own from it.
    pub designers: &'a DesignersPolicy,
}

fn capped_paths(mut paths: Vec<String>) -> (Vec<String>, usize) {
    paths.sort();
    paths.dedup();
    let count = paths.len();
    paths.truncate(LIST_CAP);
    (paths, count)
}

impl DiscardPlan {
    /// The content of `git/discarded.json` (27-15, D-05): the operator's
    /// reason, the branch, the abandoned HEAD, the dirty paths, the commit ids,
    /// the pre-image that holds the live content and where the saved `.git`
    /// will be, and the node (`nodes.master` or `nodes.backup`) whose
    /// repository it describes (27-35, WR-09). Ids, paths and the operator's
    /// text only: no commit message and no file content.
    pub fn evidence_json(
        &self,
        node: &str,
        root: LedgerRoot,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "node": node,
            "reason": self.reason,
            "branch": self.branch,
            "abandoned_head": self.abandoned_head,
            "dirty": self.would.dirty,
            "paths": self.would.paths,
            "paths_count": self.would.paths_count,
            "commits": self.would.commits,
            "pre_image": self.would.evidence,
            "save_path": git_save_location(root, gateway, project, run_id),
        })
    }

    /// Record on `report` that the run discarded what the plan names: the
    /// `discarded` block (the commits and the keep-alive ref follow once the
    /// advance wrote them) replaces `would_discard`. Called by `advance_after`
    /// once the advance wrote and read back, never earlier: a run that was
    /// rolled back or stopped keeps `would_discard` (27-35, WR-11, D-05).
    pub fn begin(&self, report: &mut GitReport) {
        report.would_discard = None;
        report.discarded = Some(DiscardedReport {
            reason: self.reason.clone(),
            dirty: self.would.dirty,
            paths: self.would.paths.clone(),
            paths_count: self.would.paths_count,
            commits: Vec::new(),
            keep_ref: None,
            evidence: self.would.evidence.clone(),
        });
    }
}

/// The Designer policy the gate before a post-import `.git` write uses
/// (D-07, checker W-4): always `wait`, whatever mode the run used before the
/// import; the run's `--designers` timeout when that mode was `wait`, else
/// [`GIT_RESTORE_DESIGNER_WAIT_S`]; the run's poll interval (the production
/// default is `DESIGNERS_POLL_S`).
pub fn restore_policy(run: &DesignersPolicy) -> DesignersPolicy {
    DesignersPolicy {
        mode: DesignersMode::Wait,
        timeout: if run.mode == DesignersMode::Wait {
            run.timeout
        } else {
            Duration::from_secs(GIT_RESTORE_DESIGNER_WAIT_S)
        },
        interval: run.interval,
    }
}

/// The Designer check right before a post-import `.git` write (D-07, checker
/// W-4): the import is done and the `.git` may be absent, so a Designer
/// session that is attached now could run `setupLocalRepo` against it. While
/// one is attached nothing is written; the gate waits under
/// [`restore_policy`] and a session still attached at the end, or a list that
/// cannot be read (fail closed), is a [`GitRestoreFailure`] with reason
/// `designer_attached` naming the open sessions, with the save kept at
/// `save_path`. `polls` collects the sessions open at each read, for the
/// `designers_poll` events.
pub async fn git_restore_designer_gate(
    api: &dyn ProjectGateway,
    run: &DesignersPolicy,
    save_path: &str,
    polls: &mut Vec<usize>,
) -> Result<(), GitRestoreFailure> {
    let policy = restore_policy(run);
    let blocked = |detail: String| GitRestoreFailure {
        reason: REASON_DESIGNER_ATTACHED,
        restored: false,
        paths: Vec::new(),
        detail,
        save_path: save_path.to_string(),
    };
    let gate = designers_gate(|| api.designers(), &policy, &SystemClock, &mut |open| {
        polls.push(open);
        Ok(())
    })
    .await;
    match gate {
        Ok((DesignersVerdict::Clear, _)) => Ok(()),
        Ok((_, seen)) => Err(blocked(format!(
            "{}; the .git is not written while one is attached",
            designers_detail(&seen)
        ))),
        Err(err) => Err(blocked(format!(
            "the attached Designers could not be read ({}); the .git is not written blind",
            err.code()
        ))),
    }
}

fn failed_restore(report: &mut GitReport, failure: &GitRestoreFailure) -> GitAfter {
    let node_name = report.master().node.clone();
    let node = report.master_mut();
    node.restore = Some(RestoreInfo {
        restored: failure.restored,
        advanced: false,
        verified: false,
        post_status: None,
        post_status_note: None,
        dropped: Vec::new(),
        dropped_count: 0,
        reason: Some(failure.reason.to_string()),
        save_kept: true,
    });
    let mut line = format!(
        "project {}: git {}: {}; the saved .git is at {}",
        node_name.trim_start_matches("nodes."),
        failure.reason,
        failure.detail,
        failure.save_path
    );
    if !failure.paths.is_empty() {
        let shown: Vec<String> = failure
            .paths
            .iter()
            .take(SHOWN)
            .map(|p| p.escape_debug().to_string())
            .collect();
        line.push_str(&format!(" ({})", shown.join(", ")));
    }
    if failure.reason == "git_recreated" {
        // Pitfall 1, D-07, D-09: a repository someone else created (a Designer
        // open runs `setupLocalRepo` while `.git` is absent) is never
        // overwritten; the save is the module's pre-run repository.
        line.push_str(
            "; the repository now at .git was created during the import and was left \
             untouched",
        );
    }
    let mut after = GitAfter::new();
    after.failed = true;
    after.warnings.push(line);
    if failure.restored {
        after.events.push(ProjectsEvent::GitRestored {
            node: node_name.clone(),
        });
    }
    after.events.push(ProjectsEvent::GitRestoreFailed {
        node: node_name,
        reason: failure.reason.to_string(),
        path: Some(failure.save_path.clone()),
    });
    after
}

/// Step 3b: after a green verify, restore the save, advance the repo to the
/// release commit, prove the result and remove the save.
///
/// The Designer gate ([`git_restore_designer_gate`]) runs first: while a
/// session is attached nothing is restored or written, reason
/// `designer_attached`, the save kept (D-07). A failure of any step keeps the
/// save and names it; the deploy itself stays (D-09). On a `local` node a
/// status over the project directory on disk must equal the predicted drift
/// (D-25, never with the gateway `.git` as the git dir); on an `ssh` node
/// `post_status` is `not_checked`.
pub async fn git_after_verify(
    prepared: &PreparedGit,
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
) -> GitAfter {
    after_verify(prepared, report, saved, inputs, None).await
}

/// What the advance hands to the backup's settle ([`advance_after`]).
#[derive(Default)]
struct SettleHold {
    /// The save was kept because the `.git` was advanced in place.
    held: bool,
    /// The run discarded gateway commits, so the save stays after the settle
    /// too (27-15, D-05); the settle still looks again (CodeRabbit #57).
    keep_save: bool,
    /// The manifest of the advanced `.git`, read right after the read-back;
    /// `None` when it could not be read.
    advanced: Option<GitManifest>,
}

/// [`git_after_verify`], and for the backup of a pair the settle of 27-35
/// (WR-08, D-19, D-G5): with `settle` set, a `.git` that was present at the
/// end of the wait and advanced in place keeps its save through the advance
/// and is probed again after `settle` ([`settle_backup_git`]); the master's
/// path passes `None` and is unchanged.
async fn after_verify(
    prepared: &PreparedGit,
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
    settle: Option<Duration>,
) -> GitAfter {
    let mut polls = Vec::new();
    let gate =
        git_restore_designer_gate(inputs.api, inputs.designers, &saved.path, &mut polls).await;
    let mut hold = SettleHold::default();
    let mut after = match gate {
        Err(failure) => failed_restore(report, &failure),
        Ok(()) => {
            let slot = settle.is_some().then_some(&mut hold);
            advance_after(prepared, report, saved, inputs, slot).await
        }
    };
    if hold.held
        && let Some(interval) = settle
    {
        after = settle_backup_git(prepared, report, saved, inputs, interval, hold, after).await;
    }
    after.events.splice(
        0..0,
        polls
            .into_iter()
            .map(|open| ProjectsEvent::DesignersPoll { open }),
    );
    after
}

/// The `record` action's advance (live equals the artifact, no import ran):
/// the repo is read again and saved, the Designer gate runs, and the repo is
/// advanced in place to the release commit (`restore_and_advance`'s present
/// branch: the `.git` is never replaced or deleted), proved as after an
/// import, and the save removed. A change of HEAD, refs or config since the
/// plan is `git_recreated` with nothing written; a save that cannot be taken
/// is `restore_failed` with nothing written.
pub async fn git_record_advance(
    prepared: &PreparedGit,
    report: &mut GitReport,
    run_id: &str,
    inputs: &AfterInputs<'_>,
) -> GitAfter {
    let saved = match git_before_import(
        prepared,
        inputs.read,
        inputs.write,
        inputs.git,
        inputs.gateway,
        inputs.project,
        run_id,
    )
    .await
    {
        Ok(saved) => saved,
        Err(err) => {
            let (reason, detail) = match &err {
                CoreError::PlanStale { .. } => (
                    "git_recreated",
                    "the git repository changed since the plan (HEAD, refs or config); \
                     nothing was advanced"
                        .to_string(),
                ),
                other => (
                    "restore_failed",
                    format!(
                        "the .git could not be saved before the advance ({}); nothing was \
                         advanced",
                        other.code()
                    ),
                ),
            };
            let node_name = report.master().node.clone();
            report.master_mut().restore = Some(RestoreInfo {
                restored: false,
                advanced: false,
                verified: false,
                post_status: None,
                post_status_note: None,
                dropped: Vec::new(),
                dropped_count: 0,
                reason: Some(reason.to_string()),
                save_kept: false,
            });
            let mut after = GitAfter::new();
            after.failed = true;
            after.warnings.push(format!(
                "project {}: git {reason}: {detail}",
                inputs.project
            ));
            after.events.push(ProjectsEvent::GitRestoreFailed {
                node: node_name,
                reason: reason.to_string(),
                path: None,
            });
            return after;
        }
    };
    report.set_save(&saved);
    let mut after = git_after_verify(prepared, report, &saved, inputs).await;
    after.events.insert(
        0,
        ProjectsEvent::GitSaved {
            node: report.master().node.clone(),
            path: saved.path.clone(),
            files: saved.manifest.files,
            bytes: saved.manifest.bytes,
        },
    );
    after
}

/// The advance itself, after the Designer gate passed.
///
/// `hold` is the backup's settle (27-35, WR-08): when given and the `.git`
/// was advanced in place (not restored), the manifest of the advanced `.git`
/// is read right after the advance was read back (before the post-advance
/// status), the save is not removed here and `hold.held` is set, so the caller
/// can look at `.git` again first.
async fn advance_after(
    prepared: &PreparedGit,
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
    mut hold: Option<&mut SettleHold>,
) -> GitAfter {
    let save_path = saved.path.clone();
    let node_name = report.master().node.clone();
    // The backup's `project.json` is checked once its project converged
    // (27-37); the master's handling is unchanged.
    let is_backup = node_name == BACKUP;
    let Some(prep) = prepared.prep.as_ref() else {
        return failed_restore(
            report,
            &GitRestoreFailure {
                reason: "advance_failed",
                restored: false,
                paths: Vec::new(),
                detail: "no advance was prepared for this repo".to_string(),
                save_path,
            },
        );
    };
    // Commits the override abandons stay reachable through a ref the advance
    // writes next to the branch (27-15, D-05, Pitfall 7).
    let discarding = prepared
        .discard
        .as_ref()
        .filter(|plan| !plan.would.commits.is_empty())
        .and_then(|plan| plan.abandoned_head.as_deref())
        .map(|abandoned_head| Discarding {
            run_id: saved.run_id.as_str(),
            abandoned_head,
        });
    let done = match restore_and_advance(
        inputs.read,
        inputs.write,
        inputs.git,
        &prepared.checkout.path,
        &prepared.state,
        saved,
        prep,
        inputs.export_zip,
        discarding,
    )
    .await
    {
        Ok(done) => done,
        Err(failure) => return failed_restore(report, &failure),
    };
    // The state the backup's settle compares against: the advanced `.git`,
    // read as soon as the advance was read back. A read that fails (the sync
    // already took the repository) leaves `None`: the probe decides.
    if let Some(slot) = hold.as_deref_mut()
        && !done.restored
    {
        slot.advanced = inputs.read.git_manifest(inputs.project).await.ok();
    }
    // The advance wrote: what the override discards is gone from the live
    // repository now (27-35, WR-11), so only here does `discarded` replace
    // `would_discard`. The evidence file was written before the import.
    if let Some(plan) = prepared.discard.as_ref() {
        plan.begin(report);
        if done.keep_save
            && let Some(discarded) = report.discarded.as_mut()
        {
            discarded.commits.clone_from(&plan.would.commits);
            discarded.keep_ref = Some(discard_ref_name(&saved.run_id));
        }
    }

    // The advance wrote: a partial repo is now adopted (27-17).
    report.master_mut().adopted = prepared.adopt.is_some() && done.advanced;
    let (dropped, dropped_count) = capped_paths(done.dropped.clone());
    let (drift, drift_count) = capped_paths(done.expected_drift.clone());
    let mut restore = RestoreInfo {
        restored: done.restored,
        advanced: done.advanced,
        verified: done.verified,
        post_status: None,
        post_status_note: None,
        dropped,
        dropped_count,
        reason: None,
        save_kept: true,
    };
    let mut after = GitAfter::new();
    if done.restored {
        after.events.push(ProjectsEvent::GitRestored {
            node: node_name.clone(),
        });
    }
    after.events.push(ProjectsEvent::GitAdvanced {
        node: node_name.clone(),
        release_commit: report.release_commit.clone(),
        dropped: dropped_count,
        expected_drift: drift_count,
    });
    match inputs.post_status {
        PostStatusMode::NotChecked => {
            restore.post_status = Some("not_checked");
            restore.post_status_note = Some(POST_STATUS_NOTE_SSH.to_string());
        }
        PostStatusMode::Disk(data_dir) => {
            match disk_status_differs(
                inputs.git,
                &prepared.checkout.path,
                data_dir,
                inputs.project,
                &done.expected_drift,
            )
            .await
            {
                Ok(differ) if differ.is_empty() => restore.post_status = Some("matched"),
                Ok(differ) => {
                    let count = differ.len();
                    let shown: Vec<String> = differ
                        .iter()
                        .take(SHOWN)
                        .map(|p| p.escape_debug().to_string())
                        .collect();
                    restore.post_status = Some("mismatch");
                    restore.post_status_note = Some(format!(
                        "D-25: a status over the project directory differs from expected_drift \
                         for {count} path(s): {}",
                        shown.join(", ")
                    ));
                    after.failed = true;
                    // 27-37: a backup whose only differing path is a missing
                    // `project.json` takes the detection word; any other
                    // difference keeps the status mismatch and the finding
                    // becomes a warning beside it.
                    let finding = if is_backup {
                        check_backup_project_json(inputs.read, inputs.project).await
                    } else {
                        None
                    };
                    let manifest_only = differ.iter().all(|p| p == PROJECT_JSON);
                    match finding {
                        Some(finding) if manifest_only => {
                            restore.reason = Some(REASON_BACKUP_PROJECT_JSON_MISSING.to_string());
                            after
                                .warnings
                                .push(backup_project_json_warning(inputs.project, &finding));
                        }
                        finding => {
                            restore.reason = Some(REASON_POST_STATUS_MISMATCH.to_string());
                            after.warnings.push(format!(
                                "project {}: git {REASON_POST_STATUS_MISMATCH}: the repository was \
                                 advanced but the project directory does not match the predicted \
                                 status; the saved .git is at {save_path}",
                                inputs.project
                            ));
                            if let Some(finding) = finding {
                                after
                                    .warnings
                                    .push(backup_project_json_warning(inputs.project, &finding));
                            }
                        }
                    }
                }
                Err(detail) => {
                    restore.post_status = Some("mismatch");
                    restore.post_status_note = Some(format!(
                        "D-25: the status over the project directory could not be run: {detail}"
                    ));
                    restore.reason = Some(REASON_POST_STATUS_MISMATCH.to_string());
                    after.failed = true;
                    after.warnings.push(format!(
                        "project {}: git {REASON_POST_STATUS_MISMATCH}: the post-advance status \
                         could not be run; the saved .git is at {save_path}",
                        inputs.project
                    ));
                    if is_backup
                        && let Some(finding) =
                            check_backup_project_json(inputs.read, inputs.project).await
                    {
                        after
                            .warnings
                            .push(backup_project_json_warning(inputs.project, &finding));
                    }
                }
            }
        }
    }
    let failed_reason = restore
        .reason
        .clone()
        .unwrap_or_else(|| REASON_POST_STATUS_MISMATCH.to_string());
    report.master_mut().restore = Some(restore);
    if after.failed {
        after.events.push(ProjectsEvent::GitRestoreFailed {
            node: node_name,
            reason: failed_reason,
            path: Some(save_path),
        });
    } else if done.keep_save {
        // Gateway commits were abandoned: the saved repository is the whole
        // gateway repo as the module left it, so it stays (27-15, D-05).
        after.warnings.push(format!(
            "project {}: gateway commits were discarded; they stay reachable at {} in the \
             gateway repo and the saved .git is kept at {save_path}",
            inputs.project,
            discard_ref_name(&saved.run_id)
        ));
        match hold {
            // the settle looks again (a late delete is restored from the
            // save) and checks `project.json` then; the save stays
            Some(slot) if !done.restored => {
                slot.held = true;
                slot.keep_save = true;
            }
            _ => {
                if is_backup {
                    project_json_gate(report, saved, inputs, &mut after).await;
                }
            }
        }
    } else {
        match hold {
            // The settle looks again, and checks `project.json` then.
            Some(slot) if !done.restored => slot.held = true,
            _ => {
                let blocked =
                    is_backup && project_json_gate(report, saved, inputs, &mut after).await;
                if !blocked {
                    remove_save(report, saved, inputs, &mut after).await;
                }
            }
        }
    }
    report.expected_drift = Some(drift);
    report.expected_drift_count = Some(drift_count);
    report.expected_drift_complete = true;
    after
}

/// A failure found while the backup's `.git` settled: the advance had written
/// and read back, so `advanced` and `verified` stay as reported; the reason is
/// set, the save is kept and named, and the node ends `git_restore_failed`.
fn settle_failed(
    report: &mut GitReport,
    saved: &GitSaveReport,
    after: &mut GitAfter,
    reason: &'static str,
    detail: &str,
) {
    let node = report.master().node.clone();
    if let Some(restore) = report.master_mut().restore.as_mut() {
        restore.reason = Some(reason.to_string());
        restore.save_kept = true;
    }
    after.failed = true;
    after.warnings.push(format!(
        "project {}: git {reason} on {node}: {detail}; the saved .git is at {}",
        saved.project, saved.path
    ));
    after.events.push(ProjectsEvent::GitRestoreFailed {
        node,
        reason: reason.to_string(),
        path: Some(saved.path.clone()),
    });
}

/// Whether the backup's `data/projects/<p>/project.json` is a regular file
/// (27-37). `None` when it is. Otherwise the finding as the warning words it:
/// `is absent`, `is a directory`, `is a link`, `is not a regular file`, or
/// `could not be checked (<code>)` for a probe that failed, which is never
/// read as present. A read through the host's per-name probe: nothing is
/// followed, read or written, and nothing on the backup is repaired.
pub async fn check_backup_project_json(read: &dyn GitHostRead, project: &str) -> Option<String> {
    match read
        .project_dir_entry(project, ImportDir::ProjectJson)
        .await
    {
        Ok(GitEntry::File) => None,
        Ok(GitEntry::Absent) => Some("is absent".to_string()),
        Ok(GitEntry::Directory) => Some("is a directory".to_string()),
        Ok(GitEntry::Symlink) => Some("is a link".to_string()),
        Ok(GitEntry::Other) => Some("is not a regular file".to_string()),
        Err(err) => Some(format!("could not be checked ({})", err.code())),
    }
}

/// The warning of a [`check_backup_project_json`] finding. It names the
/// consequence, the owner of the defect and the one step; it never tells the
/// operator to restore `.git` (D-09), because the repository is not the
/// problem.
pub fn backup_project_json_warning(project: &str, finding: &str) -> String {
    let verb = if finding.starts_with("could not") {
        "may"
    } else {
        "will"
    };
    format!(
        "project {project}: the backup's data/projects/{project}/project.json {finding} after the \
         deploy; the backup {verb} lose project {project} on its next restart. This is a git \
         module and redundancy project-sync defect that ign does not repair; put project.json \
         back on the backup (for example a copy of the master's) before the backup restarts."
    )
}

/// The finding fails the backup node (27-37): its reason is the detection
/// word, its save is kept and named, the node ends `git_restore_failed`, and
/// the warning says what it means. The advance itself stays as reported.
fn fail_on_project_json(
    report: &mut GitReport,
    saved: &GitSaveReport,
    after: &mut GitAfter,
    finding: &str,
) {
    let node = report.master().node.clone();
    if let Some(restore) = report.master_mut().restore.as_mut() {
        restore.reason = Some(REASON_BACKUP_PROJECT_JSON_MISSING.to_string());
        restore.save_kept = true;
    }
    after.failed = true;
    after
        .warnings
        .push(backup_project_json_warning(&saved.project, finding));
    after.events.push(ProjectsEvent::GitRestoreFailed {
        node,
        reason: REASON_BACKUP_PROJECT_JSON_MISSING.to_string(),
        path: Some(saved.path.clone()),
    });
}

/// The backup's `project.json` check at the point where its save would go
/// (27-37): a finding fails the node and the save stays. `true` when the node
/// was failed, so the caller leaves the save.
async fn project_json_gate(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
    after: &mut GitAfter,
) -> bool {
    match check_backup_project_json(inputs.read, inputs.project).await {
        Some(finding) => {
            fail_on_project_json(report, saved, after, &finding);
            true
        }
        None => false,
    }
}

/// The backup's settle (27-35, WR-08, D-19, D-G5). Redundancy removes the
/// backup's module-cloned `.git` when the master's change reaches it, and 27-04
/// measured that a few seconds after the first content-changing import, not
/// when the backup's REST export converges. A `.git` found present at the end
/// of the wait and advanced in place may therefore still be deleted a moment
/// later; removing the save on the strength of that read would leave the backup
/// with neither. So the save was kept through the advance ([`advance_after`]
/// with `hold`), and one poll interval later the `.git` is probed again
/// through the backup's host:
///
/// - a directory whose manifest equals `advanced`, the one read right after the
///   advance was read back:
///   the save is removed ([`remove_save`]);
/// - absent: the Designer gate runs (D-07) and the handover runs again from the
///   save, which restores it and advances it, and the save is removed only
///   after that;
/// - anything else (a different manifest, a non-directory, a probe that
///   failed): the node ends `git_recreated` (or `restore_failed` for a failed
///   probe), `.git` is left as it is and the save is kept and named.
async fn settle_backup_git(
    prepared: &PreparedGit,
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
    interval: Duration,
    hold: SettleHold,
    mut after: GitAfter,
) -> GitAfter {
    let SettleHold {
        advanced,
        keep_save,
        ..
    } = hold;
    let project = inputs.project;
    tokio::time::sleep(interval).await;
    let files = match inputs.read.read_git_state(project).await {
        Ok(files) => files,
        Err(err) => {
            settle_failed(
                report,
                saved,
                &mut after,
                "restore_failed",
                &format!(
                    "the backup's .git could not be probed again after the advance ({}); it \
                     was not touched",
                    err.code()
                ),
            );
            return after;
        }
    };
    match files.kind {
        GitEntry::Directory => match inputs.read.git_manifest(project).await {
            Ok(now) if advanced.as_ref() == Some(&now) => {
                // The second look also covers `project.json` (27-37): the
                // sync's late delete takes it with the `.git`. A finding
                // fails the node and the save stays.
                if !project_json_gate(report, saved, inputs, &mut after).await && !keep_save {
                    remove_save(report, saved, inputs, &mut after).await;
                }
            }
            Ok(_) => settle_failed(
                report,
                saved,
                &mut after,
                "git_recreated",
                "the backup's .git changed after it was advanced (the redundancy sync reached \
                 it late); it is left as it is",
            ),
            Err(err) => settle_failed(
                report,
                saved,
                &mut after,
                "restore_failed",
                &format!(
                    "the backup's .git could not be read again after the advance ({}); it was \
                     not touched",
                    err.code()
                ),
            ),
        },
        GitEntry::Absent => {
            let mut polls = Vec::new();
            let gate =
                git_restore_designer_gate(inputs.api, inputs.designers, &saved.path, &mut polls)
                    .await;
            after.events.extend(
                polls
                    .into_iter()
                    .map(|open| ProjectsEvent::DesignersPoll { open }),
            );
            let more = match gate {
                Err(failure) => failed_restore(report, &failure),
                Ok(()) => {
                    after.warnings.push(format!(
                        "project {}: the backup's .git was removed after the advance (the \
                         redundancy sync reached it late); it was restored from the save and \
                         advanced again",
                        saved.project
                    ));
                    advance_after(prepared, report, saved, inputs, None).await
                }
            };
            after.failed = more.failed;
            after.warnings.extend(more.warnings);
            after.events.extend(more.events);
        }
        GitEntry::File | GitEntry::Symlink | GitEntry::Other => settle_failed(
            report,
            saved,
            &mut after,
            "git_recreated",
            "something other than the repository that was advanced is at the backup's .git; it \
             is left as it is",
        ),
    }
    after
}

/// Everything held: the save goes. A save that cannot be removed is left and
/// reported, never a failure of a deploy that verified.
async fn remove_save(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
    after: &mut GitAfter,
) {
    match inputs
        .write
        .remove_git_save(inputs.gateway, inputs.project, &saved.run_id)
        .await
    {
        Ok(()) => {
            if let Some(r) = report.master_mut().restore.as_mut() {
                r.save_kept = false;
            }
            after.events.push(ProjectsEvent::GitSaveRemoved {
                node: report.master().node.clone(),
                path: saved.path.clone(),
            });
        }
        Err(err) => after.warnings.push(format!(
            "project {}: the saved .git at {} could not be removed ({})",
            inputs.project,
            saved.path,
            err.code()
        )),
    }
}

/// The inputs of [`git_after_unverified`].
pub struct UnverifiedInputs<'a> {
    /// The host's read surface.
    pub read: &'a dyn GitHostRead,
    /// The host's git writer.
    pub write: &'a dyn GitStateWriter,
    /// The gateway the save belongs to.
    pub gateway: &'a str,
    /// The project.
    pub project: &'a str,
    /// The gateway's REST surface, for the Designer gate (D-07).
    pub api: &'a dyn ProjectGateway,
    /// The run's `--designers` policy.
    pub designers: &'a DesignersPolicy,
}

/// K11: the import did not read back as the artifact. The Designer gate runs;
/// then the saved `.git` is put back exactly as saved if the import removed
/// it ([`restore_unchanged`]) and the repo is never advanced. A repo that is
/// still present is left untouched. The save is removed only when the repo
/// equals it, else it is kept and named. A failure is reported in
/// `restore` and in the warnings; it never changes the run's outcome, which
/// stays `imported_unverified`.
pub async fn git_after_unverified(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &UnverifiedInputs<'_>,
) -> GitAfter {
    let mut polls = Vec::new();
    let gate =
        git_restore_designer_gate(inputs.api, inputs.designers, &saved.path, &mut polls).await;
    let mut after = match gate {
        Err(failure) => failed_restore(report, &failure),
        Ok(()) => put_back_unchanged(report, saved, inputs).await.0,
    };
    after.events.splice(
        0..0,
        polls
            .into_iter()
            .map(|open| ProjectsEvent::DesignersPoll { open }),
    );
    after
}

/// The restore of [`git_after_unverified`] and [`git_after_rollback`] once the
/// Designer gate allowed it: the saved `.git` goes back exactly as saved if the
/// project step removed it ([`restore_unchanged`]), and is never advanced. A
/// repo that is still present is left untouched. The save is removed only when
/// the repo equals it, else it is kept and named.
///
/// Returns the step and whether the repository now in place equals the save
/// (27-34, WR-03): a save that stays because its removal failed is a warning,
/// not a repository that differs. `false` when the restore itself failed.
async fn put_back_unchanged(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &UnverifiedInputs<'_>,
) -> (GitAfter, bool) {
    let done = match restore_unchanged(
        inputs.read,
        inputs.write,
        inputs.gateway,
        inputs.project,
        saved,
    )
    .await
    {
        Err(failure) => return (failed_restore(report, &failure), false),
        Ok(done) => done,
    };
    let node_name = report.master().node.clone();
    report.master_mut().restore = Some(RestoreInfo {
        restored: done.restored,
        advanced: false,
        verified: done.verified,
        post_status: None,
        post_status_note: None,
        dropped: Vec::new(),
        dropped_count: 0,
        reason: None,
        save_kept: true,
    });
    let mut after = GitAfter::new();
    if done.restored {
        after
            .events
            .push(ProjectsEvent::GitRestored { node: node_name });
    }
    // The save is removed only when the repo now equals it.
    let equal = matches!(
        inputs.read.git_manifest(inputs.project).await,
        Ok(now) if now == saved.manifest
    );
    if equal {
        remove_saved_copy(report, saved, inputs, &mut after).await;
    } else {
        after.warnings.push(format!(
            "project {}: the git repository now in place is not the one saved; it was left \
             as it is and the saved .git is at {}",
            inputs.project, saved.path
        ));
    }
    (after, equal)
}

async fn remove_saved_copy(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &UnverifiedInputs<'_>,
    after: &mut GitAfter,
) {
    match inputs
        .write
        .remove_git_save(inputs.gateway, inputs.project, &saved.run_id)
        .await
    {
        Ok(()) => {
            if let Some(r) = report.master_mut().restore.as_mut() {
                r.save_kept = false;
            }
            after.events.push(ProjectsEvent::GitSaveRemoved {
                node: report.master().node.clone(),
                path: saved.path.clone(),
            });
        }
        Err(err) => after.warnings.push(format!(
            "project {}: the saved .git at {} could not be removed ({}); the repository in \
             place is the one saved, so the copy is safe to delete by hand",
            inputs.project,
            saved.path,
            err.code()
        )),
    }
}

/// What the git step of a rollback concluded.
#[derive(Debug, Clone, PartialEq)]
pub struct RollbackGit {
    /// Warnings and events of the step, as for the other post-import steps.
    pub after: GitAfter,
    /// Lines for the rollback's `left` list: the repository was not put back
    /// (a Designer attached, a failed restore) or a repository that is not the
    /// saved one stands at `.git` and was left alone (D-08). Each names the
    /// save path. A non-empty list makes a green rollback `rollback_failed`.
    pub left: Vec<String>,
}

/// D-08: the project was rolled back (or the rollback found it untouched), so
/// the repository goes back to what the run saved, unchanged, and is never
/// advanced.
///
/// The `.git` is probed. Absent (the pre-image import removed it): the Designer
/// gate runs first ([`git_restore_designer_gate`], D-07: while a session is
/// attached nothing is written and the save is kept), then the save is
/// restored ([`restore_unchanged`]). Present and equal to the save: left as it
/// is and the save removed. Present and different (a Designer recreated it, or
/// someone else changed it): left as it is, the save kept, and a `left` line
/// names the save path; no path overwrites or deletes it.
pub async fn git_after_rollback(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &UnverifiedInputs<'_>,
) -> RollbackGit {
    let mut polls = Vec::new();
    let absent = matches!(
        inputs.read.read_git_state(inputs.project).await,
        Ok(files) if files.kind == GitEntry::Absent
    );
    let gate = if absent {
        git_restore_designer_gate(inputs.api, inputs.designers, &saved.path, &mut polls).await
    } else {
        Ok(())
    };
    let (mut after, equal) = match gate {
        Err(failure) => (failed_restore(report, &failure), false),
        Ok(()) => put_back_unchanged(report, saved, inputs).await,
    };
    after.events.splice(
        0..0,
        polls
            .into_iter()
            .map(|open| ProjectsEvent::DesignersPoll { open }),
    );
    let restore = report.master().restore.as_ref();
    let left = if let Some(reason) = restore.and_then(|r| r.reason.as_deref()) {
        vec![format!(
            "project {}: its git repository was not put back ({reason}); the saved .git is at {}",
            inputs.project, saved.path
        )]
    } else if restore.is_some_and(|r| r.save_kept) && !equal {
        // 27-34 (WR-03): only a repository that differs from the save is a
        // `left` line. One put back equal to it whose saved copy could not be
        // removed is a verified rollback; `remove_saved_copy` warned.
        vec![format!(
            "project {}: the git repository now in place is not the one saved and was left as \
             it is; the saved .git is at {}",
            inputs.project, saved.path
        )]
    } else {
        Vec::new()
    };
    RollbackGit { after, left }
}

/// `--rollback-run` over a project whose `.git` exists (D-08): classify it.
/// An unsupported shape refuses `project_git_managed` with its reason before
/// anything is written; a managed or adoptable repo is reported (state,
/// branch, remote, head) so the run can save it, re-import the pre-image and
/// restore it unchanged. There is no release in an undo, so `release_commit`
/// is empty and nothing is advanced.
pub async fn check_rollback_run(
    surfaces: &ProjectsSurfaces<'_>,
    project: &str,
    entry: GitEntry,
) -> Result<GitReport, CoreError> {
    let kind = entry.as_str();
    let Some(read) = surfaces.host.git() else {
        return Err(managed(project, kind, kind));
    };
    let found = read_state(read, &surfaces.host.host_git(), project)
        .await
        .map_err(|_| managed(project, kind, kind))?;
    if !matches!(
        found.state.kind,
        GitStateKind::Managed | GitStateKind::Adoptable
    ) {
        return Err(managed(project, kind, &reason_of(&found.state)));
    }
    Ok(GitReport {
        release_commit: String::new(),
        nodes: vec![NodeGitReport {
            node: "nodes.master".to_string(),
            state: state_word(found.state.kind),
            reason: None,
            branch: found.state.tracked.clone(),
            branch_source: None,
            remote: found.state.remote.as_ref().map(|r| r.name.clone()),
            remote_url: found.state.remote.as_ref().map(|r| r.url_redacted.clone()),
            head: head_commit(&found.state),
            adopted: false,
            unpushed: Vec::new(),
            not_merged: Vec::new(),
            not_merged_count: 0,
            dirty: None,
            dirty_count: None,
            dirty_source: None,
            hazards: Vec::new(),
            save: None,
            restore: None,
        }],
        expected_drift: None,
        expected_drift_count: None,
        expected_drift_complete: false,
        would_discard: None,
        discarded: None,
        hazards_acknowledged: None,
    })
}

/// The paths where a status over the project directory on disk differs from
/// the predicted drift, or why the status could not be run (D-25).
///
/// Host git runs with ign's checkout as the object store and a private copy
/// of the written index; the gateway `.git` is never the git dir, so its
/// config, hooks and filters never run. A symlink at `.git` or its index is
/// refused before anything is read.
async fn disk_status_differs(
    git: &HostGit,
    checkout: &Path,
    data_dir: &Path,
    project: &str,
    expected: &[String],
) -> Result<Vec<String>, String> {
    let work = data_dir.join("projects").join(project);
    let dot_git = work.join(".git");
    let index_path = dot_git.join("index");
    for path in [&dot_git, &index_path] {
        let meta = std::fs::symlink_metadata(path)
            .map_err(|e| format!("{} cannot be inspected ({:?})", path.display(), e.kind()))?;
        if meta.file_type().is_symlink() {
            return Err(format!("{} is a symlink", path.display()));
        }
    }
    let index = std::fs::read(&index_path)
        .map_err(|e| format!("the written index cannot be read ({:?})", e.kind()))?;
    let lines = git
        .status_over(checkout, &index, &work)
        .await
        .map_err(|e| crate::deploy::git_gate::detail_of(&e))?;
    let got: BTreeSet<&String> = lines.modified.iter().chain(&lines.untracked).collect();
    let want: BTreeSet<&String> = expected.iter().collect();
    let mut differ: BTreeSet<String> = got
        .symmetric_difference(&want)
        .map(|p| (*p).clone())
        .collect();
    differ.extend(lines.deleted.iter().cloned());
    Ok(differ.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Redundant pairs (Phase 27, 27-22, D-16, D-19, D-G5)
// ---------------------------------------------------------------------------

/// A refusal rewritten so it names `node`: the master's refusals stay exactly
/// as they are. A reasoned `project_git_managed` names the node in its
/// location, a failed preflight check in its detail, a gate in its keys and a
/// transport failure in its node.
pub fn on_node(err: CoreError, node: &str) -> CoreError {
    if node == MASTER {
        return err;
    }
    match err {
        CoreError::ProjectGitManaged {
            project,
            location,
            kind,
            reason,
        } => CoreError::ProjectGitManaged {
            project,
            location: format!("{node} {location}"),
            kind,
            reason,
        },
        CoreError::DeployPreflightFailed { failures } => CoreError::DeployPreflightFailed {
            failures: failures
                .iter()
                .map(|f| PreflightFailure {
                    check: f.check.clone(),
                    detail: format!("{node}: {}", f.detail),
                })
                .collect(),
        },
        CoreError::DestructiveChangeRefused { gate, keys, count } => {
            CoreError::DestructiveChangeRefused {
                gate,
                keys: keys.into_iter().map(|k| format!("{node}:{k}")).collect(),
                count,
            }
        }
        CoreError::TransportFailed {
            operation, detail, ..
        } => CoreError::TransportFailed {
            operation,
            node: node.to_string(),
            detail,
        },
        other => other,
    }
}

/// An entry of `data.git.nodes` for a node whose `.git` does not exist: the
/// node is independent and deploys as it always did.
fn absent_node(node: &str) -> NodeGitReport {
    NodeGitReport {
        node: node.to_string(),
        state: "absent",
        reason: None,
        branch: None,
        branch_source: None,
        remote: None,
        remote_url: None,
        head: None,
        adopted: false,
        unpushed: Vec::new(),
        not_merged: Vec::new(),
        not_merged_count: 0,
        dirty: None,
        dirty_count: None,
        dirty_source: None,
        hazards: Vec::new(),
        save: None,
        restore: None,
    }
}

/// The published `data.git` of a target, from each node's own single-node
/// report. On a pair (`pair`) it holds the master, then the backup, an absent
/// node as an `absent` entry (D-23: a node without `.git` stays without one).
/// `None` when no node holds a repository. The block-level facts (`would_discard`,
/// `discarded`, `hazards_acknowledged`, `expected_drift`) are the master's, or
/// the backup's when the master holds no repository; `would_discard` is the
/// backup's whenever the master has none.
pub fn pair_report(
    master: Option<&GitReport>,
    backup: Option<&GitReport>,
    pair: bool,
) -> Option<GitReport> {
    let base = master.or(backup)?;
    let mut report = base.clone();
    if !pair {
        return Some(report);
    }
    let node_of = |r: Option<&GitReport>, name: &str| {
        r.and_then(|r| r.nodes.first().cloned())
            .unwrap_or_else(|| absent_node(name))
    };
    report.nodes = vec![node_of(master, MASTER), node_of(backup, BACKUP)];
    // The discard the run reports is the master's, else the backup's (27-35,
    // WR-09): a backup that is the only repository to discard from still shows
    // what the override passes.
    if report.would_discard.is_none() {
        report.would_discard = backup.and_then(|b| b.would_discard.clone());
    }
    Some(report)
}

/// The three facts two nodes must share: origin, tracked branch and HEAD.
struct NodeFacts {
    origin_key: String,
    origin: String,
    branch: Option<String>,
    head: Option<String>,
}

/// The agreement facts of `host`'s repository; `None` for a state that is
/// neither managed nor adoptable (the node's own prepare refuses it with its
/// own reason).
async fn node_facts(
    loaded: &LoadedProject,
    host: &dyn ProjectHost,
    git: &HostGit,
) -> Result<Option<NodeFacts>, CoreError> {
    let project = loaded.file.project.as_str();
    let Some(read) = host.git() else {
        return Ok(None);
    };
    let found = read_state(read, git, project).await?;
    let adoptable = found.state.kind == GitStateKind::Adoptable;
    if found.state.kind != GitStateKind::Managed && !adoptable {
        return Ok(None);
    }
    let Some(remote) = found.state.remote.clone() else {
        return Ok(None);
    };
    let branch = if adoptable {
        // A partial repo is adopted onto an explicit branch; a node whose
        // branch cannot be decided is left to its own prepare to refuse.
        adoption_branch(loaded, read, git, project, "directory")
            .await
            .ok()
            .map(|a| a.branch)
    } else {
        found.state.tracked.clone()
    };
    Ok(Some(NodeFacts {
        origin_key: remote.url_key,
        origin: remote.url_redacted,
        branch,
        head: head_commit(&found.state),
    }))
}

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// Both nodes hold a repository and they differ in origin, tracked branch or
/// HEAD: refused before any write (T-27-49). A fact that is unknown on one
/// side (the HEAD of a partial repo) is not compared. The text names both
/// nodes' values, with URLs redacted.
fn nodes_disagree(project: &str, master: &NodeFacts, backup: &NodeFacts) -> Option<CoreError> {
    let mut differ: Vec<String> = Vec::new();
    if master.origin_key != backup.origin_key {
        differ.push(format!(
            "origin {MASTER} {} / {BACKUP} {}",
            master.origin.escape_debug(),
            backup.origin.escape_debug()
        ));
    }
    if let (Some(a), Some(b)) = (&master.branch, &backup.branch)
        && a != b
    {
        differ.push(format!(
            "branch {MASTER} {} / {BACKUP} {}",
            a.escape_debug(),
            b.escape_debug()
        ));
    }
    if let (Some(a), Some(b)) = (&master.head, &backup.head)
        && a != b
    {
        differ.push(format!(
            "HEAD {MASTER} {} / {BACKUP} {}",
            short(a),
            short(b)
        ));
    }
    if differ.is_empty() {
        return None;
    }
    Some(CoreError::ProjectGitManaged {
        project: project.to_string(),
        location: format!(
            "{} differs between the nodes ({})",
            git_entry_location(project),
            differ.join("; ")
        ),
        kind: "directory".to_string(),
        reason: "nodes_disagree".to_string(),
    })
}

/// What the pair half of the prepare found.
pub struct PairPrep {
    /// The master's half, as [`prepare_git_host_with`] returns it.
    pub master: HostPrep,
    /// The backup's repository, prepared; `None` when the backup holds no
    /// `.git` (or the target has no backup).
    pub backup: Option<PreparedGit>,
    /// The backup node's read host, kept for the live step.
    pub backup_host: Option<Box<dyn ProjectHost>>,
    /// The target declares `nodes.backup` and the gates apply to it.
    pub pair: bool,
}

/// Step 1 on a target with `nodes.backup` (27-22, D-19): read the backup's
/// `.git` through its own host exactly as the master's is read, refuse when
/// both nodes hold a repository and they disagree, then prepare each node
/// from its own refs. A target without `nodes.backup`, and a report-mode
/// rehearsal, take the master's step alone. Nothing is written.
pub async fn prepare_git_pair(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    master_entry: GitEntry,
    mode: GitMode,
    warnings: &mut Vec<String>,
) -> Result<PairPrep, CoreError> {
    let backup_node = loaded
        .target
        .nodes
        .backup
        .as_ref()
        .filter(|_| mode == GitMode::Enforce);
    // A backup with no `ssh` or `local` block is reached over REST only: its
    // `.git` cannot be read, so it is not handled (as before this plan).
    let backup_node = match backup_node {
        Some(node) if node.ssh.is_none() && node.local.is_none() => {
            if master_entry.is_present() {
                warnings.push(format!(
                    "project {}: {BACKUP} declares no ssh or local transport, so its git \
                     repository cannot be read and was not handled",
                    loaded.file.project
                ));
            }
            None
        }
        other => other,
    };
    let Some(backup_node) = backup_node else {
        let master =
            prepare_git_host_with(loaded, input, surfaces, master_entry, mode, warnings).await?;
        return Ok(PairPrep {
            master,
            backup: None,
            backup_host: None,
            pair: false,
        });
    };
    let project = loaded.file.project.as_str();
    let backup_host =
        project_host_for_node(&input.targets, &loaded.target_file, BACKUP, backup_node)?;
    let backup_entry = backup_host
        .git_entry(project)
        .await
        .map_err(|e| on_node(e, BACKUP))?;
    if master_entry.is_present() && backup_entry.is_present() {
        let git = surfaces.host.host_git();
        let master_facts = node_facts(loaded, surfaces.host, &git).await?;
        let backup_facts = node_facts(loaded, backup_host.as_ref(), &git)
            .await
            .map_err(|e| on_node(e, BACKUP))?;
        if let (Some(m), Some(b)) = (&master_facts, &backup_facts)
            && let Some(err) = nodes_disagree(project, m, b)
        {
            return Err(err);
        }
    }
    let master =
        prepare_git_host_with(loaded, input, surfaces, master_entry, mode, warnings).await?;
    let backup = prepare_git_host_inner(
        loaded,
        input,
        backup_host.as_ref(),
        surfaces.host.host_git(),
        backup_entry,
        mode,
        &mut HostSeen::default(),
        BACKUP,
        LedgerRoot::for_node(backup_node),
    )
    .await
    .map_err(|e| on_node(e, BACKUP))?;
    Ok(PairPrep {
        master,
        backup,
        backup_host: Some(backup_host),
        pair: true,
    })
}

/// Step 2 for the backup: the same gates against the live project the master
/// serves (redundancy replicates it to the backup; a backup that lags is
/// caught by the run's convergence wait), with the D-24 object read through
/// the backup's own host.
pub async fn prepare_backup_live(
    backup: &mut PreparedGit,
    host: &dyn ProjectHost,
    git: HostGit,
    live: Option<&CanonicalDigest>,
    action: ProjectAction,
    loaded: &LoadedProject,
    warnings: &mut Vec<String>,
) -> Result<(), CoreError> {
    prepare_git_live_on(backup, live, action, loaded, host, git, warnings)
        .await
        .map_err(|e| on_node(e, BACKUP))
}

/// What a wait for the backup's own copy of the project needs.
#[derive(Clone, Copy)]
pub struct BackupWait<'a> {
    /// The backup's read-only REST handle (GET only).
    pub backup: &'a dyn BackupTags,
    /// `--backup-timeout` and the poll interval.
    pub policy: crate::deploy::pair::BackupPolicy,
}

/// Poll the backup's project export until its canonical digest is `want`
/// (D-G5: the backup's `data/projects/<p>` follows the gateway's project, so
/// the export is read as the proxy for it), or the budget passes. The
/// converged export bytes come back for the read-back of the handover; a wait
/// that did not converge returns the reason text (a code or a count, never
/// content).
pub async fn wait_for_backup_project(
    wait: &BackupWait<'_>,
    project: &str,
    want: &str,
) -> Result<Vec<u8>, String> {
    let start = std::time::Instant::now();
    let mut last: String;
    loop {
        match wait.backup.project_export(project).await {
            Ok(zip) => match crate::deploy::project_artifact::canonical_digest(&zip) {
                Ok(digest) if !digest.members.is_empty() && digest.sha256 == want => {
                    return Ok(zip);
                }
                Ok(_) => last = "the backup's project does not equal the release yet".to_string(),
                Err(err) => {
                    last = format!("the backup's export is unusable ({})", err.code());
                }
            },
            Err(err) => {
                last = format!("the backup's export could not be read ({})", err.code());
            }
        }
        if start.elapsed() >= wait.policy.budget {
            return Err(last);
        }
        tokio::time::sleep(wait.policy.interval).await;
    }
}

/// Step 3b for the backup (27-22, D-G5): the master is verified and its
/// repository handled; the backup's own project directory follows the
/// gateway's, so its `.git` is restored and advanced only once its project
/// export equals the artifact (`want`). The wait is bounded by
/// `--backup-timeout`; a backup that does not converge ends the node
/// `backup_unsynced` with its save kept and named, and the master is never
/// undone for it. Past the wait the node takes the master's steps: the
/// Designer gate, restore when absent, advance in place when present and equal
/// to its saved state ([`git_after_verify`]); `report` holds the backup's
/// single-node report.
///
/// The backup settles (27-35, WR-08): a `.git` advanced in place keeps its
/// save through the advance, is probed again after one `wait.policy.interval`,
/// and the save is removed only when it still equals the advanced state; a
/// `.git` the sync removed in the meantime is restored from the save and
/// advanced again, and a changed one ends the node `git_recreated` with the
/// save kept ([`settle_backup_git`]). Implemented as a backup-only option of
/// the shared step, so the master's path and timing are untouched.
pub async fn git_after_backup(
    prepared: &PreparedGit,
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
    wait: &BackupWait<'_>,
    want: &str,
) -> GitAfter {
    match wait_for_backup_project(wait, inputs.project, want).await {
        Ok(zip) => {
            let converged = AfterInputs {
                export_zip: &zip,
                ..*inputs
            };
            after_verify(
                prepared,
                report,
                saved,
                &converged,
                Some(wait.policy.interval),
            )
            .await
        }
        Err(detail) => failed_restore(
            report,
            &GitRestoreFailure {
                reason: REASON_BACKUP_UNSYNCED,
                restored: false,
                paths: Vec::new(),
                detail: format!(
                    "the backup's project did not equal the release within {} s ({detail}); \
                     its .git was not restored",
                    wait.policy.budget.as_secs()
                ),
                save_path: saved.path.clone(),
            },
        ),
    }
}

/// D-08 on the backup: its repository goes back unchanged once its project
/// has followed the master's rolled-back project (`want` is the digest the
/// master's project holds now; `None` when it could not be read, which fails
/// closed). The wait comes first because the redundancy sync removes a
/// module-cloned `.git` when the backup's content converges: a repository
/// found present before that would be called equal, its save removed, and then
/// lost. A backup that does not converge in the budget writes nothing, keeps
/// its save and is a line in `left`.
pub async fn git_rollback_backup(
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &UnverifiedInputs<'_>,
    wait: &BackupWait<'_>,
    want: Option<&str>,
) -> RollbackGit {
    let unsynced = match want {
        None => Some("the master's project digest could not be read".to_string()),
        Some(want) => wait_for_backup_project(wait, inputs.project, want)
            .await
            .err(),
    };
    if let Some(detail) = unsynced {
        let failure = GitRestoreFailure {
            reason: REASON_BACKUP_UNSYNCED,
            restored: false,
            paths: Vec::new(),
            detail: format!(
                "the backup's project did not follow the rollback within {} s ({detail}); its \
                 .git was not touched",
                wait.policy.budget.as_secs()
            ),
            save_path: saved.path.clone(),
        };
        let after = failed_restore(report, &failure);
        return RollbackGit {
            after,
            left: vec![format!(
                "project {}: the backup's git repository was not put back ({REASON_BACKUP_UNSYNCED}); \
                 the saved .git is at {} on {BACKUP}",
                inputs.project, saved.path
            )],
        };
    }
    let mut step = git_after_rollback(report, saved, inputs).await;
    // 27-37: the backup converged on the rolled-back project, so its
    // `project.json` is looked at too. Only a warning: these paths already
    // exit 6 and keep their outcome.
    if let Some(finding) = check_backup_project_json(inputs.read, inputs.project).await {
        step.after
            .warnings
            .push(backup_project_json_warning(inputs.project, &finding));
    }
    step
}

/// The `record` action's advance on the backup (27-22): the project already is
/// the artifact, so the backup is saved, then waited for (it may still be
/// converging), then handled like the master's `record`: restored when the
/// sync removed it, advanced in place otherwise. It goes through
/// [`git_after_backup`], so it gets the same settle (27-35, WR-08): the save
/// of a `.git` advanced in place goes only after one more poll interval and a
/// second look at it.
pub async fn git_record_backup(
    prepared: &PreparedGit,
    report: &mut GitReport,
    run_id: &str,
    inputs: &AfterInputs<'_>,
    wait: &BackupWait<'_>,
    want: &str,
) -> GitAfter {
    let saved = match git_before_import(
        prepared,
        inputs.read,
        inputs.write,
        inputs.git,
        inputs.gateway,
        inputs.project,
        run_id,
    )
    .await
    {
        Ok(saved) => saved,
        Err(err) => {
            let (reason, detail) = match &err {
                CoreError::PlanStale { .. } => (
                    "git_recreated",
                    "the backup's git repository changed since the plan (HEAD, refs or \
                     config); nothing was advanced"
                        .to_string(),
                ),
                other => (
                    "restore_failed",
                    format!(
                        "the backup's .git could not be saved before the advance ({}); nothing \
                         was advanced",
                        other.code()
                    ),
                ),
            };
            let node_name = report.master().node.clone();
            report.master_mut().restore = Some(RestoreInfo {
                restored: false,
                advanced: false,
                verified: false,
                post_status: None,
                post_status_note: None,
                dropped: Vec::new(),
                dropped_count: 0,
                reason: Some(reason.to_string()),
                save_kept: false,
            });
            let mut after = GitAfter::new();
            after.failed = true;
            after.warnings.push(format!(
                "project {}: git {reason} on {node_name}: {detail}",
                inputs.project
            ));
            after.events.push(ProjectsEvent::GitRestoreFailed {
                node: node_name,
                reason: reason.to_string(),
                path: None,
            });
            return after;
        }
    };
    report.set_save(&saved);
    let mut after = git_after_backup(prepared, report, &saved, inputs, wait, want).await;
    after.events.insert(
        0,
        ProjectsEvent::GitSaved {
            node: report.master().node.clone(),
            path: saved.path.clone(),
            files: saved.manifest.files,
            bytes: saved.manifest.bytes,
        },
    );
    after
}

/// The backup's entry of a published pair report as a single-node report, so
/// the per-node steps (written for the master's `nodes[0]`) run on it
/// unchanged. `None` when the report holds no backup entry.
pub fn backup_view(report: &GitReport) -> Option<GitReport> {
    let node = report.nodes.get(1)?.clone();
    Some(GitReport {
        release_commit: report.release_commit.clone(),
        nodes: vec![node],
        expected_drift: None,
        expected_drift_count: None,
        expected_drift_complete: false,
        would_discard: None,
        discarded: None,
        hazards_acknowledged: None,
    })
}

/// Fold a single-node backup report back into the published pair report. The
/// backup's drift becomes the block's only when the master advanced nothing
/// (it holds no repository). The backup's `discarded` block becomes the
/// block's only when `adopt_discard` says the backup owns the discard (the
/// master holds no discard plan, 27-35, WR-09); it replaces `would_discard`
/// only once the backup's advance wrote it.
pub fn merge_backup(report: &mut GitReport, single: GitReport, adopt_discard: bool) {
    let Some(node) = single.nodes.into_iter().next() else {
        return;
    };
    if let Some(slot) = report.nodes.get_mut(1) {
        *slot = node;
    }
    if adopt_discard && report.discarded.is_none() && single.discarded.is_some() {
        report.would_discard = None;
        report.discarded = single.discarded;
    }
    if !report.expected_drift_complete && single.expected_drift_complete {
        report.expected_drift = single.expected_drift;
        report.expected_drift_count = single.expected_drift_count;
        report.expected_drift_complete = true;
    }
}

/// Record a finished save on the backup's entry of a pair report.
pub fn set_backup_save(report: &mut GitReport, saved: &GitSaveReport) {
    if let Some(node) = report.nodes.get_mut(1) {
        node.save = Some(SaveInfo {
            path: saved.path.clone(),
            files: saved.manifest.files,
            bytes: saved.manifest.bytes,
        });
    }
}

/// `--rollback-run` on a pair (27-22, D-08): the backup's `.git` is classified
/// through its own host like the master's ([`check_rollback_run`]): an
/// unsupported shape refuses naming the backup, a managed or adoptable repo is
/// reported as `nodes[1]`. `None` when the backup holds none.
pub async fn check_rollback_backup(
    surfaces: &ProjectsSurfaces<'_>,
    backup_host: &dyn ProjectHost,
    project: &str,
) -> Result<Option<GitReport>, CoreError> {
    let entry = backup_host
        .git_entry(project)
        .await
        .map_err(|e| on_node(e, BACKUP))?;
    if !entry.is_present() {
        return Ok(None);
    }
    let host_surfaces = ProjectsSurfaces {
        gateway: surfaces.gateway,
        host: backup_host,
        tree: None,
        apply_host: None,
        deploy: None,
        tag_delete: surfaces.tag_delete.clone(),
    };
    let mut report = check_rollback_run(&host_surfaces, project, entry)
        .await
        .map_err(|e| on_node(e, BACKUP))?;
    report.master_mut().node = BACKUP.to_string();
    Ok(Some(report))
}

/// The backup node's confined git writer, its post-advance mode and its host.
pub type BackupParts = (
    Box<dyn GitStateWriter>,
    PostStatusMode,
    Box<dyn ProjectHost>,
);

/// What a run builds for the backup, before any write, so a node that cannot
/// take it refuses first: the confined git writer, the post-advance mode and
/// the host read surface.
pub fn backup_writer_and_mode(
    loaded: &LoadedProject,
    input_targets: &Path,
) -> Result<Option<BackupParts>, CoreError> {
    let Some(node) = loaded.target.nodes.backup.as_ref() else {
        return Ok(None);
    };
    let writer = git_state_writer_for_node(input_targets, &loaded.target_file, BACKUP, node)?;
    let mode = PostStatusMode::of(local_data_dir_for_node(
        input_targets,
        &loaded.target_file,
        BACKUP,
        node,
    )?);
    let host = project_host_for_node(input_targets, &loaded.target_file, BACKUP, node)?;
    Ok(Some((writer, mode, host)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::git_handover::RESTORE_REASONS;
    use crate::deploy::git_state::HOST_REASONS;

    #[test]
    fn reason_words_have_producers() {
        let words: Vec<&str> = REASON_PRODUCERS.iter().map(|(w, _)| *w).collect();
        let unique: BTreeSet<&str> = words.iter().copied().collect();
        assert_eq!(unique.len(), words.len(), "a word appears twice");
        for word in HOST_REASONS.iter().chain(RESTORE_REASONS) {
            assert!(words.contains(word), "{word} has no producer row");
        }
        for (word, producer) in REASON_PRODUCERS {
            assert!(!producer.is_empty(), "{word}");
            let here = producer.contains("(27-12");
            let known = HOST_REASONS.contains(word) || RESTORE_REASONS.contains(word);
            let later = ["27-13", "27-17", "27-22", "27-37"]
                .iter()
                .any(|plan| producer.contains(plan));
            assert!(
                known || here || later,
                "{word}: no producer in a plan of this phase ({producer})"
            );
            if here {
                assert!(
                    PRODUCED_HERE.contains(word),
                    "{word} says 27-12 produces it but is not in PRODUCED_HERE"
                );
            }
        }
        for word in PRODUCED_HERE {
            assert!(words.contains(word), "{word}");
        }
    }

    #[test]
    fn post_status_not_checked_on_ssh() {
        use crate::deploy::target::NodeTarget;
        for os in ["linux", "windows"] {
            let node: NodeTarget = serde_json::from_value(serde_json::json!({
                "profile": "dev",
                "ssh": {
                    "host": "gw.example.invalid",
                    "user": "deploy",
                    "os": os,
                    "data_dir": if os == "linux" { "/opt/ign/data" } else { "C:/ign/data" },
                    "known_hosts": "targets/known_hosts"
                }
            }))
            .expect("an ssh node parses");
            assert!(node.ssh.is_some(), "{os}");
            let mode = PostStatusMode::of(None);
            let (word, note) = mode.not_checked_words().expect("ssh is not checked");
            assert_eq!(word, "not_checked");
            assert!(note.contains("D-25") && note.contains("D-02"), "{note}");
        }
        let local = PostStatusMode::of(Some(PathBuf::from("/data")));
        assert!(local.not_checked_words().is_none());
        assert!(matches!(local, PostStatusMode::Disk(_)));
    }

    /// D-24: the dirty word of a discard has exactly two values; `unknown`
    /// is not an outcome.
    #[test]
    fn discard_dirty_words() {
        assert_eq!(
            serde_json::to_value(DiscardDirty::None).expect("json"),
            serde_json::json!("none")
        );
        assert_eq!(
            serde_json::to_value(DiscardDirty::Listed).expect("json"),
            serde_json::json!("listed")
        );
        // Nothing else can be built: an exhaustive match names every variant.
        let words: Vec<&str> = [DiscardDirty::None, DiscardDirty::Listed]
            .into_iter()
            .map(|d| match d {
                DiscardDirty::None => "none",
                DiscardDirty::Listed => "listed",
            })
            .collect();
        assert_eq!(words, ["none", "listed"]);
    }

    #[test]
    fn discard_ref_is_named_for_the_run() {
        assert_eq!(
            discard_ref_name("20261113T101500Z-ab12cd"),
            "refs/ign-discarded/20261113T101500Z-ab12cd"
        );
    }

    #[test]
    fn capped_paths_sorts_and_counts() {
        let many: Vec<String> = (0..250).map(|n| format!("p{n:04}")).rev().collect();
        let (list, count) = capped_paths(many);
        assert_eq!(count, 250);
        assert_eq!(list.len(), LIST_CAP);
        assert_eq!(list[0], "p0000");
    }
}
