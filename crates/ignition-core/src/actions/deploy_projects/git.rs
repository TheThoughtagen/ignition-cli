//! `ign deploy projects` and a gateway project the git module manages
//! (Phase 27, 27-12): the published `data.git` block, the gate and reason
//! words and the three steps around the import.
//!
//! The steps, in the order a run takes them:
//!
//! 1. [`prepare_git_host`] (right after the `.git` probe): read the gateway
//!    repo's files, classify them, check the release checkout, ask the remote
//!    for its heads and build the advance input ([`prepare_advance`]) on ign's
//!    machine. Nothing is written; a failure here refuses before any write,
//!    in `--check` and in the apply alike (B5).
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
use crate::deploy::git_advance::{AdvancePrep, prepare_advance};
use crate::deploy::git_gate::{
    GitGateFailure, LIST_CAP, NodeGates, ReleaseCheckout, RemoteView, check_release_checkout,
    gate_node, remote_view,
};
use crate::deploy::git_handover::{
    GitRestoreFailure, GitSaveReport, restore_and_advance, restore_unchanged, save,
};
use crate::deploy::git_objects::dirty_from_gateway;
use crate::deploy::git_remote::HostGit;
use crate::deploy::git_state::{GitFiles, GitHead, GitRefs, GitState, GitStateKind, classify};
use crate::deploy::project_artifact::CanonicalDigest;
use crate::deploy::transport::projects::{
    GitEntry, GitHostRead, GitStateWriter, git_entry_location,
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
    ("nodes_disagree", "pair agreement in the prepare (27-22)"),
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
    ("backup_unsynced", "backup wait (27-22)"),
    (
        "post_status_mismatch",
        "git_after_verify, post-advance disk status on a local node (27-12, D-25)",
    ),
    (
        "designer_attached",
        "git_restore_designer_gate (27-13, D-07), called before every post-import .git write; 27-21 adds git_after_rollback",
    ),
];

/// The words [`REASON_PRODUCERS`] names this module as the producer of.
pub const PRODUCED_HERE: &[&str] = &[
    "remote_mismatch",
    "release_not_on_branch",
    "post_status_mismatch",
    "designer_attached",
];

/// The restore reason of a restore that was not attempted because a Designer
/// was attached (D-07, checker W-4).
pub const REASON_DESIGNER_ATTACHED: &str = "designer_attached";

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
    /// What an override would discard (27-15); null until then.
    pub would_discard: Option<serde_json::Value>,
    /// What an override discarded (27-15); null until then.
    pub discarded: Option<serde_json::Value>,
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
    /// The gate facts, once [`prepare_git_live`] ran.
    pub gates: Option<NodeGates>,
    /// The published block.
    pub report: GitReport,
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
pub async fn prepare_git_host(
    loaded: &LoadedProject,
    input: &ProjectsInput,
    surfaces: &ProjectsSurfaces<'_>,
    entry: GitEntry,
) -> Result<Option<PreparedGit>, CoreError> {
    if !entry.is_present() {
        return Ok(None);
    }
    let project = loaded.file.project.as_str();
    let kind = entry.as_str();
    let Some(read) = surfaces.host.git() else {
        return Err(CoreError::TransportFailed {
            operation: "git".to_string(),
            node: "nodes.master".to_string(),
            detail: format!(
                "{} is present but this host cannot read it",
                git_entry_location(project)
            ),
        });
    };
    let git = surfaces.host.host_git();
    let found = read_state(read, &git, project).await?;
    if found.state.kind != GitStateKind::Managed {
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
    let view = remote_view(&git, &checkout)
        .await
        .map_err(|e| gate_failure(e, project, kind))?;
    let members: BTreeSet<String> = loaded.artifact.digest.members.keys().cloned().collect();
    // B5: the pack, its base and the release blobs are built here, before any
    // write, in `--check` and in the apply alike.
    let prep = prepare_advance(
        &git,
        &input.repo,
        &found.state,
        &refs,
        &checkout.commit,
        &members,
    )
    .await
    .map_err(|e| preflight("git_host", crate::deploy::git_gate::detail_of(&e)))?;
    let report = GitReport {
        release_commit: checkout.commit.clone(),
        nodes: vec![NodeGitReport {
            node: "nodes.master".to_string(),
            state: state_word(found.state.kind),
            reason: None,
            branch: found.state.tracked.clone(),
            branch_source: Some("config"),
            remote: Some(remote.name.clone()),
            remote_url: Some(remote.url_redacted.clone()),
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
    };
    Ok(Some(PreparedGit {
        mode: GitMode::Enforce,
        state: found.state,
        refs,
        checkout,
        view,
        prep: Some(prep),
        gates: None,
        report,
        kind: kind.to_string(),
        head_log_tail: found.files.head_log_tail,
    }))
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
pub async fn prepare_git_live(
    prepared: &mut PreparedGit,
    live: Option<&CanonicalDigest>,
    action: ProjectAction,
    loaded: &LoadedProject,
    surfaces: &ProjectsSurfaces<'_>,
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
    let Some(read) = surfaces.host.git() else {
        return Err(CoreError::TransportFailed {
            operation: "git".to_string(),
            node: "nodes.master".to_string(),
            detail: format!(
                "{} is present but this host cannot read it",
                git_entry_location(project)
            ),
        });
    };
    let git = surfaces.host.host_git();
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
    let enforce = prepared.mode == GitMode::Enforce
        && matches!(action, ProjectAction::Import | ProjectAction::Record);
    if enforce {
        if !gates.release_on_branch {
            return Err(managed(project, &kind, "release_not_on_branch"));
        }
        if !gates.unpushed.is_empty() {
            return Err(CoreError::DestructiveChangeRefused {
                gate: "git_unpushed".to_string(),
                count: gates.unpushed.len(),
                keys: gates.unpushed.clone(),
            });
        }
        if gates.not_merged_count > 0 {
            return Err(CoreError::DestructiveChangeRefused {
                gate: "git_not_merged".to_string(),
                count: gates.not_merged_count,
                keys: gates.not_merged.clone(),
            });
        }
        if action == ProjectAction::Import && gates.dirty_count > 0 {
            return Err(CoreError::DestructiveChangeRefused {
                gate: "git_dirty".to_string(),
                count: gates.dirty_count,
                keys: gates.dirty.clone().unwrap_or_default(),
            });
        }
    }
    prepared.gates = Some(gates);
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
    let now = read_state(read, git, project).await?;
    if now.state != prepared.state || now.refs.as_ref() != Some(&prepared.refs) {
        return Err(CoreError::PlanStale {
            expected: head_commit(&prepared.state).unwrap_or_else(|| "none".to_string()),
            actual: head_commit(&now.state).unwrap_or_else(|| "none".to_string()),
            detail: format!(
                "the git repository of project {project} changed since the plan (HEAD, refs or \
                 config)"
            ),
        });
    }
    save(read, write, gateway, project, run_id).await
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
    let mut polls = Vec::new();
    let gate =
        git_restore_designer_gate(inputs.api, inputs.designers, &saved.path, &mut polls).await;
    let mut after = match gate {
        Err(failure) => failed_restore(report, &failure),
        Ok(()) => advance_after(prepared, report, saved, inputs).await,
    };
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
async fn advance_after(
    prepared: &PreparedGit,
    report: &mut GitReport,
    saved: &GitSaveReport,
    inputs: &AfterInputs<'_>,
) -> GitAfter {
    let save_path = saved.path.clone();
    let node_name = report.master().node.clone();
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
    let done = match restore_and_advance(
        inputs.read,
        inputs.write,
        inputs.git,
        &prepared.checkout.path,
        &prepared.state,
        saved,
        prep,
        inputs.export_zip,
    )
    .await
    {
        Ok(done) => done,
        Err(failure) => return failed_restore(report, &failure),
    };

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
                    restore.reason = Some(REASON_POST_STATUS_MISMATCH.to_string());
                    after.failed = true;
                    after.warnings.push(format!(
                        "project {}: git {REASON_POST_STATUS_MISMATCH}: the repository was advanced \
                         but the project directory does not match the predicted status; the \
                         saved .git is at {save_path}",
                        inputs.project
                    ));
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
                }
            }
        }
    }
    report.master_mut().restore = Some(restore);
    if after.failed {
        after.events.push(ProjectsEvent::GitRestoreFailed {
            node: node_name,
            reason: REASON_POST_STATUS_MISMATCH.to_string(),
            path: Some(save_path),
        });
    } else {
        remove_save(report, saved, inputs, &mut after).await;
    }
    report.expected_drift = Some(drift);
    report.expected_drift_count = Some(drift_count);
    report.expected_drift_complete = true;
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
        Ok(()) => {
            match restore_unchanged(
                inputs.read,
                inputs.write,
                inputs.gateway,
                inputs.project,
                saved,
            )
            .await
            {
                Err(failure) => failed_restore(report, &failure),
                Ok(done) => {
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
                    } else {
                        after.warnings.push(format!(
                            "project {}: the import left a git repository that is not the one \
                             saved; it was left as it is and the saved .git is at {}",
                            inputs.project, saved.path
                        ));
                    }
                    after
                }
            }
        }
    };
    after.events.splice(
        0..0,
        polls
            .into_iter()
            .map(|open| ProjectsEvent::DesignersPoll { open }),
    );
    after
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
            let later = ["27-13", "27-17", "27-22"]
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

    #[test]
    fn capped_paths_sorts_and_counts() {
        let many: Vec<String> = (0..250).map(|n| format!("p{n:04}")).rev().collect();
        let (list, count) = capped_paths(many);
        assert_eq!(count, 250);
        assert_eq!(list.len(), LIST_CAP);
        assert_eq!(list[0], "p0000");
    }
}
