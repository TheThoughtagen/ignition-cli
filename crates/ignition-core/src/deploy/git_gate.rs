//! The break/fix gate of a git-managed project deploy (Phase 27, 27-05).
//!
//! Four facts decide whether a deploy may overwrite what the gateway's git
//! module holds (D-01, D-02, D-04, D-11). They are computed here once, as
//! typed results, from ign's own release checkout with host git and the
//! operator's credentials:
//!
//! - **release checkout** ([`check_release_checkout`]): the release commit R
//!   is the commit the tag peels to; the checkout must sit at R, hold no
//!   modified tracked file and not be shallow, and one of its remotes must be
//!   the gateway's origin (matched by normalised URL, never by passing a
//!   URL to git);
//! - **remote view** ([`remote_view`]): ign asks the remote itself which
//!   branches exist and fetches their objects into its own store; the
//!   gateway's `refs/remotes/*` are never trusted for this;
//! - **ancestry** ([`gate_node`]): *unpushed* (the gateway HEAD G is on no
//!   remote branch), *not merged* (G is on the remote but not an ancestor of
//!   R), *release on branch* (R is an ancestor of the tracked branch's remote
//!   tip, a newer tip allowed);
//! - **dirty** ([`gate_node`], [`head_digest`], [`dirty_paths`]): members of
//!   the live export whose canonical digest differs from the tree at G.
//!
//! No result here is a [`CoreError`](crate::error::CoreError) slug: the
//! contract plan maps these typed results to refusals and envelope fields.
//! A git command in this module never writes a ref, `FETCH_HEAD` or config
//! value in ign's checkout; only its object store grows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::git_remote::{GitDir, HostGit, is_object_id};
use super::git_state::{GitState, url_key};
use super::project_artifact::{CanonicalDigest, build_artifact};
use crate::error::CoreError;

/// Most paths a `dirty` list holds; the full count travels beside it.
pub const LIST_CAP: usize = 200;

/// Most commit ids an `unpushed` or `not_merged` list names.
pub const ID_CAP: usize = 50;

/// Why a gate fact could not be established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitGateFailure {
    /// Host git failed or answered something unexpected (the detail is
    /// redacted).
    HostGit {
        /// What failed.
        detail: String,
    },
    /// The release checkout is not fit to deploy from.
    Checkout {
        /// Which condition failed, naming the tag, ids or paths.
        detail: String,
    },
    /// No remote of the checkout is the gateway's origin.
    RemoteMismatch {
        /// The gateway origin as the normalised key (`host/org/repo`, no
        /// scheme or userinfo).
        gateway_url: String,
    },
    /// The matched remote could not be read, or did not deliver the objects
    /// it listed.
    RemoteUnreachable {
        /// The remote's name in the checkout.
        remote: String,
        /// What failed (redacted).
        detail: String,
    },
    /// The gateway HEAD is not in ign's store and the gateway's own git
    /// objects could not supply it (27-10, D-24). Dirty is then neither
    /// clean nor unknown: the run refuses.
    GatewayObjects {
        /// What failed; ids are short and no content is ever named.
        detail: String,
    },
}

impl std::fmt::Display for GitGateFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitGateFailure::HostGit { detail } => write!(f, "host git failed: {detail}"),
            GitGateFailure::Checkout { detail } => write!(f, "release checkout: {detail}"),
            GitGateFailure::RemoteMismatch { gateway_url } => write!(
                f,
                "no remote of this checkout is the gateway's origin ({gateway_url})"
            ),
            GitGateFailure::RemoteUnreachable { remote, detail } => {
                write!(f, "remote `{remote}` could not be read: {detail}")
            }
            GitGateFailure::GatewayObjects { detail } => write!(
                f,
                "the gateway's git objects cannot supply its HEAD: {detail}"
            ),
        }
    }
}

/// The text of a host-git [`CoreError`] without the envelope wording.
pub(super) fn detail_of(err: &CoreError) -> String {
    match err {
        CoreError::DeployPreflightFailed { failures } => failures
            .iter()
            .map(|f| f.detail.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        other => other.to_string(),
    }
}

fn host(err: CoreError) -> GitGateFailure {
    GitGateFailure::HostGit {
        detail: detail_of(&err),
    }
}

pub(super) fn short(id: &str) -> &str {
    &id[..id.len().min(12)]
}

/// A release checkout that passed [`check_release_checkout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseCheckout {
    /// The checkout's path.
    pub path: PathBuf,
    /// R: the commit the release tag peels to, and `HEAD`.
    pub commit: String,
    /// The matched remote's name in the checkout.
    pub remote: String,
    /// The matched remote's normalised key (equal to the gateway's).
    pub url_key: String,
    /// The matched remote's URL without userinfo, for messages.
    pub url_redacted: String,
}

/// Check that `repo` is a release checkout of `tag` and find the remote that
/// is the gateway's origin.
///
/// Refuses, naming which: the tag does not exist; the checkout is shallow;
/// `HEAD` is not R; a tracked file is modified; no remote matches
/// `gateway_url_key`.
pub async fn check_release_checkout(
    git: &HostGit,
    repo: &Path,
    tag: &str,
    gateway_url_key: &str,
) -> Result<ReleaseCheckout, GitGateFailure> {
    let commit = git
        .release_commit(repo, tag)
        .await
        .map_err(host)?
        .ok_or_else(|| GitGateFailure::Checkout {
            detail: format!(
                "the release tag `{}` does not exist in this checkout",
                tag.escape_debug()
            ),
        })?;
    if git.is_shallow(repo).await.map_err(host)? {
        return Err(GitGateFailure::Checkout {
            detail: "this checkout is shallow, so the release cannot be compared with the \
                     gateway's HEAD; fetch the full history (`fetch-depth: 0` in a CI \
                     checkout)"
                .to_string(),
        });
    }
    let status = git.checkout_status(repo).await.map_err(host)?;
    if status.head != commit {
        return Err(GitGateFailure::Checkout {
            detail: format!(
                "HEAD is {} but the release tag `{}` is {}; check out the tag before deploying",
                short(&status.head),
                tag.escape_debug(),
                short(&commit)
            ),
        });
    }
    if !status.modified.is_empty() {
        let shown: Vec<String> = status
            .modified
            .iter()
            .take(5)
            .map(|p| p.escape_debug().to_string())
            .collect();
        let more = status.modified.len().saturating_sub(shown.len());
        return Err(GitGateFailure::Checkout {
            detail: format!(
                "tracked files are modified: {}{}",
                shown.join(", "),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            ),
        });
    }
    let remotes = git.remotes(repo).await.map_err(host)?;
    let matches: Vec<&(String, String)> = remotes
        .iter()
        .filter(|(_, url)| url_key(url) == gateway_url_key)
        .collect();
    let chosen = matches
        .iter()
        .find(|(name, _)| name == "origin")
        .or_else(|| matches.first())
        .ok_or_else(|| GitGateFailure::RemoteMismatch {
            gateway_url: gateway_url_key.to_string(),
        })?;
    Ok(ReleaseCheckout {
        path: repo.to_path_buf(),
        commit,
        remote: chosen.0.clone(),
        url_key: gateway_url_key.to_string(),
        url_redacted: super::git_state::redact_url(&chosen.1),
    })
}

/// The branches of the gateway's origin as the remote itself reports them,
/// with their objects present in ign's store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteView {
    /// The remote's name in the checkout.
    pub remote: String,
    /// Short branch name to tip id.
    pub heads: BTreeMap<String, String>,
}

/// Ask the remote for its heads (`ls-remote`) and bring every tip's objects
/// into ign's store (objects only). A tip missing after the fetch (the
/// branch moved, or the remote did not deliver it) fails closed.
pub async fn remote_view(
    git: &HostGit,
    checkout: &ReleaseCheckout,
) -> Result<RemoteView, GitGateFailure> {
    let unreachable = |err: CoreError| GitGateFailure::RemoteUnreachable {
        remote: checkout.remote.clone(),
        detail: format!(
            "cannot read {} ({}): {}",
            checkout.remote.escape_debug(),
            checkout.url_redacted,
            detail_of(&err)
        ),
    };
    let heads = git
        .ls_remote_heads(&checkout.path, &checkout.remote)
        .await
        .map_err(unreachable)?;
    let tips: Vec<String> = heads
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let present = git.has_commits(&checkout.path, &tips).await.map_err(host)?;
    let missing: Vec<String> = heads
        .iter()
        .filter(|(_, tip)| !present.contains(*tip))
        .map(|(branch, _)| branch.clone())
        .collect();
    if !missing.is_empty() {
        git.fetch_heads(&checkout.path, &checkout.remote, &missing)
            .await
            .map_err(unreachable)?;
        let after = git.has_commits(&checkout.path, &tips).await.map_err(host)?;
        for (branch, tip) in &heads {
            if !after.contains(tip) {
                return Err(GitGateFailure::RemoteUnreachable {
                    remote: checkout.remote.clone(),
                    detail: format!(
                        "branch `{}` listed tip {} but the fetch did not deliver it (the \
                         branch may have moved); run again",
                        branch.escape_debug(),
                        short(tip)
                    ),
                });
            }
        }
    }
    Ok(RemoteView {
        remote: checkout.remote.clone(),
        heads,
    })
}

/// Where a node's dirty list came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtySource {
    /// The tree at the gateway HEAD was in ign's own store (27-05).
    ReleaseStore,
    /// The tree was rebuilt from the gateway's own git objects, read over the
    /// host channel (27-10, D-24).
    GatewayObjects,
}

impl DirtySource {
    /// The word published for this source.
    pub fn as_str(self) -> &'static str {
        match self {
            DirtySource::ReleaseStore => "release_store",
            DirtySource::GatewayObjects => "gateway_objects",
        }
    }
}

/// The gate facts of one managed node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeGates {
    /// G, the gateway HEAD commit.
    pub head: Option<String>,
    /// Remote branches G is on (G an ancestor of the tip), name-sorted.
    pub on_branches: Vec<String>,
    /// G and the reflog ids newer than the last one the remote holds, newest
    /// first, capped at [`ID_CAP`]; empty when G is on a remote branch.
    pub unpushed: Vec<String>,
    /// `R..G`, newest first, capped at [`ID_CAP`], when G is on the remote
    /// and not an ancestor of R.
    pub not_merged: Vec<String>,
    /// The full size of `R..G`.
    pub not_merged_count: usize,
    /// R is an ancestor of the tracked branch's remote tip (a newer tip is
    /// allowed). False when the tracked branch is not on the remote.
    pub release_on_branch: bool,
    /// Members of the live export that differ from the tree at G, byte-sorted
    /// and capped at [`LIST_CAP`]. `None` (never an empty list) when G is not
    /// in ign's store: the paths then come from the gateway's own objects
    /// (27-10, `git_objects::dirty_from_gateway`) and `needs_gateway_objects`
    /// is set.
    pub dirty: Option<Vec<String>>,
    /// The full number of differing members (0 while `dirty` is `None`).
    pub dirty_count: usize,
    /// Where `dirty` came from; `None` while `dirty` is `None`.
    pub dirty_source: Option<DirtySource>,
    /// G is not in ign's store, so the dirty paths cannot be computed here.
    pub needs_gateway_objects: bool,
}

/// Compute the gate facts for one `Managed` node.
///
/// `head_log_tail` is the gateway's `logs/HEAD` tail (untrusted text) and
/// only names extra unpushed ids. `live` is the canonical digest of the live
/// export the prepare already computed.
pub async fn gate_node(
    git: &HostGit,
    checkout: &ReleaseCheckout,
    view: &RemoteView,
    state: &GitState,
    head_log_tail: Option<&[u8]>,
    live: &CanonicalDigest,
) -> Result<NodeGates, GitGateFailure> {
    let path = checkout.path.as_path();
    let g = state
        .head
        .as_ref()
        .and_then(|h| h.commit())
        .filter(|id| is_object_id(id))
        .ok_or_else(|| GitGateFailure::HostGit {
            detail: "the gateway repo has no HEAD commit to compare".to_string(),
        })?
        .to_string();
    let have_g = git.has_commit(path, &g).await.map_err(host)?;

    // Ancestry of one id against every remote tip, each tip asked once.
    let mut asked: BTreeMap<(String, String), bool> = BTreeMap::new();
    let mut ancestor = async |a: &str, b: &str| -> Result<bool, GitGateFailure> {
        let key = (a.to_string(), b.to_string());
        if let Some(known) = asked.get(&key) {
            return Ok(*known);
        }
        let answer = git.is_ancestor(path, a, b).await.map_err(host)?;
        asked.insert(key, answer);
        Ok(answer)
    };

    let mut on_branches = Vec::new();
    if have_g {
        for (branch, tip) in &view.heads {
            if ancestor(&g, tip).await? {
                on_branches.push(branch.clone());
            }
        }
    }

    let mut unpushed = Vec::new();
    if on_branches.is_empty() {
        unpushed.push(g.clone());
        let candidates = reflog_ids(head_log_tail);
        let present = git.has_commits(path, &candidates).await.map_err(host)?;
        for id in candidates.iter().rev() {
            if unpushed.len() >= ID_CAP {
                break;
            }
            if *id == g || unpushed.contains(id) {
                continue;
            }
            let mut held = false;
            if present.contains(id) {
                for tip in view.heads.values() {
                    if ancestor(id, tip).await? {
                        held = true;
                        break;
                    }
                }
            }
            if held {
                break;
            }
            unpushed.push(id.clone());
        }
    }

    let mut not_merged = Vec::new();
    let mut not_merged_count = 0;
    if !on_branches.is_empty() && !ancestor(&g, &checkout.commit).await? {
        not_merged = git
            .rev_list(path, &g, &checkout.commit, ID_CAP)
            .await
            .map_err(host)?;
        not_merged_count = git
            .rev_count(path, &g, &checkout.commit)
            .await
            .map_err(host)?;
    }

    let release_on_branch = match state.tracked.as_ref().and_then(|b| view.heads.get(b)) {
        Some(tip) => ancestor(&checkout.commit, tip).await?,
        None => false,
    };

    let (dirty, dirty_count) = if have_g {
        let head = head_digest(git, &GitDir::Checkout(checkout.path.clone()), &g).await?;
        let (list, count) = capped_dirty(live, &head);
        (Some(list), count)
    } else {
        (None, 0)
    };

    Ok(NodeGates {
        head: Some(g),
        on_branches,
        unpushed,
        not_merged,
        not_merged_count,
        release_on_branch,
        dirty_source: dirty.as_ref().map(|_| DirtySource::ReleaseStore),
        dirty,
        dirty_count,
        needs_gateway_objects: !have_g,
    })
}

/// The dirty paths between `live` and `head`, capped at [`LIST_CAP`], with
/// the full count.
pub(super) fn capped_dirty(live: &CanonicalDigest, head: &CanonicalDigest) -> (Vec<String>, usize) {
    let mut all = dirty_paths(live, head);
    let count = all.len();
    all.truncate(LIST_CAP);
    (all, count)
}

/// The ids a `logs/HEAD` tail names as new values, oldest first. A line the
/// tail cut in half, or any line that is not `<old> <new> ...`, is skipped.
fn reflog_ids(tail: Option<&[u8]>) -> Vec<String> {
    let Some(tail) = tail else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for line in tail.split(|b| *b == b'\n') {
        let text = String::from_utf8_lossy(line);
        let mut fields = text.split(' ');
        let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
            continue;
        };
        if is_object_id(old) && is_object_id(new) && new.bytes().any(|b| b != b'0') {
            ids.push(new.to_string());
        }
    }
    ids
}

/// The canonical digest of the project tree at `commit`: the tree is
/// materialised into a private temp dir from `repo`'s objects and run through
/// [`build_artifact`], so the same root allowlist, denylist and `.ignexclude`
/// rules apply as in the deploy itself.
///
/// A tree that is not a deployable project (no `project.json`, a symlink) is
/// reported as a host-git failure naming the commit.
pub async fn head_digest(
    git: &HostGit,
    repo: &GitDir,
    commit: &str,
) -> Result<CanonicalDigest, GitGateFailure> {
    let dir = tempfile::tempdir().map_err(|e| GitGateFailure::HostGit {
        detail: format!("a private temp dir could not be made: {e}"),
    })?;
    let tree = dir.path().join("tree");
    git.materialize(repo, commit, &tree).await.map_err(host)?;
    let artifact = build_artifact(&tree).map_err(|e| GitGateFailure::HostGit {
        detail: format!(
            "the tree at {} is not a deployable project: {}",
            short(commit),
            e
        ),
    })?;
    Ok(artifact.digest)
}

/// Every member key whose canonical digest differs between `live` and `head`
/// or that exists on one side only, byte-sorted.
pub fn dirty_paths(live: &CanonicalDigest, head: &CanonicalDigest) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for (key, sha) in &live.members {
        if head.members.get(key) != Some(sha) {
            paths.insert(key.clone());
        }
    }
    for key in head.members.keys() {
        if !live.members.contains_key(key) {
            paths.insert(key.clone());
        }
    }
    paths.into_iter().collect()
}
