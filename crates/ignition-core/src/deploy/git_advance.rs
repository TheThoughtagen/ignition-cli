//! The advance of a managed gateway repo to the release commit (Phase 27).
//!
//! After an import the gateway's `data/projects/<p>/.git` still describes the
//! previous commit while the work tree holds the release. The advance moves
//! the repo to the release commit R so the git module's own status, commit and
//! push see the release as the baseline: R's missing objects arrive as a pack,
//! the tracked branch and its remote-tracking ref name R, and the index
//! describes R's tree with assume-unchanged on every path the gateway does not
//! hold on disk.
//!
//! The work splits in two so nothing that can fail runs after a gateway write:
//!
//! - [`prepare_advance`] runs on ign's machine, before anything is written to
//!   a gateway. It computes everything that needs objects: the pack and its
//!   base, the release tree and the release blobs of every in-scope path. A
//!   host-git failure is `deploy_preflight_failed` check `git_host`.
//! - [`finish_advance`] runs after the import, when the read-back export shows
//!   what the gateway actually kept. It builds the assume-unchanged set and the
//!   index from that export (never from the artifact: the gateway can drop an
//!   in-scope file on import, which would otherwise read as a deletion), checks
//!   host git's status over the extracted export against the prediction, and
//!   returns the [`AdvancePlan`]: fixed [`GitWrite`]s in the order the host
//!   applies them (pack, pack index, index, config on an adoption, remote-tracking
//!   ref, branch ref, HEAD: the index before every ref, config and HEAD, 27-26).
//!   It writes nothing.
//!
//! A partial repo the module left behind (an unborn `HEAD`, no local branch, no
//! index; [`GitStateKind::Adoptable`]) is advanced the same way under
//! [`AdvanceMode::Adopt`] (27-17, D-13): the branch the caller names gets the
//! local and the remote-tracking ref, `HEAD` points at it, the index is written,
//! and a `[branch "<b>"]` section is added to the gateway `config`. That config
//! is built in [`prepare_advance_with`] with host git on a private copy, before
//! any gateway write, like the pack.
//!
//! The base of the pack is every commit id the gateway's own refs name that
//! ign's store holds ([`pack_bases`]); an id ign's store lacks (an unpushed
//! gateway HEAD) is never passed to `pack-objects`, and with no usable base the
//! pack carries the release's full history.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::apply::run_store::RunId;
use super::git_remote::{HostGit, PackFiles, host_failure, is_object_id, safe_ref_name};
use super::git_state::{GitRefs, GitState, GitStateKind};
use crate::actions::projects::IMPORT_MAX_BYTES;
use crate::error::CoreError;

/// Most paths a mismatch names.
pub const MISMATCH_PATHS_MAX: usize = 200;

/// A gateway file an advance writes, by shape. Only these names exist: an
/// advance cannot name any other path below `.git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitWriteName {
    /// `objects/pack/pack-<h>.pack`
    PackFile(String),
    /// `objects/pack/pack-<h>.idx`
    PackIndex(String),
    /// `refs/heads/<branch>`
    BranchRef(String),
    /// `refs/remotes/<remote>/<branch>`
    RemoteRef {
        /// The remote's name.
        remote: String,
        /// The branch.
        branch: String,
    },
    /// `refs/ign-discarded/<run-id>`: the keep-alive ref of a run that
    /// discards gateway commits (27-15, D-05). It names the abandoned HEAD so
    /// the commits stay reachable after the branch moves to the release.
    DiscardRef(String),
    /// `config`
    Config,
    /// `HEAD`
    Head,
    /// `index`
    Index,
}

impl GitWriteName {
    /// The path below `.git`, `/`-separated.
    pub fn rel(&self) -> String {
        match self {
            GitWriteName::PackFile(h) => format!("objects/pack/pack-{h}.pack"),
            GitWriteName::PackIndex(h) => format!("objects/pack/pack-{h}.idx"),
            GitWriteName::BranchRef(b) => format!("refs/heads/{b}"),
            GitWriteName::RemoteRef { remote, branch } => {
                format!("refs/remotes/{remote}/{branch}")
            }
            GitWriteName::DiscardRef(run_id) => format!("refs/ign-discarded/{run_id}"),
            GitWriteName::Config => "config".to_string(),
            GitWriteName::Head => "HEAD".to_string(),
            GitWriteName::Index => "index".to_string(),
        }
    }

    /// Refuse a name whose parts are not a hex pack name or a safe branch or
    /// remote name.
    pub fn check_shape(&self) -> Result<(), &'static str> {
        match self {
            GitWriteName::PackFile(h) | GitWriteName::PackIndex(h) => {
                if is_object_id(h) {
                    Ok(())
                } else {
                    Err("a pack name is not a full hex digest")
                }
            }
            GitWriteName::BranchRef(b) => {
                if safe_ref_name(&format!("refs/heads/{b}")) {
                    Ok(())
                } else {
                    Err("the tracked branch is not a safe ref name")
                }
            }
            GitWriteName::RemoteRef { remote, branch } => {
                if !remote.contains('/')
                    && safe_ref_name(&format!("refs/remotes/{remote}/{branch}"))
                {
                    Ok(())
                } else {
                    Err("the remote or branch is not a safe ref name")
                }
            }
            GitWriteName::DiscardRef(run_id) => {
                if RunId::parse(run_id).is_ok()
                    && safe_ref_name(&format!("refs/ign-discarded/{run_id}"))
                {
                    Ok(())
                } else {
                    Err("the run id is not a safe ref name")
                }
            }
            GitWriteName::Config | GitWriteName::Head | GitWriteName::Index => Ok(()),
        }
    }
}

/// One file to write below the gateway repo's `.git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWrite {
    /// Which file.
    pub name: GitWriteName,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

/// The commit ids the pack is built against: the gateway's HEAD, its
/// `refs/heads/*` and its `refs/remotes/*` values that ign's store holds,
/// deduplicated and byte-sorted. An id the store lacks (an unpushed HEAD) is
/// dropped, never passed as `^id`; the gateway holds every object reachable
/// from the ids kept, so excluding them never leaves an object missing.
pub async fn pack_bases(
    git: &HostGit,
    checkout: &Path,
    refs: &GitRefs,
) -> Result<Vec<String>, CoreError> {
    let mut candidates: BTreeSet<&str> = BTreeSet::new();
    if let Some(head) = refs.head.commit() {
        candidates.insert(head);
    }
    candidates.extend(refs.heads.values().map(String::as_str));
    candidates.extend(refs.remotes.values().map(String::as_str));
    let mut bases = Vec::new();
    for id in candidates {
        if is_object_id(id) && git.has_commit(checkout, id).await? {
            bases.push(id.to_string());
        }
    }
    Ok(bases)
}

/// What kind of repo an advance starts from and so what it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceMode {
    /// A managed repo: the tracked branch moves to the release commit.
    FastForward,
    /// A partial repo (27-17, D-13): the named branch is created at the
    /// release commit, with its remote-tracking ref, and `config` gains the
    /// branch's upstream section.
    Adopt {
        /// The branch to adopt onto (an explicit, validated name).
        branch: String,
        /// The one remote's name in the gateway config.
        remote: String,
        /// The gateway `config` as the prepare read it.
        config: Vec<u8>,
    },
}

/// The lowercase hex sha256 of `bytes`: how a config is recognised later.
pub fn config_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// What [`prepare_advance`] computed before any gateway write. It holds no
/// index and no assume-unchanged set: those depend on what the gateway kept,
/// which only the read-back export shows.
#[derive(Debug, Clone)]
pub struct AdvancePrep {
    /// The tracked branch (the adoption branch of an adopted repo).
    pub branch: String,
    /// The remote's name in the gateway config.
    pub remote: String,
    /// The release commit R.
    pub release_commit: String,
    /// The pack of R's objects the gateway lacks; `None` when it holds them.
    pub pack: Option<PackFiles>,
    /// The base ids the pack excludes.
    pub pack_bases: Vec<String>,
    /// Every path of R's tree, byte-sorted.
    pub tree_paths: Vec<String>,
    /// The artifact's members: the in-scope paths the import writes.
    pub artifact_members: BTreeSet<String>,
    /// R's bytes of every in-scope tree path (tree intersected with the
    /// artifact members).
    pub release_blobs: BTreeMap<String, Vec<u8>>,
    /// The repo is adopted rather than fast-forwarded (27-17).
    pub adopted: bool,
    /// The gateway `config` with the adoption branch's upstream section, built
    /// before any write; `None` for a fast-forward.
    pub config: Option<Vec<u8>>,
    /// The digest of the config [`config`](Self::config) was built from: the
    /// handover refuses to write it over any other config.
    pub config_base: Option<String>,
}

/// Compute everything of an advance that needs objects, on ign's machine,
/// before any write to a gateway. `state` must be managed. A fast-forward of
/// a managed repo; [`prepare_advance_with`] takes the mode.
pub async fn prepare_advance(
    git: &HostGit,
    checkout: &Path,
    state: &GitState,
    refs: &GitRefs,
    release_commit: &str,
    artifact_members: &BTreeSet<String>,
) -> Result<AdvancePrep, CoreError> {
    prepare_advance_with(
        git,
        checkout,
        state,
        refs,
        release_commit,
        artifact_members,
        AdvanceMode::FastForward,
    )
    .await
}

/// [`prepare_advance`] for either kind of repo. [`AdvanceMode::FastForward`]
/// needs a managed `state`; [`AdvanceMode::Adopt`] needs an adoptable one whose
/// one remote is the mode's, and builds the adopted config here (the pack comes
/// from the same [`pack_bases`] rule, which for a partial repo means the
/// remote-tracking ids of its `packed-refs` that ign's store holds).
pub async fn prepare_advance_with(
    git: &HostGit,
    checkout: &Path,
    state: &GitState,
    refs: &GitRefs,
    release_commit: &str,
    artifact_members: &BTreeSet<String>,
    mode: AdvanceMode,
) -> Result<AdvancePrep, CoreError> {
    let wrong_state = |want: &str| CoreError::InvalidInput {
        reason: format!(
            "this advance needs {want}, not a {} one",
            state.kind.as_str()
        ),
    };
    let Some(remote) = &state.remote else {
        return Err(wrong_state("a gateway repo with one remote"));
    };
    let (branch, config, config_base, adopted) = match &mode {
        AdvanceMode::FastForward => {
            if state.kind != GitStateKind::Managed {
                return Err(wrong_state("a managed gateway repo"));
            }
            let Some(branch) = &state.tracked else {
                return Err(wrong_state("a managed gateway repo"));
            };
            (branch.clone(), None, None, false)
        }
        AdvanceMode::Adopt {
            branch,
            remote: named,
            config,
        } => {
            if state.kind != GitStateKind::Adoptable || state.tracked.is_some() {
                return Err(wrong_state("an adoptable gateway repo"));
            }
            if named != &remote.name {
                return Err(CoreError::InvalidInput {
                    reason: "the adoption remote is not the gateway repo's one remote".to_string(),
                });
            }
            let adopted = git
                .adopt_branch_config(config, branch, &remote.name)
                .await?;
            (
                branch.clone(),
                Some(adopted),
                Some(config_digest(config)),
                true,
            )
        }
    };
    let branch = &branch;
    if !is_object_id(release_commit) {
        return Err(host_failure("the release commit is not a full hex id"));
    }
    for name in [
        GitWriteName::BranchRef(branch.clone()),
        GitWriteName::RemoteRef {
            remote: remote.name.clone(),
            branch: branch.clone(),
        },
    ] {
        name.check_shape().map_err(host_failure)?;
    }
    let tree_paths = git.tree_paths(checkout, release_commit).await?;
    let bases = pack_bases(git, checkout, refs).await?;
    let pack = git.build_pack(checkout, release_commit, &bases).await?;
    let in_tree: BTreeSet<&str> = tree_paths.iter().map(String::as_str).collect();
    let wanted: Vec<String> = artifact_members
        .iter()
        .filter(|m| in_tree.contains(m.as_str()))
        .cloned()
        .collect();
    let release_blobs = git.blobs(checkout, release_commit, &wanted).await?;
    Ok(AdvancePrep {
        branch: branch.clone(),
        remote: remote.name.clone(),
        release_commit: release_commit.to_string(),
        pack,
        pack_bases: bases,
        tree_paths,
        artifact_members: artifact_members.clone(),
        release_blobs,
        adopted,
        config,
        config_base,
    })
}

/// How the read-back export differs from the release, per path (all lists
/// byte-sorted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriftPrediction {
    /// Release tree paths whose export bytes differ from the release blob.
    pub modified: Vec<String>,
    /// Artifact members absent from the release tree that the export holds:
    /// export-only, which a git status lists as untracked.
    pub untracked: Vec<String>,
    /// Export members that are not artifact members.
    pub unexpected: Vec<String>,
    /// In-scope release files the gateway does not hold after the import:
    /// artifact members the export lacks (the gateway did not keep them) and
    /// files inside the artifact's own directories that the artifact left out
    /// (a file no `resource.json` lists, a denied or `.ignexclude`d path),
    /// which the gateway never receives. All are assume-unchanged, never
    /// drift (checker B3).
    pub dropped: Vec<String>,
}

/// Classify the read-back export against the release tree and the artifact.
pub fn predict_drift(
    prep: &AdvancePrep,
    export_members: &BTreeMap<String, Vec<u8>>,
) -> DriftPrediction {
    let in_tree: BTreeSet<&str> = prep.tree_paths.iter().map(String::as_str).collect();
    let mut prediction = DriftPrediction::default();
    for (path, bytes) in export_members {
        if !prep.artifact_members.contains(path) {
            prediction.unexpected.push(path.clone());
        } else if in_tree.contains(path.as_str()) {
            if prep.release_blobs.get(path) != Some(bytes) {
                prediction.modified.push(path.clone());
            }
        } else {
            prediction.untracked.push(path.clone());
        }
    }
    // The artifact's roots are the first segments of its members (`ignition`,
    // `com.inductiveautomation.perspective`, ...); a release file below one of
    // them that is not an artifact member and not in the export is in scope
    // but never reaches the gateway. A path outside every root (README, tags,
    // images, deploy) is out of scope and not reported.
    let roots: BTreeSet<&str> = prep
        .artifact_members
        .iter()
        .filter_map(|m| m.split_once('/').map(|(root, _)| root))
        .collect();
    let mut dropped: BTreeSet<String> = prep
        .artifact_members
        .iter()
        .filter(|m| !export_members.contains_key(*m))
        .cloned()
        .collect();
    dropped.extend(
        prep.tree_paths
            .iter()
            .filter(|p| !prep.artifact_members.contains(*p) && !export_members.contains_key(*p))
            .filter(|p| {
                p.split_once('/')
                    .is_some_and(|(root, _)| roots.contains(root))
            })
            .cloned(),
    );
    prediction.dropped = dropped.into_iter().collect();
    prediction
}

/// The advance stopped before any write because the export is not what the
/// prediction says (reason word `status_unexpected` for callers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceMismatch {
    /// The paths that differ, byte-sorted, at most [`MISMATCH_PATHS_MAX`].
    pub unexpected: Vec<String>,
    /// How many paths differ in all.
    pub count: usize,
}

impl AdvanceMismatch {
    fn of(paths: BTreeSet<String>) -> AdvanceMismatch {
        let count = paths.len();
        AdvanceMismatch {
            unexpected: paths.into_iter().take(MISMATCH_PATHS_MAX).collect(),
            count,
        }
    }
}

/// Why [`finish_advance`] produced no plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceFailure {
    /// The export holds members outside the artifact, or host git's status
    /// over it differs from the prediction.
    Mismatch(AdvanceMismatch),
    /// Host git or the local temp files failed; nothing was written.
    HostGit {
        /// A redacted description.
        detail: String,
    },
}

impl fmt::Display for AdvanceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdvanceFailure::Mismatch(m) => write!(
                f,
                "{} path(s) differ from the predicted status: {}",
                m.count,
                m.unexpected.join(", ")
            ),
            AdvanceFailure::HostGit { detail } => f.write_str(detail),
        }
    }
}

impl std::error::Error for AdvanceFailure {}

impl From<CoreError> for AdvanceFailure {
    fn from(err: CoreError) -> Self {
        AdvanceFailure::HostGit {
            detail: err.to_string(),
        }
    }
}

/// A finished advance: the writes, in the order the host applies them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancePlan {
    /// The tracked branch.
    pub branch: String,
    /// The remote's name.
    pub remote: String,
    /// The release commit.
    pub release_commit: String,
    /// In the order the host applies them and stops at the first failure:
    /// pack, pack index, index, config (adoption only), remote-tracking ref,
    /// discard ref (override only), branch ref, HEAD. The index is before
    /// every ref, config and HEAD write, so a write that fails part way leaves
    /// HEAD and the branch at the old commit over an index that holds the
    /// release (staged changes, never a revert of the deploy).
    pub writes: Vec<GitWrite>,
    /// Release tree paths the export does not hold, byte-sorted; marked
    /// assume-unchanged in the index.
    pub assume_unchanged: Vec<String>,
    /// Artifact members the gateway did not keep, byte-sorted.
    pub dropped: Vec<String>,
    /// What a git status over the advanced repo lists: modified plus
    /// untracked, byte-sorted.
    pub expected_drift: Vec<String>,
}

impl AdvancePlan {
    /// Keep the gateway commits this run abandons reachable (27-15, D-05): add
    /// the write of `refs/ign-discarded/<run_id>` naming `abandoned_head`,
    /// right before the branch ref (before HEAD when the plan has no branch
    /// ref, at the end when it has no HEAD either). ign writes no reflog, so
    /// if the branch moved first and this write then failed, the abandoned
    /// commits would be left without a ref (27-26, WR-A1). The pack is unchanged: the abandoned
    /// commits' objects are already in the gateway repo, and a ref naming them
    /// keeps them there. A run id that is not a safe ref name, or a head that
    /// is not a full object id, adds nothing and is refused.
    pub fn keep_discarded(
        &mut self,
        run_id: &str,
        abandoned_head: &str,
    ) -> Result<(), &'static str> {
        if !is_object_id(abandoned_head) {
            return Err("the abandoned HEAD is not a full hex commit id");
        }
        let name = GitWriteName::DiscardRef(run_id.to_string());
        name.check_shape()?;
        let at = self
            .writes
            .iter()
            .position(|w| matches!(w.name, GitWriteName::BranchRef(_)))
            .or_else(|| {
                self.writes
                    .iter()
                    .position(|w| matches!(w.name, GitWriteName::Head))
            })
            .unwrap_or(self.writes.len());
        self.writes.insert(
            at,
            GitWrite {
                name,
                bytes: format!("{abandoned_head}\n").into_bytes(),
            },
        );
        Ok(())
    }
}

/// Build the advance from the read-back export (`export_zip`). Local work on
/// ign's machine: it reads the export and runs host git on temp files, and
/// touches no gateway file, so a failure leaves the gateway repo as it was.
///
/// In order: unpack the export; [`predict_drift`]; a member outside the
/// artifact is a mismatch; assume-unchanged is the release tree minus the
/// export's members; build the index; host git's status over the extracted
/// export must equal the prediction (modified and untracked) with no deleted
/// path. A non-empty `dropped` alone is not a mismatch.
pub async fn finish_advance(
    git: &HostGit,
    checkout: &Path,
    prep: &AdvancePrep,
    export_zip: &[u8],
) -> Result<AdvancePlan, AdvanceFailure> {
    let (export, duplicates) = read_export(export_zip)?;
    let prediction = predict_drift(prep, &export);
    let mut unexpected: BTreeSet<String> = prediction.unexpected.iter().cloned().collect();
    unexpected.extend(duplicates);
    if !unexpected.is_empty() {
        return Err(AdvanceFailure::Mismatch(AdvanceMismatch::of(unexpected)));
    }

    let assume_unchanged: Vec<String> = prep
        .tree_paths
        .iter()
        .filter(|p| !export.contains_key(*p))
        .cloned()
        .collect();
    let index = git
        .build_index(checkout, &prep.release_commit, &assume_unchanged)
        .await?;

    let work = tempfile::tempdir().map_err(|e| AdvanceFailure::HostGit {
        detail: format!("a private temp dir could not be made: {e}"),
    })?;
    for (path, bytes) in &export {
        write_member(work.path(), path, bytes)?;
    }
    let status = git.status_over(checkout, &index, work.path()).await?;
    let mut differ: BTreeSet<String> = BTreeSet::new();
    differ.extend(sym_diff(&status.modified, &prediction.modified));
    differ.extend(sym_diff(&status.untracked, &prediction.untracked));
    differ.extend(status.deleted.iter().cloned());
    if !differ.is_empty() {
        return Err(AdvanceFailure::Mismatch(AdvanceMismatch::of(differ)));
    }

    let mut writes = Vec::new();
    if let Some(pack) = &prep.pack {
        writes.push(GitWrite {
            name: GitWriteName::PackFile(pack.name.clone()),
            bytes: pack.pack.clone(),
        });
        writes.push(GitWrite {
            name: GitWriteName::PackIndex(pack.name.clone()),
            bytes: pack.idx.clone(),
        });
    }
    // The index goes before every ref, config and HEAD write (27-26, WR-A1,
    // D-06, D-10). A write that fails part way stops the host with the
    // earlier ones applied and no undo: with the index first, HEAD and the
    // branch still name the old commit and the index holds the release, which
    // reads as the release's changes staged on the old commit. The old order
    // (index last) left the branch at the release over the old index, or HEAD
    // on a branch with no index, which a Designer commit or discard turns into
    // a revert of the deploy.
    writes.push(GitWrite {
        name: GitWriteName::Index,
        bytes: index,
    });
    let commit_line = format!("{}\n", prep.release_commit).into_bytes();
    // An adopted repo's upstream section: after the index, before the refs and
    // HEAD, so a reader that sees HEAD on the branch sees the section.
    if let Some(config) = &prep.config {
        writes.push(GitWrite {
            name: GitWriteName::Config,
            bytes: config.clone(),
        });
    }
    writes.push(GitWrite {
        name: GitWriteName::RemoteRef {
            remote: prep.remote.clone(),
            branch: prep.branch.clone(),
        },
        bytes: commit_line.clone(),
    });
    // `keep_discarded` puts the discard ref of an override right before this.
    writes.push(GitWrite {
        name: GitWriteName::BranchRef(prep.branch.clone()),
        bytes: commit_line,
    });
    writes.push(GitWrite {
        name: GitWriteName::Head,
        bytes: format!("ref: refs/heads/{}\n", prep.branch).into_bytes(),
    });
    for write in &writes {
        write
            .name
            .check_shape()
            .map_err(|why| AdvanceFailure::HostGit {
                detail: why.to_string(),
            })?;
    }

    let mut expected_drift: Vec<String> = prediction
        .modified
        .iter()
        .chain(&prediction.untracked)
        .cloned()
        .collect();
    expected_drift.sort();
    Ok(AdvancePlan {
        branch: prep.branch.clone(),
        remote: prep.remote.clone(),
        release_commit: prep.release_commit.clone(),
        writes,
        assume_unchanged,
        dropped: prediction.dropped,
        expected_drift,
    })
}

/// The paths in exactly one of two byte-sorted lists.
fn sym_diff(a: &[String], b: &[String]) -> Vec<String> {
    let a: BTreeSet<&String> = a.iter().collect();
    let b: BTreeSet<&String> = b.iter().collect();
    a.symmetric_difference(&b).map(|s| (*s).clone()).collect()
}

/// The file members of an export zip and the names that appear twice.
type ReadExport = (BTreeMap<String, Vec<u8>>, Vec<String>);

/// Read the file members of the export zip.
fn read_export(zip_bytes: &[u8]) -> Result<ReadExport, AdvanceFailure> {
    let host = |detail: String| AdvanceFailure::HostGit { detail };
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| host(format!("the read-back export is not a readable zip: {e}")))?;
    let mut members = BTreeMap::new();
    let mut duplicates = Vec::new();
    let mut total = 0usize;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| host(format!("the read-back export cannot be walked: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let room = IMPORT_MAX_BYTES.saturating_sub(total);
        let mut bytes = Vec::new();
        entry
            .take(room as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| host(format!("the read-back export cannot be read: {e}")))?;
        total += bytes.len();
        if total > IMPORT_MAX_BYTES {
            return Err(host(format!(
                "the read-back export is over the {} MiB limit",
                IMPORT_MAX_BYTES / (1024 * 1024)
            )));
        }
        if members.insert(name.clone(), bytes).is_some() {
            duplicates.push(name);
        }
    }
    Ok((members, duplicates))
}

/// Write one export member below `root`; only plain relative paths.
fn write_member(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), AdvanceFailure> {
    let path = Path::new(rel);
    let plain = !rel.is_empty()
        && !rel.contains('\\')
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
    if !plain {
        return Err(AdvanceFailure::HostGit {
            detail: format!(
                "an export member has an unsafe path (`{}`)",
                rel.escape_debug()
            ),
        });
    }
    let target = root.join(path);
    let write = || -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)
    };
    write().map_err(|e| AdvanceFailure::HostGit {
        detail: format!("an export member could not be extracted: {e}"),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::deploy::git_state::{GitFiles, GitHead, classify};
    use crate::deploy::transport::projects::GitEntry;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::Command;

    pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub(crate) fn put(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write");
    }

    fn in_scope(path: &str) -> bool {
        path == "project.json"
            || path.starts_with("ignition/")
            || path.starts_with("com.inductiveautomation.")
    }

    pub(crate) const CODE: &str = "ignition/script-python/cap/util/code.py";
    const UTIL_RES: &str = "ignition/script-python/cap/util/resource.json";
    const NOTES: &str = "ignition/script-python/cap/util/NOTES.txt";
    const NEW_CODE: &str = "ignition/script-python/cap/util2/code.py";
    const LOCAL_ONLY: &str = "ignition/script-python/cap/util/local_only.py";

    pub(crate) struct Fixture {
        _dir: tempfile::TempDir,
        pub(crate) seed: PathBuf,
        pub(crate) gw: PathBuf,
        pub(crate) c1: String,
        pub(crate) c2: String,
    }

    impl Fixture {
        /// A bare remote, a release checkout holding C1 and C2, and a plain
        /// clone at C1 standing in for the gateway repo.
        pub(crate) fn new() -> Fixture {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path();
            let remote = root.join("capapp.git");
            let seed = root.join("seed");
            let gw = root.join("gw");
            std::fs::create_dir(&remote).expect("mkdir");
            std::fs::create_dir(&seed).expect("mkdir");
            git(&remote, &["init", "-q", "--bare", "-b", "production"]);
            git(&seed, &["init", "-q", "-b", "production"]);
            git(
                &seed,
                &["remote", "add", "origin", remote.to_str().expect("utf8")],
            );
            put(&seed, "project.json", "{\"title\":\"CapApp\"}\n");
            put(&seed, CODE, "def f():\n    return 1\n");
            put(
                &seed,
                UTIL_RES,
                "{\"scope\":\"A\",\"files\":[\"code.py\"]}\n",
            );
            put(&seed, NOTES, "notes\n");
            put(
                &seed,
                "com.inductiveautomation.perspective/views/V/view.json",
                "{\"root\":{}}\n",
            );
            put(
                &seed,
                "com.inductiveautomation.perspective/views/V/resource.json",
                "{\"scope\":\"G\",\"files\":[\"view.json\"]}\n",
            );
            put(&seed, "README.md", "readme\n");
            put(&seed, "tags/tags.json", "{\"tags\":[]}\n");
            git(&seed, &["add", "-A"]);
            git(&seed, &["commit", "-q", "-m", "one"]);
            let c1 = git(&seed, &["rev-parse", "HEAD"]);
            git(&seed, &["push", "-q", "origin", "production"]);
            git(
                root,
                &[
                    "clone",
                    "-q",
                    "-b",
                    "production",
                    remote.to_str().expect("utf8"),
                    "gw",
                ],
            );
            put(&seed, CODE, "def f():\n    return 2\n");
            put(&seed, NEW_CODE, "def g():\n    return 3\n");
            put(
                &seed,
                "ignition/script-python/cap/util2/resource.json",
                "{\"scope\":\"A\",\"files\":[\"code.py\"]}\n",
            );
            git(&seed, &["add", "-A"]);
            git(&seed, &["commit", "-q", "-m", "two"]);
            let c2 = git(&seed, &["rev-parse", "HEAD"]);
            git(&seed, &["push", "-q", "origin", "production"]);
            Fixture {
                _dir: dir,
                seed,
                gw,
                c1,
                c2,
            }
        }

        pub(crate) fn git_dir(&self) -> PathBuf {
            self.gw.join(".git")
        }

        /// The gateway's state, read through `HostGit` as a host read would.
        pub(crate) async fn gateway_state(&self, host: &HostGit) -> (GitState, GitRefs) {
            let files = read_git_files(&self.git_dir());
            files.check_caps().expect("caps");
            let config = host
                .parse_config(files.config.as_deref().expect("config"))
                .await
                .expect("config");
            let refs = host
                .parse_refs(&files)
                .await
                .expect("refs")
                .expect("some refs");
            let state = classify(&files, Some(&config), Some(&refs));
            (state, refs)
        }

        /// The in-scope tree paths of C2 plus any `extra` members.
        pub(crate) async fn members(&self, host: &HostGit, extra: &[&str]) -> BTreeSet<String> {
            let mut members: BTreeSet<String> = host
                .tree_paths(&self.seed, &self.c2)
                .await
                .expect("tree")
                .into_iter()
                .filter(|p| in_scope(p))
                .collect();
            members.extend(extra.iter().map(|e| (*e).to_string()));
            members
        }

        /// Replace the gateway work tree with what an import of C2 leaves:
        /// the in-scope files of C2, minus `drop`, plus `extra`, with
        /// `pretty` re-serialised the gateway way. Returns the export zip.
        pub(crate) async fn import(
            &self,
            host: &HostGit,
            drop: &[&str],
            extra: &[(&str, &str)],
            pretty: Option<&str>,
        ) -> Vec<u8> {
            for entry in std::fs::read_dir(&self.gw).expect("read_dir") {
                let path = entry.expect("entry").path();
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).expect("rm");
                } else {
                    std::fs::remove_file(&path).expect("rm");
                }
            }
            let paths: Vec<String> = host
                .tree_paths(&self.seed, &self.c2)
                .await
                .expect("tree")
                .into_iter()
                .filter(|p| in_scope(p) && !drop.contains(&p.as_str()))
                .collect();
            let blobs = host
                .blobs(&self.seed, &self.c2, &paths)
                .await
                .expect("blobs");
            let mut files: BTreeMap<String, Vec<u8>> = blobs;
            for (name, body) in extra {
                files.insert((*name).to_string(), body.as_bytes().to_vec());
            }
            if let Some(path) = pretty {
                let text = String::from_utf8(files[path].clone()).expect("utf8");
                let pretty = text.replace('{', "{\n  ").replace('}', "\n}");
                files.insert(path.to_string(), pretty.into_bytes());
            }
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (name, bytes) in &files {
                let path = self.gw.join(name);
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(&path, bytes).expect("write");
                writer
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .expect("start");
                writer.write_all(bytes).expect("zip write");
            }
            writer.finish().expect("finish").into_inner()
        }

        /// What the host does with an advance: write each file in order.
        fn apply(&self, plan: &AdvancePlan) {
            for write in &plan.writes {
                let path = self.git_dir().join(write.name.rel());
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(path, &write.bytes).expect("write");
            }
        }

        /// `git status --porcelain` paths of the gateway repo, byte-sorted.
        fn status_paths(&self) -> Vec<String> {
            let text = git(
                &self.gw,
                &["status", "--porcelain", "--untracked-files=all"],
            );
            let mut paths: Vec<String> = text
                .lines()
                .filter_map(|l| l.trim_start().split_once(' '))
                .map(|(_, path)| path.to_string())
                .collect();
            paths.sort();
            paths
        }

        /// Paths `git ls-files -v` marks `h` (assume-unchanged).
        fn assume_unchanged_paths(&self) -> Vec<String> {
            let text = git(&self.gw, &["ls-files", "-v"]);
            let mut paths: Vec<String> = text
                .lines()
                .filter_map(|l| l.strip_prefix("h "))
                .map(str::to_string)
                .collect();
            paths.sort();
            paths
        }
    }

    pub(crate) fn read_git_files(git_dir: &Path) -> GitFiles {
        let read = |rel: &str| std::fs::read(git_dir.join(rel)).ok();
        let mut files = GitFiles::without_contents(GitEntry::Directory);
        files.head = read("HEAD");
        files.config = read("config");
        files.packed_refs = read("packed-refs");
        files.index_present = git_dir.join("index").exists();
        files.markers = crate::deploy::git_state::GIT_MARKERS
            .iter()
            .filter(|m| git_dir.join(m).exists())
            .map(|m| (*m).to_string())
            .collect();
        fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, Vec<u8>)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let rel = format!("{rel}/{name}");
                if entry.path().is_dir() {
                    walk(&entry.path(), &rel, out);
                } else {
                    out.push((rel, std::fs::read(entry.path()).expect("read")));
                }
            }
        }
        walk(&git_dir.join("refs"), "refs", &mut files.loose_refs);
        files
            .loose_refs
            .retain(|(name, _)| !name.starts_with("refs/tags/"));
        files
    }

    async fn prepared(f: &Fixture, host: &HostGit, extra: &[&str]) -> AdvancePrep {
        let (state, refs) = f.gateway_state(host).await;
        assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
        let members = f.members(host, extra).await;
        prepare_advance(host, &f.seed, &state, &refs, &f.c2, &members)
            .await
            .expect("prepare")
    }

    fn kinds(plan: &AdvancePlan) -> Vec<&'static str> {
        plan.writes
            .iter()
            .map(|w| match w.name {
                GitWriteName::PackFile(_) => "pack",
                GitWriteName::PackIndex(_) => "idx",
                GitWriteName::BranchRef(_) => "branch",
                GitWriteName::RemoteRef { .. } => "remote",
                GitWriteName::DiscardRef(_) => "discarded",
                GitWriteName::Config => "config",
                GitWriteName::Head => "head",
                GitWriteName::Index => "index",
            })
            .collect()
    }

    #[tokio::test]
    async fn advance_applied_to_clone_is_clean() {
        let f = Fixture::new();
        let host = HostGit::new();
        let prep = prepared(&f, &host, &[LOCAL_ONLY]).await;
        assert_eq!(prep.pack_bases, vec![f.c1.clone()]);
        assert!(prep.pack.is_some());
        assert_eq!(prep.release_commit, f.c2);

        let export = f
            .import(&host, &[], &[(LOCAL_ONLY, "x = 1\n")], Some(UTIL_RES))
            .await;
        let plan = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect("plan");
        assert_eq!(plan.expected_drift, vec![LOCAL_ONLY, UTIL_RES]);
        assert_eq!(plan.dropped, Vec::<String>::new());
        assert_eq!(plan.assume_unchanged, vec!["README.md", "tags/tags.json"]);
        f.apply(&plan);

        assert_eq!(git(&f.gw, &["rev-parse", "HEAD"]), f.c2);
        assert_eq!(
            git(&f.gw, &["rev-parse", "refs/remotes/origin/production"]),
            f.c2
        );
        assert_eq!(f.status_paths(), plan.expected_drift);
        assert_eq!(f.assume_unchanged_paths(), plan.assume_unchanged);
        git(&f.gw, &["fsck", "--connectivity-only"]);
        // The advance leaves the branch's upstream config alone.
        assert_eq!(
            git(&f.gw, &["config", "branch.production.remote"]),
            "origin"
        );
    }

    #[tokio::test]
    async fn pack_bases_skip_unknown_head() {
        let f = Fixture::new();
        let host = HostGit::new();
        // An unpushed local commit D, unknown to the release checkout.
        put(
            &f.gw,
            "ignition/script-python/cap/util/unpushed.py",
            "u = 1\n",
        );
        git(&f.gw, &["add", "-A"]);
        git(&f.gw, &["commit", "-q", "-m", "unpushed"]);
        let d = git(&f.gw, &["rev-parse", "HEAD"]);
        assert!(!host.has_commit(&f.seed, &d).await.expect("lookup"));

        let (_, refs) = f.gateway_state(&host).await;
        assert_eq!(refs.head.commit(), Some(d.as_str()));
        let bases = pack_bases(&host, &f.seed, &refs).await.expect("bases");
        assert_eq!(bases, vec![f.c1.clone()]);

        let prep = prepared(&f, &host, &[]).await;
        assert!(prep.pack.is_some());
        let export = f.import(&host, &[], &[], None).await;
        let plan = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect("plan");
        f.apply(&plan);
        git(&f.gw, &["fsck", "--connectivity-only"]);
        git(&f.gw, &["cat-file", "-e", &d]);
        assert_eq!(git(&f.gw, &["rev-parse", "HEAD"]), f.c2);
    }

    #[tokio::test]
    async fn pack_bases_empty_full_pack() {
        let f = Fixture::new();
        let host = HostGit::new();
        let (state, _) = f.gateway_state(&host).await;
        let unknown = "d".repeat(40);
        let mut refs = GitRefs {
            head: GitHead::Branch {
                name: "production".to_string(),
                commit: Some(unknown.clone()),
            },
            heads: BTreeMap::new(),
            remotes: BTreeMap::new(),
        };
        refs.heads
            .insert("refs/heads/production".to_string(), unknown.clone());
        refs.remotes
            .insert("refs/remotes/origin/production".to_string(), "e".repeat(40));
        assert_eq!(
            pack_bases(&host, &f.seed, &refs).await.expect("bases"),
            Vec::<String>::new()
        );
        let members = f.members(&host, &[]).await;
        let prep = prepare_advance(&host, &f.seed, &state, &refs, &f.c2, &members)
            .await
            .expect("prepare");
        assert!(prep.pack_bases.is_empty());
        let pack = prep.pack.expect("a full pack");
        let count = u32::from_be_bytes([pack.pack[8], pack.pack[9], pack.pack[10], pack.pack[11]]);
        let objects = git(&f.seed, &["rev-list", "--objects", &f.c2])
            .lines()
            .count();
        assert_eq!(count as usize, objects);
    }

    #[tokio::test]
    async fn pack_none_when_up_to_date() {
        let f = Fixture::new();
        let host = HostGit::new();
        git(&f.gw, &["pull", "-q", "--ff-only"]);
        assert_eq!(git(&f.gw, &["rev-parse", "HEAD"]), f.c2);
        let prep = prepared(&f, &host, &[]).await;
        assert!(prep.pack.is_none());
        assert_eq!(prep.pack_bases, vec![f.c2.clone()]);
        let export = f.import(&host, &[], &[], None).await;
        let plan = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect("plan");
        assert_eq!(kinds(&plan), vec!["index", "remote", "branch", "head"]);
        f.apply(&plan);
        assert_eq!(f.status_paths(), plan.expected_drift);
    }

    fn prep_of(tree: &[&str], artifact: &[&str], blobs: &[(&str, &str)]) -> AdvancePrep {
        AdvancePrep {
            branch: "production".to_string(),
            remote: "origin".to_string(),
            release_commit: "a".repeat(40),
            pack: None,
            pack_bases: Vec::new(),
            tree_paths: tree.iter().map(|s| (*s).to_string()).collect(),
            artifact_members: artifact.iter().map(|s| (*s).to_string()).collect(),
            release_blobs: blobs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.as_bytes().to_vec()))
                .collect(),
            adopted: false,
            config: None,
            config_base: None,
        }
    }

    /// Each mode takes only its own kind of repo, and an adoption only names
    /// the repo's own remote.
    #[tokio::test]
    async fn prepare_with_refuses_the_wrong_mode() {
        use crate::deploy::git_state::GitRemote;
        let f = Fixture::new();
        let host = HostGit::new();
        let (managed, refs) = f.gateway_state(&host).await;
        assert_eq!(managed.kind, GitStateKind::Managed);
        let members = f.members(&host, &[]).await;
        let config = std::fs::read(f.git_dir().join("config")).expect("config");
        let adopt = |remote: &str| AdvanceMode::Adopt {
            branch: "production".to_string(),
            remote: remote.to_string(),
            config: config.clone(),
        };
        // An adoption of a managed repo.
        let err = prepare_advance_with(
            &host,
            &f.seed,
            &managed,
            &refs,
            &f.c2,
            &members,
            adopt("origin"),
        )
        .await
        .expect_err("a managed repo is not adopted");
        assert_eq!(err.code(), "invalid_input");

        // A fast-forward of an adoptable repo.
        let adoptable = GitState {
            kind: GitStateKind::Adoptable,
            reason: None,
            head: managed.head.clone(),
            remote: managed.remote.clone(),
            tracked: None,
            remote_refs: BTreeMap::new(),
        };
        let err = prepare_advance(&host, &f.seed, &adoptable, &refs, &f.c2, &members)
            .await
            .expect_err("an adoptable repo is not fast-forwarded");
        assert_eq!(err.code(), "invalid_input");

        // An adoption that names a remote the repo does not have.
        let err = prepare_advance_with(
            &host,
            &f.seed,
            &adoptable,
            &refs,
            &f.c2,
            &members,
            adopt("not-the-remote"),
        )
        .await
        .expect_err("another remote");
        assert_eq!(err.code(), "invalid_input");

        // A state with no remote has nothing to advance.
        let none = GitState {
            remote: None::<GitRemote>,
            ..adoptable
        };
        assert!(
            prepare_advance(&host, &f.seed, &none, &refs, &f.c2, &members)
                .await
                .is_err()
        );
    }

    #[test]
    fn predict_drift_classes() {
        let prep = prep_of(
            &["a", "b", "c"],
            &["a", "b", "d", "e", "f"],
            &[("a", "A"), ("b", "B")],
        );
        let export: BTreeMap<String, Vec<u8>> = [
            ("a", "A2"),
            ("b", "B"),
            ("d", "D"),
            ("f", "F"),
            ("z", "Z"),
            ("y", "Y"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.as_bytes().to_vec()))
        .collect();
        let got = predict_drift(&prep, &export);
        assert_eq!(got.modified, vec!["a"]);
        assert_eq!(got.untracked, vec!["d", "f"]);
        assert_eq!(got.unexpected, vec!["y", "z"]);
        assert_eq!(got.dropped, vec!["e"]);
    }

    #[test]
    fn predict_drift_reports_unlisted_in_scope_release_files_as_dropped() {
        // NOTES.txt sits in a resource folder whose resource.json does not list
        // it, so the artifact leaves it out and the gateway never holds it; it
        // is in scope and must be reported. README and deploy/ are out of scope.
        let prep = prep_of(
            &[
                "project.json",
                "ignition/x/code.py",
                "ignition/x/NOTES.txt",
                "deploy/project.yaml",
                "README.md",
            ],
            &["project.json", "ignition/x/code.py"],
            &[("project.json", "P"), ("ignition/x/code.py", "C")],
        );
        let export: BTreeMap<String, Vec<u8>> =
            [("project.json", "P"), ("ignition/x/code.py", "C")]
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.as_bytes().to_vec()))
                .collect();
        let got = predict_drift(&prep, &export);
        assert_eq!(got.dropped, vec!["ignition/x/NOTES.txt"]);
        assert!(got.modified.is_empty() && got.untracked.is_empty() && got.unexpected.is_empty());
    }

    #[tokio::test]
    async fn finish_dropped_member_is_assume_unchanged() {
        let f = Fixture::new();
        let host = HostGit::new();
        let prep = prepared(&f, &host, &[]).await;
        assert!(prep.artifact_members.contains(NOTES));
        // The gateway dropped NOTES.txt (a file its resource.json does not list).
        let export = f.import(&host, &[NOTES], &[], Some(UTIL_RES)).await;
        let plan = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect("a dropped member is not a mismatch");
        assert_eq!(plan.dropped, vec![NOTES]);
        assert!(plan.assume_unchanged.contains(&NOTES.to_string()));
        assert_eq!(plan.expected_drift, vec![UTIL_RES]);
        f.apply(&plan);
        assert_eq!(f.status_paths(), plan.expected_drift);
        assert!(f.assume_unchanged_paths().contains(&NOTES.to_string()));
        git(&f.gw, &["fsck", "--connectivity-only"]);
    }

    #[tokio::test]
    async fn finish_refuses_unexpected_member() {
        let f = Fixture::new();
        let host = HostGit::new();
        let prep = prepared(&f, &host, &[]).await;
        let extra = "ignition/script-python/cap/extra.txt";
        let export = f.import(&host, &[], &[(extra, "x\n")], None).await;
        let failure = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect_err("an export member outside the artifact");
        match failure {
            AdvanceFailure::Mismatch(m) => {
                assert_eq!(m.unexpected, vec![extra]);
                assert_eq!(m.count, 1);
            }
            other => panic!("not a mismatch: {other:?}"),
        }
    }

    #[tokio::test]
    async fn finish_refuses_status_mismatch() {
        let f = Fixture::new();
        let host = HostGit::new();
        let mut prep = prepared(&f, &host, &[]).await;
        let export = f.import(&host, &[], &[], Some(UTIL_RES)).await;
        // A prediction that wrongly believes the re-serialised file is the
        // release's own bytes: host status lists a modified path it lacks.
        let on_disk = std::fs::read(f.gw.join(UTIL_RES)).expect("read");
        prep.release_blobs.insert(UTIL_RES.to_string(), on_disk);
        let failure = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect_err("status differs from the prediction");
        match failure {
            AdvanceFailure::Mismatch(m) => assert_eq!(m.unexpected, vec![UTIL_RES]),
            other => panic!("not a mismatch: {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_order_fixed() {
        let f = Fixture::new();
        let host = HostGit::new();
        let prep = prepared(&f, &host, &[]).await;
        let export = f.import(&host, &[], &[], None).await;
        let plan = finish_advance(&host, &f.seed, &prep, &export)
            .await
            .expect("plan");
        assert_eq!(
            kinds(&plan),
            vec!["pack", "idx", "index", "remote", "branch", "head"]
        );
        let pack = prep.pack.as_ref().expect("pack");
        let rels: Vec<String> = plan.writes.iter().map(|w| w.name.rel()).collect();
        assert_eq!(
            rels,
            vec![
                format!("objects/pack/pack-{}.pack", pack.name),
                format!("objects/pack/pack-{}.idx", pack.name),
                "index".to_string(),
                "refs/remotes/origin/production".to_string(),
                "refs/heads/production".to_string(),
                "HEAD".to_string(),
            ]
        );
        assert_eq!(plan.writes[5].bytes, b"ref: refs/heads/production\n");
        assert_eq!(plan.writes[4].bytes, format!("{}\n", f.c2).into_bytes());
        for write in &plan.writes {
            write.name.check_shape().expect("fixed shape");
        }
        for bad in [
            GitWriteName::PackFile("zz".to_string()),
            GitWriteName::PackIndex("../x".to_string()),
            GitWriteName::BranchRef("../x".to_string()),
            GitWriteName::BranchRef("a..b".to_string()),
            GitWriteName::RemoteRef {
                remote: "a/b".to_string(),
                branch: "x".to_string(),
            },
            GitWriteName::RemoteRef {
                remote: "origin".to_string(),
                branch: ".hidden".to_string(),
            },
        ] {
            assert!(bad.check_shape().is_err(), "{bad:?}");
        }
        // The config shape exists for 27-17 and carries no parts to check.
        assert_eq!(GitWriteName::Config.rel(), "config");
    }

    #[test]
    fn keep_discarded_adds_the_ref_before_the_branch() {
        let head = "a".repeat(40);
        let write = |name: GitWriteName| GitWrite {
            name,
            bytes: Vec::new(),
        };
        let remote = || GitWriteName::RemoteRef {
            remote: "capapp".to_string(),
            branch: "production".to_string(),
        };
        let mut plan = AdvancePlan {
            branch: "production".to_string(),
            remote: "capapp".to_string(),
            release_commit: "b".repeat(40),
            writes: vec![
                write(GitWriteName::PackFile("c".repeat(40))),
                write(GitWriteName::Index),
                write(remote()),
                write(GitWriteName::BranchRef("production".to_string())),
                write(GitWriteName::Head),
            ],
            assume_unchanged: Vec::new(),
            dropped: Vec::new(),
            expected_drift: Vec::new(),
        };
        plan.keep_discarded("20261113T101500Z-ab12cd", &head)
            .expect("a safe run id and a full id");
        let names: Vec<String> = plan.writes.iter().map(|w| w.name.rel()).collect();
        assert_eq!(
            names,
            [
                format!("objects/pack/pack-{}.pack", "c".repeat(40)),
                "index".to_string(),
                "refs/remotes/capapp/production".to_string(),
                "refs/ign-discarded/20261113T101500Z-ab12cd".to_string(),
                "refs/heads/production".to_string(),
                "HEAD".to_string(),
            ]
        );
        assert_eq!(plan.writes[3].bytes, format!("{head}\n").into_bytes());
        // Unsafe run ids and a short head add nothing.
        for run in ["", "../x", ".hidden", "a b", "a..b", "x.lock"] {
            let before = plan.writes.len();
            assert!(plan.keep_discarded(run, &head).is_err(), "{run:?}");
            assert_eq!(plan.writes.len(), before);
        }
        assert!(plan.keep_discarded("run-1", "abc").is_err());

        // No branch ref in the plan: before HEAD, never after it; with no
        // HEAD either, at the end.
        for (given, want) in [
            (
                vec![GitWriteName::Index, GitWriteName::Head],
                vec!["index", "refs/ign-discarded/run-1", "HEAD"],
            ),
            (
                vec![GitWriteName::Index, remote()],
                vec![
                    "index",
                    "refs/remotes/capapp/production",
                    "refs/ign-discarded/run-1",
                ],
            ),
        ] {
            let mut bare = AdvancePlan {
                writes: given.into_iter().map(write).collect(),
                ..plan.clone()
            };
            bare.keep_discarded("run-1", &head).expect("added");
            let names: Vec<String> = bare.writes.iter().map(|w| w.name.rel()).collect();
            assert_eq!(names, want);
        }
    }
}
