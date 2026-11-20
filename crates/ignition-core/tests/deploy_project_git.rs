//! 27-02: keep and advance a module-style `.git` across a simulated module
//! import on a local node, through the real transport seam and host git.
//!
//! Every test builds a bare remote, a release checkout and a gateway clone
//! with host git (`common::git_fixture`), runs `prepare_advance`, `save`, the
//! import simulation and `restore_and_advance`, and asks host git what the
//! gateway repo looks like afterwards.

mod common;

use common::git_fixture::{
    BRANCH, CODE, EXTRA, GATEWAY, GitFixture, LOCAL_ONLY, LOCAL_ONLY_RES, NOTES, PROJECT, REMOTE,
    UNPUSHED, UTIL_RES, gateway_form, git, in_scope, live_export_from, put,
};
use ignition_core::deploy::git_advance::{GitWrite, GitWriteName};
use ignition_core::deploy::git_gate::{
    GitGateFailure, LIST_CAP, NodeGates, ReleaseCheckout, RemoteView, check_release_checkout,
    dirty_paths, gate_node, head_digest, remote_view,
};
use ignition_core::deploy::git_handover::{
    GitRestoreFailure, GitRestoreReport, RESTORE_REASONS, restore_and_advance, save,
};
use ignition_core::deploy::git_remote::{GitDir, HostGit};
use ignition_core::deploy::git_state::GitState;
#[cfg(unix)]
use ignition_core::deploy::git_state::url_key;
use ignition_core::deploy::ledger::LedgerRoot;
use ignition_core::deploy::project_artifact::{CanonicalDigest, canonical_digest};
use ignition_core::deploy::transport::projects::{GitHostRead, GitStateWriter};

const RUN: &str = "20261113T101500Z-ab12";

/// Prepare, save, import, hand over: the whole path with the given import.
async fn handover(
    f: &GitFixture,
    keep_git: bool,
    gateway_form: &[&str],
    extra: &[&str],
    drop: &[&str],
) -> Result<GitRestoreReport, GitRestoreFailure> {
    let host = HostGit::new();
    let node = f.node();
    let (state, prep) = f.prepare(&host).await;
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    f.module_import(keep_git, gateway_form, extra, drop);
    restore_and_advance(
        &node,
        &node,
        &host,
        &f.checkout,
        &state,
        &saved,
        &prep,
        &f.export_zip(),
        None,
    )
    .await
}

#[tokio::test]
async fn handover_managed_fast_forward() {
    let f = GitFixture::new();
    let report = handover(&f, false, &[UTIL_RES], &[], &[])
        .await
        .expect("handover");

    assert!(
        report.restored && report.advanced && report.verified,
        "{report:?}"
    );
    assert_eq!(report.expected_drift, vec![UTIL_RES]);
    assert_eq!(report.dropped, Vec::<String>::new());
    assert_eq!(f.git_in_project(&["rev-parse", "HEAD"]), f.c2);
    assert_eq!(
        f.git_in_project(&["rev-parse", &format!("refs/heads/{BRANCH}")]),
        f.c2
    );
    assert_eq!(
        f.git_in_project(&["rev-parse", &format!("refs/remotes/{REMOTE}/{BRANCH}")]),
        f.c2
    );
    assert_eq!(
        f.git_in_project(&["symbolic-ref", "HEAD"]),
        format!("refs/heads/{BRANCH}")
    );
    assert_eq!(f.status_paths(), report.expected_drift);
    f.git_in_project(&["fsck", "--connectivity-only"]);
    // The module's own settings are untouched.
    assert_eq!(
        f.git_in_project(&["config", &format!("branch.{BRANCH}.remote")]),
        REMOTE
    );
    assert_eq!(f.git_in_project(&["config", "http.sslVerify"]), "false");
    // The save is kept; the caller removes it once everything held.
    assert!(f.data_dir.join(&report.save_path).is_dir());
}

#[tokio::test]
async fn handover_assume_unchanged_only_absent() {
    let f = GitFixture::new();
    handover(&f, false, &[UTIL_RES], &[], &[])
        .await
        .expect("handover");

    let tree: Vec<String> = f
        .git_in_project(&["ls-tree", "-r", "--name-only", &f.c2])
        .lines()
        .map(str::to_string)
        .collect();
    let absent: Vec<String> = tree
        .iter()
        .filter(|p| !f.project_dir().join(p).exists())
        .cloned()
        .collect();
    assert_eq!(
        absent,
        vec!["README.md", "images/logo.txt", "tags/tags.json"],
        "the import removes exactly the out-of-scope paths"
    );
    assert_eq!(f.assume_unchanged_paths(), absent);
    // Every path on disk is a plain cached entry (D-20).
    let mut present: Vec<String> = tree
        .iter()
        .filter(|p| f.project_dir().join(p).exists())
        .cloned()
        .collect();
    present.sort();
    assert_eq!(f.tracked_paths(), present);
    assert!(present.iter().all(|p| in_scope(p)), "{present:?}");
}

#[tokio::test]
async fn handover_present_after_import_advances_in_place() {
    let f = GitFixture::new();
    let report = handover(&f, true, &[UTIL_RES], &[], &[])
        .await
        .expect("handover");

    assert!(!report.restored, "{report:?}");
    assert!(report.advanced && report.verified);
    assert_eq!(f.git_in_project(&["rev-parse", "HEAD"]), f.c2);
    assert_eq!(f.status_paths(), vec![UTIL_RES]);
    f.git_in_project(&["fsck", "--connectivity-only"]);
}

#[tokio::test]
async fn handover_unpushed_head_advances() {
    let f = GitFixture::new();
    let d = f.designer_commit(false);
    // The release checkout has never seen D.
    assert!(
        !HostGit::new()
            .has_commit(&f.checkout, &d)
            .await
            .expect("lookup")
    );

    let report = handover(&f, false, &[UTIL_RES], &[], &[])
        .await
        .expect("handover");
    assert!(
        report.restored && report.advanced && report.verified,
        "{report:?}"
    );
    assert_eq!(f.git_in_project(&["rev-parse", "HEAD"]), f.c2);
    f.git_in_project(&["fsck", "--connectivity-only"]);
    f.git_in_project(&["cat-file", "-e", &d]);
    // D's file is not in C2, so it is no part of the drift either.
    assert!(!f.project_dir().join(UNPUSHED).exists());
    assert_eq!(f.status_paths(), vec![UTIL_RES]);
}

#[tokio::test]
async fn handover_export_only_member_predicted() {
    let f = GitFixture::new();
    f.untracked_in_checkout();
    let report = handover(&f, false, &[UTIL_RES], &[], &[])
        .await
        .expect("handover");

    let expected = vec![LOCAL_ONLY, LOCAL_ONLY_RES, UTIL_RES];
    assert_eq!(report.expected_drift, expected);
    assert_eq!(f.status_paths(), expected);
    let porcelain = f.git_in_project(&["status", "--porcelain", "--untracked-files=all"]);
    assert!(
        porcelain.lines().any(|l| l == format!("?? {LOCAL_ONLY}")),
        "{porcelain}"
    );
    assert_eq!(
        porcelain.lines().count(),
        3,
        "nothing beyond the untracked folder and the pretty-printed file: {porcelain}"
    );
    f.git_in_project(&["fsck", "--connectivity-only"]);
}

#[tokio::test]
async fn handover_extra_member_fails() {
    let f = GitFixture::new();
    let host = HostGit::new();
    let node = f.node();
    let (state, prep) = f.prepare(&host).await;
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    f.module_import(false, &[UTIL_RES], &[EXTRA], &[]);
    let failure = restore_and_advance(
        &node,
        &node,
        &host,
        &f.checkout,
        &state,
        &saved,
        &prep,
        &f.export_zip(),
        None,
    )
    .await
    .expect_err("an export member the artifact lacks");

    assert_eq!(failure.reason, "status_unexpected", "{failure:?}");
    assert!(RESTORE_REASONS.contains(&failure.reason));
    assert_eq!(failure.paths, vec![EXTRA]);
    assert_eq!(failure.save_path, saved.path);
    // Nothing restored, nothing written; the save is intact.
    assert!(!f.git_dir().exists());
    assert_eq!(
        node.save_manifest(GATEWAY, PROJECT, RUN)
            .await
            .expect("save manifest"),
        saved.manifest
    );
}

#[tokio::test]
async fn handover_dropped_in_scope_member() {
    let f = GitFixture::new();
    let report = handover(&f, false, &[UTIL_RES], &[], &[NOTES])
        .await
        .expect("a dropped member never refuses the handover");

    assert!(report.verified, "{report:?}");
    assert_eq!(report.dropped, vec![NOTES]);
    assert_eq!(report.expected_drift, vec![UTIL_RES]);
    assert!(!f.project_dir().join(NOTES).exists());
    assert!(f.assume_unchanged_paths().contains(&NOTES.to_string()));
    // Neither deleted nor modified: git status lists exactly the drift.
    assert_eq!(f.status_paths(), report.expected_drift);
    f.git_in_project(&["fsck", "--connectivity-only"]);
}

/// A git writer that applies the first `applied` writes of an advance, in
/// order, and then fails: the lock that appears between two renames, or a
/// full disk, in the middle of the advance.
struct FailsAfter {
    inner: ignition_core::deploy::transport::local::LocalFsTransport,
    applied: usize,
}

#[async_trait::async_trait]
impl GitStateWriter for FailsAfter {
    async fn save_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<
        ignition_core::deploy::transport::projects::GitManifest,
        ignition_core::error::CoreError,
    > {
        self.inner.save_git(gateway, project, run_id).await
    }
    async fn restore_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<
        ignition_core::deploy::transport::projects::GitManifest,
        ignition_core::error::CoreError,
    > {
        self.inner.restore_git(gateway, project, run_id).await
    }
    async fn write_git_files(
        &self,
        project: &str,
        writes: &[GitWrite],
    ) -> Result<(), ignition_core::error::CoreError> {
        let first = &writes[..self.applied.min(writes.len())];
        self.inner.write_git_files(project, first).await?;
        Err(ignition_core::error::CoreError::TransportFailed {
            operation: "write".to_string(),
            node: "nodes.master".to_string(),
            detail: "the write was refused after the first files".to_string(),
        })
    }
    async fn remove_git_save(
        &self,
        g: &str,
        p: &str,
        r: &str,
    ) -> Result<(), ignition_core::error::CoreError> {
        self.inner.remove_git_save(g, p, r).await
    }
    async fn probe_write(
        &self,
        project: &str,
        run_id: &str,
    ) -> Result<(), ignition_core::error::CoreError> {
        self.inner.probe_write(project, run_id).await
    }
}

/// WR-A1 (27-26, D-06, D-10): the advance writes the index before every ref,
/// config and HEAD. A write that fails right after pack, pack index and index
/// leaves HEAD and the branch at the old commit with an index that holds the
/// release: the release shows as staged changes on the old commit, never the
/// deploy reversed.
#[tokio::test]
async fn handover_write_fails_after_index_leaves_staged_changes() {
    let f = GitFixture::new();
    let host = HostGit::new();
    let node = f.node();
    let (state, prep) = f.prepare(&host).await;
    assert!(prep.pack.is_some(), "the release commit needs a pack");
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    f.module_import(false, &[UTIL_RES], &[], &[]);
    // Pack, pack index, index: the first three writes of the advance.
    let writer = FailsAfter {
        inner: f.node(),
        applied: 3,
    };
    let failure = restore_and_advance(
        &node,
        &writer,
        &host,
        &f.checkout,
        &state,
        &saved,
        &prep,
        &f.export_zip(),
        None,
    )
    .await
    .expect_err("the write fails after the index");
    assert_eq!(failure.reason, "advance_failed", "{failure:?}");
    assert!(RESTORE_REASONS.contains(&failure.reason));
    assert_eq!(failure.save_path, saved.path);
    assert!(f.data_dir.join(&failure.save_path).is_dir(), "save kept");

    // HEAD and the branch still name the old commit.
    assert_eq!(f.git_in_project(&["rev-parse", "HEAD"]), f.c1);
    assert_eq!(
        f.git_in_project(&["rev-parse", &format!("refs/heads/{BRANCH}")]),
        f.c1
    );
    assert_eq!(
        f.git_in_project(&["rev-parse", &format!("refs/remotes/{REMOTE}/{BRANCH}")]),
        f.c1
    );

    // The index describes the release: old commit to index is the release's
    // changes as staged work, not their reverse. Asked on a copy of the repo.
    let copy = tempfile::tempdir().expect("tempdir");
    let copied = copy.path().join("repo");
    common::git_fixture::copy_tree(&f.project_dir(), &copied);
    let out = std::process::Command::new("git")
        .args(["write-tree"])
        .current_dir(&copied)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let index_tree = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        index_tree,
        git(&copied, &["rev-parse", &format!("{}^{{tree}}", f.c2)]),
        "the index must hold the release tree"
    );
}

#[tokio::test]
async fn restore_never_overwrites() {
    let f = GitFixture::new();
    let node = f.node();
    save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    let before = node.git_manifest(PROJECT).await.expect("manifest");
    let head = std::fs::read(f.git_dir().join("HEAD")).expect("HEAD");

    let err = node
        .restore_git(GATEWAY, PROJECT, RUN)
        .await
        .expect_err("a .git exists");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(err.to_string().contains(".git"), "{err}");
    assert_eq!(node.git_manifest(PROJECT).await.expect("manifest"), before);
    assert_eq!(std::fs::read(f.git_dir().join("HEAD")).expect("HEAD"), head);
}

/// K11 (27-13): an import that removed `.git` is followed by the saved repo
/// exactly as it was, and never by an advance.
#[tokio::test]
async fn restore_unchanged_puts_the_saved_repo_back() {
    use ignition_core::deploy::git_handover::restore_unchanged;
    let f = GitFixture::new();
    let node = f.node();
    let before = node.git_manifest(PROJECT).await.expect("manifest");
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    f.module_import(false, &[], &[], &[]);
    assert!(!f.git_dir().exists(), "the import removed .git");

    let report = restore_unchanged(&node, &node, GATEWAY, PROJECT, &saved)
        .await
        .expect("restored");
    assert!(report.restored && !report.advanced);
    assert!(report.expected_drift.is_empty() && report.dropped.is_empty());
    assert_eq!(node.git_manifest(PROJECT).await.expect("manifest"), before);
    assert_eq!(
        f.git_in_project(&["rev-parse", "HEAD"]),
        f.c1,
        "not advanced"
    );
}

/// A `.git` that survived the import is left exactly as it is: nothing is
/// written, and nothing advances.
#[tokio::test]
async fn restore_unchanged_leaves_a_present_repo() {
    use ignition_core::deploy::git_handover::restore_unchanged;
    let f = GitFixture::new();
    let node = f.node();
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    f.module_import(true, &[], &[], &[]);
    let before = node.git_manifest(PROJECT).await.expect("manifest");

    let report = restore_unchanged(&node, &node, GATEWAY, PROJECT, &saved)
        .await
        .expect("left as it is");
    assert!(!report.restored && !report.advanced);
    assert_eq!(node.git_manifest(PROJECT).await.expect("manifest"), before);
    assert_eq!(f.git_in_project(&["rev-parse", "HEAD"]), f.c1);
}

/// A save that no longer matches what was saved is never restored.
#[tokio::test]
async fn restore_unchanged_refuses_a_changed_save() {
    use ignition_core::deploy::git_handover::restore_unchanged;
    let f = GitFixture::new();
    let node = f.node();
    let mut saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");
    f.module_import(false, &[], &[], &[]);
    saved.manifest.files += 1;

    let failure = restore_unchanged(&node, &node, GATEWAY, PROJECT, &saved)
        .await
        .expect_err("the save does not match");
    assert_eq!(failure.reason, "restore_failed");
    assert!(!failure.restored);
    assert!(!f.git_dir().exists(), "nothing was written");
    assert!(RESTORE_REASONS.contains(&failure.reason));
}

#[cfg(unix)]
#[tokio::test]
async fn save_refuses_symlink() {
    let f = GitFixture::new();
    let node = f.node();
    std::os::unix::fs::symlink("/nonexistent/elsewhere", f.git_dir().join("evil"))
        .expect("symlink");

    let err = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect_err("a symlink inside .git");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(err.to_string().contains("symlink"), "{err}");
    let run_dir = f.data_dir.join(format!(
        "ign-deploy/{GATEWAY}/projects/{PROJECT}/git-save/{RUN}"
    ));
    assert!(!run_dir.exists(), "no save directory is left behind");
    // The source was not touched.
    assert!(f.git_dir().join("HEAD").is_file());
}

#[tokio::test]
async fn write_refuses_existing_lock() {
    let f = GitFixture::new();
    let node = f.node();
    let branch_ref = f.git_dir().join(format!("refs/heads/{BRANCH}"));
    let remote_ref = f.git_dir().join(format!("refs/remotes/{REMOTE}/{BRANCH}"));
    std::fs::write(f.git_dir().join(format!("refs/heads/{BRANCH}.lock")), b"").expect("lock");
    // A fresh clone keeps its remote-tracking ref in packed-refs, not loose.
    let (old_branch, old_remote) = (
        std::fs::read(&branch_ref).expect("branch"),
        std::fs::read(&remote_ref).ok(),
    );
    let new = format!("{}\n", f.c2).into_bytes();
    // The remote-tracking write comes first and would succeed alone.
    let writes = [
        GitWrite {
            old: Default::default(),
            packed_refs_old: None,
            name: GitWriteName::RemoteRef {
                remote: REMOTE.to_string(),
                branch: BRANCH.to_string(),
            },
            bytes: new.clone(),
        },
        GitWrite {
            old: Default::default(),
            packed_refs_old: None,
            name: GitWriteName::BranchRef(BRANCH.to_string()),
            bytes: new,
        },
    ];
    let err = node
        .write_git_files(PROJECT, &writes)
        .await
        .expect_err("a lock is held");
    assert_eq!(err.code(), "transport_failed", "{err}");
    assert!(err.to_string().contains(".lock"), "{err}");
    assert_eq!(std::fs::read(&branch_ref).expect("branch"), old_branch);
    assert_eq!(std::fs::read(&remote_ref).ok(), old_remote);
    assert!(
        !f.git_dir()
            .join(format!("refs/remotes/{REMOTE}/{BRANCH}.lock"))
            .exists(),
        "no stray lock"
    );
}

#[tokio::test]
async fn write_refuses_unsafe_names() {
    let f = GitFixture::new();
    let node = f.node();
    for bad in [
        GitWriteName::BranchRef("../../x".to_string()),
        GitWriteName::PackFile("not-hex".to_string()),
        GitWriteName::RemoteRef {
            remote: "a/b".to_string(),
            branch: "x".to_string(),
        },
    ] {
        let err = node
            .write_git_files(
                PROJECT,
                &[GitWrite {
                    old: Default::default(),
                    packed_refs_old: None,
                    name: bad.clone(),
                    bytes: b"x".to_vec(),
                }],
            )
            .await
            .expect_err("an unsafe name");
        assert_eq!(err.code(), "transport_failed", "{bad:?}: {err}");
    }
}

#[tokio::test]
async fn save_dir_is_not_dot_git() {
    let f = GitFixture::new();
    let node = f.node();
    let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
        .await
        .expect("save");

    assert!(
        saved.path.ends_with(&format!("git-save/{RUN}/repo")),
        "{}",
        saved.path
    );
    assert!(saved.path.split('/').all(|part| part != ".git"));
    assert!(f.data_dir.join(&saved.path).join("HEAD").is_file());
    // Removing the save leaves the live `.git` alone.
    node.remove_git_save(GATEWAY, PROJECT, RUN)
        .await
        .expect("remove");
    assert!(!f.data_dir.join(&saved.path).exists());
    assert!(f.git_dir().join("HEAD").is_file());
    // Absent is a success.
    node.remove_git_save(GATEWAY, PROJECT, RUN)
        .await
        .expect("again");
}

/// The fixture's in-scope paths are the ones the artifact carries, so the
/// drop and extra cases above act on real members.
#[test]
fn fixture_members_are_real() {
    let f = GitFixture::new();
    let members = f.artifact().digest.members;
    for member in [CODE, UTIL_RES, NOTES] {
        assert!(members.contains_key(member), "{member}");
    }
    assert!(!members.contains_key(EXTRA));
    assert!(members.keys().all(|m| in_scope(m)));
}

// ---------------------------------------------------------------------------
// 27-05: the C1 break/fix gate and the D-11 reachability check, host git only.
// ---------------------------------------------------------------------------

const RELEASE: &str = "v1.1.0";

/// Everything a gate test needs once the checkout has passed.
struct Gate {
    host: HostGit,
    state: GitState,
    checkout: ReleaseCheckout,
    view: RemoteView,
}

impl Gate {
    /// The gateway's origin key, as the classified state carries it.
    fn key(state: &GitState) -> String {
        state
            .remote
            .as_ref()
            .expect("managed remote")
            .url_key
            .clone()
    }

    async fn new(f: &GitFixture, tag: &str) -> Gate {
        let host = HostGit::new();
        let (state, _) = f.gateway_state(&host).await;
        let checkout = check_release_checkout(&host, &f.checkout, tag, &Gate::key(&state))
            .await
            .expect("release checkout");
        let view = remote_view(&host, &checkout).await.expect("remote view");
        Gate {
            host,
            state,
            checkout,
            view,
        }
    }

    async fn nodes(&self, f: &GitFixture, live: &CanonicalDigest) -> NodeGates {
        let tail = f.head_log_tail().await;
        gate_node(
            &self.host,
            &self.checkout,
            &self.view,
            &self.state,
            tail.as_deref(),
            live,
        )
        .await
        .expect("gate_node")
    }
}

/// The live export of the gateway project directory as it is now.
fn live_now(f: &GitFixture) -> CanonicalDigest {
    canonical_digest(&live_export_from(&f.project_dir())).expect("live digest")
}

fn checkout_detail(err: GitGateFailure) -> String {
    match err {
        GitGateFailure::Checkout { detail } => detail,
        other => panic!("not a checkout failure: {other:?}"),
    }
}

#[tokio::test]
async fn checkout_at_tag_ok() {
    let f = GitFixture::new();
    let host = HostGit::new();
    let (state, _) = f.gateway_state(&host).await;
    let rc = check_release_checkout(&host, &f.checkout, RELEASE, &Gate::key(&state))
        .await
        .expect("fit checkout");
    assert_eq!(rc.commit, f.c2);
    assert_eq!(rc.remote, "origin");
    assert_eq!(rc.url_key, Gate::key(&state));
}

#[tokio::test]
async fn checkout_not_at_tag() {
    let f = GitFixture::new();
    git(&f.checkout, &["checkout", "-q", "--detach", &f.c1]);
    let host = HostGit::new();
    let (state, _) = f.gateway_state(&host).await;
    let err = check_release_checkout(&host, &f.checkout, RELEASE, &Gate::key(&state))
        .await
        .expect_err("HEAD is not the tag");
    let detail = checkout_detail(err);
    assert!(detail.contains(RELEASE), "{detail}");
    assert!(detail.contains(&f.c1[..12]), "{detail}");
    assert!(detail.contains(&f.c2[..12]), "{detail}");
}

#[tokio::test]
async fn checkout_unknown_tag() {
    let f = GitFixture::new();
    let host = HostGit::new();
    let (state, _) = f.gateway_state(&host).await;
    let err = check_release_checkout(&host, &f.checkout, "v9.9.9", &Gate::key(&state))
        .await
        .expect_err("no such tag");
    assert!(checkout_detail(err).contains("v9.9.9"));
}

#[tokio::test]
async fn checkout_modified() {
    let f = GitFixture::new();
    put(&f.checkout, CODE, "def f():\n    return 42\n");
    let host = HostGit::new();
    let (state, _) = f.gateway_state(&host).await;
    let err = check_release_checkout(&host, &f.checkout, RELEASE, &Gate::key(&state))
        .await
        .expect_err("a tracked file is modified");
    assert!(checkout_detail(err).contains(CODE));
}

#[tokio::test]
async fn checkout_shallow() {
    let f = GitFixture::new();
    let shallow = f.shallow_checkout();
    let host = HostGit::new();
    let err = check_release_checkout(&host, &shallow, RELEASE, "example.invalid/org/repo")
        .await
        .expect_err("shallow");
    assert!(checkout_detail(err).contains("fetch-depth: 0"));
}

#[tokio::test]
async fn remote_mismatch() {
    let f = GitFixture::new();
    let host = HostGit::new();
    let err = check_release_checkout(&host, &f.checkout, RELEASE, "example.invalid/org/other")
        .await
        .expect_err("no remote matches");
    assert_eq!(
        err,
        GitGateFailure::RemoteMismatch {
            gateway_url: "example.invalid/org/other".to_string()
        }
    );
}

#[cfg(unix)]
#[tokio::test]
async fn remote_unreachable_redacted() {
    let f = GitFixture::new();
    let url = "https://user:secret@127.0.0.1:9/x.git";
    assert_eq!(f.checkout_with_userinfo_remote(url), "origin");

    // A recorder in front of git: nothing of the URL may reach an argument.
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("argv.log");
    let recorder = dir.path().join("git-recorder");
    write_exec(
        &recorder,
        &format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\nexec git \"$@\"\n",
            log.display()
        ),
    );
    let host = HostGit::with_program(&recorder);

    let checkout = check_release_checkout(&host, &f.checkout, RELEASE, &url_key(url))
        .await
        .expect("the remote matches by key");
    assert_eq!(checkout.url_redacted, "https://127.0.0.1:9/x.git");
    let err = remote_view(&host, &checkout)
        .await
        .expect_err("nothing listens on port 9");
    match &err {
        GitGateFailure::RemoteUnreachable { remote, detail } => {
            assert_eq!(remote, "origin");
            assert!(detail.contains("https://127.0.0.1:9/x.git"), "{detail}");
            assert!(!detail.contains("secret"), "{detail}");
        }
        other => panic!("not unreachable: {other:?}"),
    }
    assert!(!err.to_string().contains("secret"));
    let argv = std::fs::read_to_string(&log).expect("argv log");
    assert!(!argv.contains("secret"), "{argv}");
    assert!(!argv.contains("127.0.0.1"), "{argv}");
    assert!(argv.contains("ls-remote"));
}

#[tokio::test]
async fn remote_gates_clean() {
    let f = GitFixture::new();
    let gate = Gate::new(&f, RELEASE).await;
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(nodes.head.as_deref(), Some(f.c1.as_str()));
    assert_eq!(nodes.on_branches, vec![BRANCH.to_string()]);
    assert_eq!(nodes.unpushed, Vec::<String>::new());
    assert_eq!(nodes.not_merged, Vec::<String>::new());
    assert_eq!(nodes.not_merged_count, 0);
    assert!(nodes.release_on_branch);
    assert!(!nodes.needs_gateway_objects);
    assert_eq!(gate.view.heads[BRANCH], f.c2);
    assert!(gate.view.heads.contains_key("main"));
}

#[tokio::test]
async fn remote_gates_unpushed() {
    let f = GitFixture::new();
    let d = f.designer_commit(false);
    let gate = Gate::new(&f, RELEASE).await;
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(nodes.head.as_deref(), Some(d.as_str()));
    assert_eq!(nodes.unpushed, vec![d]);
    assert!(nodes.on_branches.is_empty());
    assert_eq!(nodes.not_merged, Vec::<String>::new());
    assert!(nodes.needs_gateway_objects);
}

#[tokio::test]
async fn remote_gates_unpushed_names_earlier_ids() {
    let f = GitFixture::new();
    let d1 = f.designer_commit(false);
    let d2 = f.designer_commit(false);
    let gate = Gate::new(&f, RELEASE).await;
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    // Newest first: G, then the earlier unpushed id from `logs/HEAD`; the
    // clone's own start (on the remote) ends the list.
    assert_eq!(nodes.unpushed, vec![d2, d1]);
}

#[tokio::test]
async fn remote_gates_not_merged() {
    let f = GitFixture::new();
    let b = f.breakfix_on_production();
    let gate = Gate::new(&f, RELEASE).await;
    assert_eq!(gate.view.heads[BRANCH], b);
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(nodes.head.as_deref(), Some(b.as_str()));
    assert_eq!(nodes.on_branches, vec![BRANCH.to_string()]);
    assert_eq!(nodes.unpushed, Vec::<String>::new());
    assert_eq!(nodes.not_merged, vec![b]);
    assert_eq!(nodes.not_merged_count, 1);
    // The release is still an ancestor of the (newer) tracked tip.
    assert!(nodes.release_on_branch);
    // The fetch brought B into ign's store, so dirty is computable.
    assert!(!nodes.needs_gateway_objects);
    assert_eq!(nodes.dirty, Some(Vec::new()));
}

#[tokio::test]
async fn release_reachable_newer_tip() {
    let f = GitFixture::new();
    let c3 = f.push_to_production("ignition/script-python/cap/util2/more.py", "m = 1\n");
    let gate = Gate::new(&f, RELEASE).await;
    assert_eq!(gate.view.heads[BRANCH], c3);
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert!(nodes.release_on_branch);
}

#[tokio::test]
async fn release_not_on_tracked_branch() {
    let f = GitFixture::new();
    let r = f.tag_release_on("main", "v2.0.0");
    let gate = Gate::new(&f, "v2.0.0").await;
    assert_eq!(gate.checkout.commit, r);
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert!(!nodes.release_on_branch);
    // G (the gateway's clone of C1) is on the remote but not in the release.
    assert_eq!(nodes.on_branches, vec![BRANCH.to_string()]);
    assert_eq!(nodes.not_merged, vec![f.c1.clone()]);
    assert_eq!(nodes.not_merged_count, 1);
}

#[cfg(unix)]
#[tokio::test]
async fn moved_tip_fails_closed() {
    let f = GitFixture::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let wrapper = dir.path().join("git-moved");
    write_exec(
        &wrapper,
        "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = ls-remote ]; then\n    printf 'ffffffffffffffffffffffffffffffffffffffff\\trefs/heads/production\\n'\n    exit 0\n  fi\ndone\nexec git \"$@\"\n",
    );

    let host = HostGit::new();
    let (state, _) = f.gateway_state(&host).await;
    let checkout = check_release_checkout(&host, &f.checkout, RELEASE, &Gate::key(&state))
        .await
        .expect("checkout");
    let err = remote_view(&HostGit::with_program(&wrapper), &checkout)
        .await
        .expect_err("the named tip is never delivered");
    match err {
        GitGateFailure::RemoteUnreachable { remote, detail } => {
            assert_eq!(remote, "origin");
            assert!(detail.contains("production"), "{detail}");
        }
        other => panic!("not unreachable: {other:?}"),
    }
}

/// T-27-12: the fetch adds objects only. No ref, no `FETCH_HEAD`, no change
/// to HEAD or the index.
#[tokio::test]
async fn remote_fetch_writes_no_ref() {
    let f = GitFixture::new();
    let b = f.breakfix_on_production();
    let snapshot = |f: &GitFixture| {
        (
            git(&f.checkout, &["for-each-ref"]),
            git(&f.checkout, &["rev-parse", "HEAD"]),
            git(&f.checkout, &["ls-files", "-s"]),
            f.checkout.join(".git").join("FETCH_HEAD").exists(),
        )
    };
    let before = snapshot(&f);
    assert!(!before.3);
    let gate = Gate::new(&f, RELEASE).await;
    assert_eq!(gate.view.heads[BRANCH], b);
    assert_eq!(snapshot(&f), before);
    // The objects did arrive.
    git(&f.checkout, &["cat-file", "-e", &format!("{b}^{{commit}}")]);
}

#[tokio::test]
async fn dirty_names_changed_script() {
    let f = GitFixture::new();
    let gate = Gate::new(&f, RELEASE).await;
    let clean = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(clean.dirty, Some(Vec::new()));

    put(&f.project_dir(), CODE, "def f():\n    return 99\n");
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(nodes.dirty, Some(vec![CODE.to_string()]));
    assert_eq!(nodes.dirty_count, 1);
    assert!(!nodes.needs_gateway_objects);
}

#[tokio::test]
async fn dirty_ignores_gateway_form() {
    let f = GitFixture::new();
    let gate = Gate::new(&f, RELEASE).await;
    let original =
        std::fs::read_to_string(f.project_dir().join(UTIL_RES)).expect("read resource.json");
    let rewritten = gateway_form(&original);
    assert_ne!(rewritten, original);
    put(&f.project_dir(), UTIL_RES, &rewritten);
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(nodes.dirty, Some(Vec::new()));
    assert_eq!(nodes.dirty_count, 0);
}

#[tokio::test]
async fn dirty_names_added_and_removed() {
    let f = GitFixture::new();
    let gate = Gate::new(&f, RELEASE).await;
    let removed = "com.inductiveautomation.perspective/views/V/view.json";
    std::fs::remove_file(f.project_dir().join(removed)).expect("remove");
    put(&f.project_dir(), EXTRA, "x\n");
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(
        nodes.dirty,
        Some(vec![removed.to_string(), EXTRA.to_string()])
    );
    assert_eq!(nodes.dirty_count, 2);
}

#[tokio::test]
async fn dirty_deferred_when_head_missing() {
    let f = GitFixture::new();
    f.designer_commit(false);
    let gate = Gate::new(&f, RELEASE).await;
    let nodes = gate.nodes(&f, &live_now(&f)).await;
    assert_eq!(nodes.dirty, None);
    assert_eq!(nodes.dirty_count, 0);
    assert!(nodes.needs_gateway_objects);
}

#[tokio::test]
async fn dirty_capped() {
    let f = GitFixture::new();
    let gate = Gate::new(&f, RELEASE).await;
    let head = head_digest(&gate.host, &GitDir::Checkout(f.checkout.clone()), &f.c1)
        .await
        .expect("head digest");
    let mut live = head.clone();
    for i in 0..500 {
        live.members.insert(
            format!("ignition/script-python/cap/gen/m{i:03}/code.py"),
            format!("{i:064x}"),
        );
    }
    let all = dirty_paths(&live, &head);
    assert_eq!(all.len(), 500);
    assert!(all.windows(2).all(|w| w[0] < w[1]), "byte-sorted");

    let nodes = gate.nodes(&f, &live).await;
    let dirty = nodes.dirty.expect("computed");
    assert_eq!(dirty.len(), LIST_CAP);
    assert_eq!(dirty[..], all[..LIST_CAP]);
    assert_eq!(nodes.dirty_count, 500);
}

// ---- 27-10: the dirty paths of an unpushed gateway HEAD, from the gateway's
// own objects (D-24) ------------------------------------------------------------

mod gateway_objects {
    use super::*;
    use ignition_core::deploy::git_gate::DirtySource;
    use ignition_core::deploy::git_objects::{
        FetchStats, GIT_OBJECTS_FETCH_MAX_BYTES, GIT_OBJECTS_LOOSE_MAX, GitObjectEntry,
        GitObjectEntryKind, GitObjectFile, GitObjectList, GitPackFiles, dirty_from_gateway,
        gateway_head_digest,
    };
    use ignition_core::deploy::transport::local::LocalFsTransport;
    use ignition_core::error::CoreError;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    const VIEW: &str = "com.inductiveautomation.perspective/views/V/view.json";

    /// What a wrapper around a local node saw.
    #[derive(Default)]
    struct Calls {
        listed: usize,
        fetched: Vec<String>,
        dests: Vec<PathBuf>,
    }

    /// A [`GitHostRead`] over the local node that records every object read,
    /// and can replace the listing.
    struct Recording {
        inner: LocalFsTransport,
        calls: Mutex<Calls>,
        listing: Mutex<Option<Result<GitObjectList, String>>>,
    }

    impl Recording {
        fn new(inner: LocalFsTransport) -> Recording {
            Recording {
                inner,
                calls: Mutex::new(Calls::default()),
                listing: Mutex::new(None),
            }
        }

        fn replace_listing(&self, listing: Result<GitObjectList, String>) {
            *self.listing.lock().expect("lock") = Some(listing);
        }

        fn fetched(&self) -> Vec<String> {
            self.calls.lock().expect("lock").fetched.clone()
        }

        fn dests(&self) -> Vec<PathBuf> {
            self.calls.lock().expect("lock").dests.clone()
        }
    }

    #[async_trait::async_trait]
    impl GitHostRead for Recording {
        async fn project_file_digests(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_advance::ProjectDisk, CoreError> {
            self.inner.project_file_digests(project).await
        }
        async fn read_git_state(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_state::GitFiles, CoreError> {
            self.inner.read_git_state(project).await
        }
        async fn git_manifest(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.inner.git_manifest(project).await
        }
        async fn save_manifest(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.inner.save_manifest(gateway, project, run_id).await
        }
        async fn git_file_digest(
            &self,
            project: &str,
            name: &GitWriteName,
        ) -> Result<Option<String>, CoreError> {
            self.inner.git_file_digest(project, name).await
        }
        async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
            self.inner.read_git_yaml().await
        }
        async fn project_dir_entry(
            &self,
            project: &str,
            dir: ignition_core::deploy::transport::projects::ImportDir,
        ) -> Result<ignition_core::deploy::transport::projects::GitEntry, CoreError> {
            self.inner.project_dir_entry(project, dir).await
        }
        async fn free_bytes(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
            self.inner.free_bytes(gateway, project).await
        }
        async fn git_size(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitSize, CoreError> {
            self.inner.git_size(project).await
        }
        async fn list_git_objects(&self, project: &str) -> Result<GitObjectList, CoreError> {
            self.calls.lock().expect("lock").listed += 1;
            let replaced = self.listing.lock().expect("lock").clone();
            match replaced {
                Some(Ok(list)) => Ok(list),
                Some(Err(why)) => Err(CoreError::TransportFailed {
                    operation: "read".to_string(),
                    node: "recording".to_string(),
                    detail: why,
                }),
                None => self.inner.list_git_objects(project).await,
            }
        }
        async fn fetch_git_objects(
            &self,
            project: &str,
            files: &[GitObjectFile],
            dest: &Path,
        ) -> Result<(), CoreError> {
            {
                let mut calls = self.calls.lock().expect("lock");
                calls.fetched.extend(files.iter().map(|f| f.rel.clone()));
                calls.dests.push(dest.to_path_buf());
            }
            self.inner.fetch_git_objects(project, files, dest).await
        }
    }

    /// An unpushed designer commit that changes `CODE`, then a live change to
    /// the view: the gateway's HEAD is unknown to ign's checkout.
    async fn unpushed_scene(f: &GitFixture) -> (Gate, CanonicalDigest, String) {
        unpushed_scene_with(f, |_| {}).await
    }

    /// The same scene with `prepare` run on the gateway clone before the gate
    /// reads it (a gc, a pack split, an object dropped).
    async fn unpushed_scene_with(
        f: &GitFixture,
        prepare: impl FnOnce(&GitFixture),
    ) -> (Gate, CanonicalDigest, String) {
        let d = f.designer_commit_changing(CODE, "def f():\n    return 7\n", false);
        f.live_change(VIEW, "{\"root\":{\"live\":true}}\n");
        prepare(f);
        let gate = Gate::new(f, RELEASE).await;
        let live = live_now(f);
        (gate, live, d)
    }

    /// ign's checkout as a snapshot: every object file with its size, refs,
    /// HEAD and the index bytes.
    fn snapshot(f: &GitFixture) -> (Vec<(String, u64)>, String, String, Vec<u8>) {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, u64)>) {
            for entry in std::fs::read_dir(dir).expect("read_dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                if entry.file_type().expect("type").is_dir() {
                    walk(&path, root, out);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .expect("below")
                        .display()
                        .to_string();
                    out.push((rel, entry.metadata().expect("meta").len()));
                }
            }
        }
        let git_dir = f.checkout.join(".git");
        let mut files = Vec::new();
        walk(
            &git_dir.join("objects"),
            &git_dir.join("objects"),
            &mut files,
        );
        files.sort();
        (
            files,
            git(&f.checkout, &["for-each-ref"]),
            git(&f.checkout, &["rev-parse", "HEAD"]),
            std::fs::read(git_dir.join("index")).expect("index"),
        )
    }

    fn entry(rel: &str, size: u64, kind: GitObjectEntryKind) -> GitObjectEntry {
        GitObjectEntry {
            rel: rel.to_string(),
            size,
            kind,
        }
    }

    #[tokio::test]
    async fn gate_unpushed_dirty_listed() {
        let f = GitFixture::new();
        let (gate, live, d) = unpushed_scene(&f).await;
        let mut nodes = gate.nodes(&f, &live).await;
        assert_eq!(nodes.head.as_deref(), Some(d.as_str()));
        assert_eq!(nodes.dirty, None);
        assert!(nodes.needs_gateway_objects);
        assert_eq!(nodes.dirty_source, None);

        let stats = dirty_from_gateway(
            &gate.host,
            &f.checkout,
            &f.node(),
            PROJECT,
            &mut nodes,
            &live,
        )
        .await
        .expect("dirty from the gateway's objects");
        // Relative to G: CODE equals what D committed, so only the view that
        // changed after D is dirty.
        assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
        assert_eq!(nodes.dirty_count, 1);
        assert_eq!(nodes.dirty_source, Some(DirtySource::GatewayObjects));
        assert_eq!(
            nodes.dirty_source.expect("source").as_str(),
            "gateway_objects"
        );
        assert!(!nodes.needs_gateway_objects);
        assert!(stats.loose > 0, "{stats:?}");
    }

    #[tokio::test]
    async fn gate_release_store_source_is_named() {
        let f = GitFixture::new();
        let gate = Gate::new(&f, RELEASE).await;
        let nodes = gate.nodes(&f, &live_now(&f)).await;
        assert_eq!(nodes.dirty_source, Some(DirtySource::ReleaseStore));
        assert_eq!(
            nodes.dirty_source.expect("source").as_str(),
            "release_store"
        );
    }

    #[tokio::test]
    async fn gateway_objects_noop_when_the_store_holds_g() {
        let f = GitFixture::new();
        let gate = Gate::new(&f, RELEASE).await;
        let live = live_now(&f);
        let mut nodes = gate.nodes(&f, &live).await;
        let before = nodes.clone();
        let rec = Recording::new(f.node());
        let stats = dirty_from_gateway(&gate.host, &f.checkout, &rec, PROJECT, &mut nodes, &live)
            .await
            .expect("no-op");
        assert_eq!(stats, FetchStats::default());
        assert_eq!(nodes, before);
        assert_eq!(rec.calls.lock().expect("lock").listed, 0);
    }

    #[tokio::test]
    async fn gateway_objects_loose_only() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene(&f).await;
        let mut nodes = gate.nodes(&f, &live).await;
        let rec = Recording::new(f.node());
        let stats = dirty_from_gateway(&gate.host, &f.checkout, &rec, PROJECT, &mut nodes, &live)
            .await
            .expect("dirty");
        assert!(stats.loose > 0, "{stats:?}");
        assert_eq!((stats.idx, stats.packs), (0, 0), "{stats:?}");
        assert!(rec.fetched().iter().all(|r| !r.contains("/pack/")));
    }

    #[tokio::test]
    async fn gateway_objects_leave_no_trace() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene(&f).await;
        let mut nodes = gate.nodes(&f, &live).await;
        let ign_before = snapshot(&f);
        let gateway_before = f.node().git_manifest(PROJECT).await.expect("manifest");
        let rec = Recording::new(f.node());
        dirty_from_gateway(&gate.host, &f.checkout, &rec, PROJECT, &mut nodes, &live)
            .await
            .expect("dirty");
        assert_eq!(snapshot(&f), ign_before, "ign's object store, refs, index");
        assert_eq!(
            f.node().git_manifest(PROJECT).await.expect("manifest"),
            gateway_before,
            "the gateway .git"
        );
        let dests = rec.dests();
        assert!(!dests.is_empty());
        for dest in dests {
            assert!(!dest.exists(), "the scratch dir was removed: {dest:?}");
        }
    }

    /// Every rel a recording saw is one of the fixed object shapes.
    fn assert_only_object_rels(rec: &Recording) {
        use ignition_core::deploy::git_objects::check_object_rel;
        for rel in rec.fetched() {
            assert!(
                check_object_rel(&rel).is_ok(),
                "{rel} is not an object file"
            );
            assert!(
                !rel.starts_with("config") && !rel.starts_with("hooks") && !rel.contains("info"),
                "{rel}"
            );
        }
    }

    async fn run(
        f: &GitFixture,
        gate: &Gate,
        live: &CanonicalDigest,
        rec: &Recording,
    ) -> (NodeGates, Result<FetchStats, GitGateFailure>) {
        let mut nodes = gate.nodes(f, live).await;
        let result =
            dirty_from_gateway(&gate.host, &f.checkout, rec, PROJECT, &mut nodes, live).await;
        (nodes, result)
    }

    #[tokio::test]
    async fn gateway_objects_from_pack() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene_with(&f, |f| f.gateway_gc()).await;
        let rec = Recording::new(f.node());
        let (nodes, result) = run(&f, &gate, &live, &rec).await;
        let stats = result.expect("dirty from a pack");
        assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
        assert_eq!(nodes.dirty_source, Some(DirtySource::GatewayObjects));
        assert_eq!(stats.loose, 0, "{stats:?}");
        assert!(stats.packs >= 1 && stats.idx >= 1, "{stats:?}");
        assert_only_object_rels(&rec);
    }

    #[tokio::test]
    async fn gateway_objects_selects_packs() {
        let f = GitFixture::new();
        let mut names = None;
        let (gate, live, _d) =
            unpushed_scene_with(&f, |f| names = Some(f.gateway_split_packs())).await;
        let (history, unpushed) = names.expect("split");
        let rec = Recording::new(f.node());
        let (nodes, result) = run(&f, &gate, &live, &rec).await;
        let stats = result.expect("dirty");
        assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
        assert_eq!(
            (stats.packs, stats.idx, stats.loose),
            (1, 2, 0),
            "{stats:?}"
        );
        let fetched = rec.fetched();
        assert!(
            fetched.contains(&format!("objects/pack/pack-{unpushed}.pack")),
            "{fetched:?}"
        );
        assert!(
            !fetched.contains(&format!("objects/pack/pack-{history}.pack")),
            "the history pack holds nothing G's tree lacks: {fetched:?}"
        );
        assert_only_object_rels(&rec);
    }

    /// 27-47 (C3): a pack is checked against its index (the index records the
    /// pack's trailer), so a round fetches every pack together with its own
    /// index: the `.idx` is the file right after its `.pack`. The indexes are
    /// also read once on their own to choose the packs.
    #[tokio::test]
    async fn gateway_objects_fetches_each_pack_with_its_index() {
        let f = GitFixture::new();
        let mut names = None;
        let (gate, live, _d) =
            unpushed_scene_with(&f, |f| names = Some(f.gateway_split_packs())).await;
        let (_history, unpushed) = names.expect("split");
        let rec = Recording::new(f.node());
        let (nodes, result) = run(&f, &gate, &live, &rec).await;
        result.expect("dirty");
        assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
        let fetched = rec.fetched();
        let pack = format!("objects/pack/pack-{unpushed}.pack");
        let at = fetched
            .iter()
            .position(|rel| *rel == pack)
            .expect("the pack was fetched");
        assert_eq!(
            fetched.get(at + 1).map(String::as_str),
            Some(format!("objects/pack/pack-{unpushed}.idx").as_str()),
            "the pack's own index comes with it: {fetched:?}"
        );
    }

    #[tokio::test]
    async fn gateway_objects_rounds_with_an_empty_ign_store() {
        let f = GitFixture::new();
        let (gate, live, d) = unpushed_scene_with(&f, |f| {
            f.gateway_split_packs();
        })
        .await;
        // An empty checkout: ign's store holds nothing, so the commit itself
        // is missing first, then the rest of its tree from the history pack.
        let empty = tempfile::tempdir().expect("tempdir");
        git(empty.path(), &["init", "-q"]);
        let rec = Recording::new(f.node());
        let mut nodes = gate.nodes(&f, &live).await;
        let stats = dirty_from_gateway(&gate.host, empty.path(), &rec, PROJECT, &mut nodes, &live)
            .await
            .expect("dirty with an empty store");
        assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
        assert_eq!(stats.packs, 2, "{stats:?}");
        let digest = gateway_head_digest(&gate.host, empty.path(), &rec, PROJECT, &d)
            .await
            .expect("digest")
            .0;
        assert_eq!(
            digest,
            head_digest(&gate.host, &GitDir::Checkout(f.project_dir()), &d)
                .await
                .expect("the gateway's own view of its HEAD")
        );
    }

    #[tokio::test]
    async fn gateway_objects_missing_fails_closed() {
        let f = GitFixture::new();
        let mut blob = String::new();
        let (gate, live, d) = unpushed_scene_with(&f, |f| {
            blob = f.git_in_project(&["rev-parse", &format!("HEAD:{CODE}")]);
            f.gateway_drop_object(&blob);
        })
        .await;
        let rec = Recording::new(f.node());
        let (nodes, result) = run(&f, &gate, &live, &rec).await;
        match result.expect_err("the gateway cannot supply G's tree") {
            GitGateFailure::GatewayObjects { detail } => {
                assert!(detail.contains("1 objects"), "{detail}");
                assert!(detail.contains(&d[..12]), "{detail}");
                assert!(!detail.contains(&blob), "ids are short: {detail}");
            }
            other => panic!("not a gateway-objects failure: {other:?}"),
        }
        // Nothing was decided: dirty is neither listed nor clean.
        assert_eq!(nodes.dirty, None);
        assert_eq!(nodes.dirty_source, None);
        assert!(nodes.needs_gateway_objects);
        for dest in rec.dests() {
            assert!(!dest.exists(), "the scratch dir was removed: {dest:?}");
        }
    }

    #[tokio::test]
    async fn gateway_objects_names_checked() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene(&f).await;

        // A listing the host refuses (a symlink, a `..` name) never reaches a
        // fetch.
        let symlink =
            GitObjectList::from_entries(&[entry("objects/ab", 0, GitObjectEntryKind::Symlink)]);
        let dotdot = GitObjectList::from_entries(&[entry(
            "objects/ab/../../config",
            10,
            GitObjectEntryKind::File,
        )]);
        for refused in [symlink, dotdot] {
            let why = refused.expect_err("the listing is refused");
            let rec = Recording::new(f.node());
            rec.replace_listing(Err(why));
            let (nodes, result) = run(&f, &gate, &live, &rec).await;
            assert!(matches!(result, Err(GitGateFailure::GatewayObjects { .. })));
            assert!(rec.fetched().is_empty());
            assert!(rec.dests().is_empty());
            assert_eq!(nodes.dirty, None);
        }

        // A listing built by hand with a hostile name is re-checked here.
        let rec = Recording::new(f.node());
        rec.replace_listing(Ok(GitObjectList {
            loose: vec![GitObjectFile {
                rel: "objects/ab/../../config".to_string(),
                size: 1,
            }],
            packs: Vec::new(),
        }));
        let (_, result) = run(&f, &gate, &live, &rec).await;
        assert!(matches!(result, Err(GitGateFailure::GatewayObjects { .. })));
        assert!(rec.fetched().is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn gateway_objects_refuse_a_real_symlink() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene(&f).await;
        let outside = tempfile::tempdir().expect("tempdir");
        std::os::unix::fs::symlink(
            outside.path(),
            f.git_dir().join("objects").join("not-hex-link"),
        )
        .expect("symlink");
        let rec = Recording::new(f.node());
        let (_, result) = run(&f, &gate, &live, &rec).await;
        assert!(matches!(result, Err(GitGateFailure::GatewayObjects { .. })));
        assert!(rec.fetched().is_empty());
        assert!(rec.dests().is_empty());
    }

    #[tokio::test]
    async fn gateway_objects_ignore_everything_but_object_files() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene_with(&f, |f| f.gateway_gc()).await;
        let objects = f.git_dir().join("objects");
        std::fs::create_dir_all(objects.join("info")).expect("info");
        std::fs::write(objects.join("info").join("packs"), "P pack-x.pack\n").expect("packs");
        std::fs::write(objects.join("info").join("junk"), "x").expect("junk");
        std::fs::write(
            objects
                .join("pack")
                .join(format!("pack-{}.keep", "a".repeat(40))),
            "",
        )
        .expect("keep");
        std::fs::write(objects.join("pack").join("tmp_pack_x"), "partial").expect("tmp");
        let rec = Recording::new(f.node());
        let (nodes, result) = run(&f, &gate, &live, &rec).await;
        result.expect("dirty");
        assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
        for rel in rec.fetched() {
            assert!(
                !rel.contains("info") && !rel.contains(".keep") && !rel.contains("tmp_pack"),
                "{rel}"
            );
        }
        assert_only_object_rels(&rec);
    }

    #[tokio::test]
    async fn gateway_objects_caps() {
        let f = GitFixture::new();
        let (gate, live, _d) = unpushed_scene(&f).await;

        let many_loose = GitObjectList {
            loose: (0..=GIT_OBJECTS_LOOSE_MAX)
                .map(|i| GitObjectFile {
                    rel: format!("objects/{:02x}/{:038x}", i % 256, i),
                    size: 1,
                })
                .collect(),
            packs: Vec::new(),
        };
        let big_packs = GitObjectList {
            loose: Vec::new(),
            packs: (1..=2)
                .map(|i| GitPackFiles {
                    name: format!("{i:040x}"),
                    pack_size: GIT_OBJECTS_FETCH_MAX_BYTES / 2 + 1,
                    idx_size: 1,
                })
                .collect(),
        };
        for (listing, word) in [(many_loose, "loose objects"), (big_packs, "byte cap")] {
            let rec = Recording::new(f.node());
            rec.replace_listing(Ok(listing));
            let (nodes, result) = run(&f, &gate, &live, &rec).await;
            match result.expect_err("over a cap") {
                GitGateFailure::GatewayObjects { detail } => {
                    assert!(detail.contains(word), "{detail}")
                }
                other => panic!("not a gateway-objects failure: {other:?}"),
            }
            assert!(rec.fetched().is_empty(), "no fetch before the cap check");
            assert!(rec.dests().is_empty());
            assert_eq!(nodes.dirty, None);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn gateway_objects_never_reads_gateway_config() {
        use std::os::unix::fs::PermissionsExt;
        // Loose objects, packed objects and split packs, each against a repo
        // whose hooks and config would run a program if anything read them.
        for prepare in 0..3 {
            let f = GitFixture::new();
            let marker = f.data_dir.join("hook-ran");
            let (gate, live, _d) = unpushed_scene_with(&f, |f| match prepare {
                1 => f.gateway_gc(),
                2 => {
                    f.gateway_split_packs();
                }
                _ => {}
            })
            .await;
            let hooks = f.git_dir().join("hooks");
            std::fs::create_dir_all(&hooks).expect("hooks");
            for name in [
                "post-checkout",
                "reference-transaction",
                "post-index-change",
            ] {
                let hook = hooks.join(name);
                std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display()))
                    .expect("hook");
                std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
                    .expect("chmod");
            }
            let config = std::fs::read_to_string(f.git_dir().join("config")).expect("config");
            std::fs::write(
                f.git_dir().join("config"),
                format!(
                    "{config}[core]\n\tfsmonitor = touch {m}\n\thooksPath = {h}\n[filter \"x\"]\n\tsmudge = touch {m}\n",
                    m = marker.display(),
                    h = hooks.display()
                ),
            )
            .expect("config");
            let rec = Recording::new(f.node());
            let (nodes, result) = run(&f, &gate, &live, &rec).await;
            result.expect("dirty");
            assert_eq!(nodes.dirty, Some(vec![VIEW.to_string()]));
            assert_only_object_rels(&rec);
            assert!(
                !marker.exists(),
                "a gateway hook or setting ran (case {prepare})"
            );
        }
    }
}

/// 27-12: the published path of `ign deploy projects` over a project the git
/// module manages, driven through `deploy_projects_prepare` and
/// `deploy_projects_run` against `FakeProjects` with a disk mirror (R-3) and a
/// real gateway repo built by host git (`GitFixture`).
mod published {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    use common::fake_projects::FakeProjects;
    use common::git_fixture::members_of;
    use ignition_core::actions::deploy_projects::git::GitReport;
    use ignition_core::actions::deploy_projects::{
        DeployProjectsResult, GIT_RESTORE_FAILED_SLUG, LoadedProject, NoProjectsObserver,
        ProjectOutcome, ProjectsInput, ProjectsPrepared, ProjectsRunOptions, ProjectsSurfaces,
        TagDeleteSurface, deploy_projects_prepare, deploy_projects_run, load_project,
    };
    use ignition_core::deploy::apply::preflight::{DesignersMode, DesignersPolicy};
    use ignition_core::deploy::git_remote::HostGit as Git;
    use ignition_core::deploy::transport::local::LocalFsTransport;
    use ignition_core::deploy::transport::projects::{
        GIT_CHANGED_OPERATION, GitEntry, ProjectHost, ProjectRunWriter,
    };
    use ignition_core::error::CoreError;

    const PROVIDER: &str = "PlantA";

    /// The master's run writer, which refuses the `run.json` rewrite that
    /// says the project import was sent (27-53).
    pub struct FlagRefuser {
        pub inner: LocalFsTransport,
    }

    #[async_trait::async_trait]
    impl ProjectRunWriter for FlagRefuser {
        async fn write_journal(
            &self,
            gateway: &str,
            project: &str,
            bytes: &[u8],
        ) -> Result<(), CoreError> {
            self.inner.write_journal(gateway, project, bytes).await
        }

        async fn clear_journal(&self, gateway: &str, project: &str) -> Result<(), CoreError> {
            self.inner.clear_journal(gateway, project).await
        }

        async fn write_pre_image(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
            name: &str,
            bytes: &[u8],
        ) -> Result<(), CoreError> {
            let sent = serde_json::from_slice::<serde_json::Value>(bytes)
                .ok()
                .and_then(|v| v["import_sent"].as_bool());
            if name == "run.json" && sent == Some(true) {
                return Err(CoreError::TransportFailed {
                    operation: "write".to_string(),
                    node: "nodes.master".to_string(),
                    detail: "the host refused the import flag".to_string(),
                });
            }
            self.inner
                .write_pre_image(gateway, project, run_id, name, bytes)
                .await
        }
    }

    /// A host that answers like the local transport and runs `program` as
    /// host git (a stub, to prove git is never run or to make one step fail).
    pub struct StubHost {
        pub inner: LocalFsTransport,
        pub program: Option<OsString>,
        /// The remote token host git holds (`IGNITION_GIT_TOKEN` in a CI run).
        pub token: Option<String>,
        /// Answers the host's git reads instead of the local transport.
        pub read: Option<Box<dyn GitHostRead>>,
    }

    #[async_trait::async_trait]
    impl ProjectHost for StubHost {
        async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError> {
            self.inner.git_entry(project).await
        }
        async fn read_release_record(
            &self,
            gateway: &str,
            project: &str,
        ) -> Result<Option<Vec<u8>>, CoreError> {
            self.inner.read_release_record(gateway, project).await
        }
        async fn read_tag_record(
            &self,
            gateway: &str,
            provider: &str,
        ) -> Result<Option<Vec<u8>>, CoreError> {
            self.inner.read_tag_record(gateway, provider).await
        }
        async fn read_journal(
            &self,
            gateway: &str,
            project: &str,
        ) -> Result<Option<Vec<u8>>, CoreError> {
            ProjectHost::read_journal(&self.inner, gateway, project).await
        }
        async fn read_pre_image(
            &self,
            gateway: &str,
            project: &str,
            name: &str,
        ) -> Result<Option<Vec<u8>>, CoreError> {
            self.inner.read_pre_image(gateway, project, name).await
        }
        fn git(&self) -> Option<&dyn GitHostRead> {
            match &self.read {
                Some(read) => Some(read.as_ref()),
                None => ProjectHost::git(&self.inner),
            }
        }
        fn host_git(&self) -> Git {
            match &self.program {
                Some(program) => Git::with_program(program).and_remote_token(self.token.clone()),
                None => Git::with_remote_token(self.token.clone()),
            }
        }
    }

    /// Fixture, targets root and the fake gateway.
    pub struct Rig {
        pub f: GitFixture,
        pub targets: tempfile::TempDir,
        pub gateway: FakeProjects,
        /// The release checkout the run deploys from.
        pub repo: PathBuf,
        /// The release tag.
        pub release: String,
        /// `--discard-gateway-changes`, when the run gives one (27-15).
        pub discard: Option<String>,
        /// `--allow-git-sync-hazards`, when the run gives one (27-18).
        pub hazard_ack: Option<String>,
        /// The master's tree reader, for a tag-owning pair (27-22): the
        /// mode-collection check of a pair reads it.
        pub tree: Option<LocalFsTransport>,
        /// The poll interval of the wait for the backup, which is also the
        /// settle the backup's `.git` gets before its save goes (27-35).
        pub backup_interval: std::time::Duration,
        /// The master's host refuses the `run.json` rewrite that says the
        /// import was sent (27-53).
        pub fail_import_flag: bool,
        /// The `--designers` policy of a `--rollback-run` (`fail` with the
        /// production wait when `None`): a test that leaves a session
        /// attached shortens it (27-54).
        pub rollback_designers: Option<DesignersPolicy>,
    }

    pub fn put_file(root: &std::path::Path, rel: &str, body: &str) {
        put(root, rel, body);
    }

    impl Rig {
        pub fn new() -> Rig {
            Rig::with(|g| g)
        }

        /// A rig whose gateway is shaped by `shape` (a mirror knob).
        pub fn with(shape: impl FnOnce(FakeProjects) -> FakeProjects) -> Rig {
            Rig::build(false, true, shape)
        }

        /// A rig whose project owns the tags of a provider (Phase 26), so a
        /// failed project step rolls the run back. `registered` is whether the
        /// git module manages the project (R-3: an import removes `.git`) or
        /// not (R-6: it stays).
        pub fn tagged(registered: bool) -> Rig {
            Rig::build(true, registered, |g| g)
        }

        /// [`Rig::tagged`] with the gateway shaped by `shape`.
        pub fn tagged_with(
            registered: bool,
            shape: impl FnOnce(FakeProjects) -> FakeProjects,
        ) -> Rig {
            Rig::build(true, registered, shape)
        }

        fn build(
            owned: bool,
            registered: bool,
            shape: impl FnOnce(FakeProjects) -> FakeProjects,
        ) -> Rig {
            let f = GitFixture::new();
            let tag_line = if owned {
                format!("tag_providers: [{PROVIDER}]\n")
            } else {
                String::new()
            };
            put_file(
                &f.checkout,
                "deploy/project.yaml",
                &format!("schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\n{tag_line}"),
            );
            if owned {
                put_file(
                    &f.checkout,
                    &format!("deploy/tag-providers/{PROVIDER}.json"),
                    r#"{"schema_version": 1, "enabled": true, "config": {}}"#,
                );
                put_file(
                    &f.checkout,
                    &format!("tags/{PROVIDER}/Line1/Count.json"),
                    r#"{"dataType":"Int4","documentation":"new","tagType":"AtomicTag","value":1,"valueSource":"memory"}"#,
                );
                let record = ignition_core::deploy::tag_record::TagRecord {
                    schema_version: 1,
                    gateway: "capgw".to_string(),
                    provider: PROVIDER.to_string(),
                    project: PROJECT.to_string(),
                    release: "v0.9.0".to_string(),
                    run_id: "20261020T101500Z-ab12".to_string(),
                    deployed_at: "2026-10-20T10:15:00Z".to_string(),
                    tags_sha256: "ab".repeat(32),
                    nodes: vec!["Line1".to_string()],
                    groups: Vec::new(),
                };
                std::fs::create_dir_all(
                    f.data_dir
                        .join(format!("ign-deploy/capgw/tag-providers/{PROVIDER}")),
                )
                .expect("the tag record directory");
                std::fs::write(
                    f.data_dir.join(format!(
                        "ign-deploy/capgw/tag-providers/{PROVIDER}/record.json"
                    )),
                    record.to_bytes(),
                )
                .expect("the tag record");
            }
            put_file(&f.checkout, ".gitignore", ".ign/\n");
            put_file(
                &f.data_dir,
                "config/resources/core/ignition/system-properties/config.json",
                r#"{"systemName":"capgw"}"#,
            );
            let targets = tempfile::tempdir().expect("targets root");
            put_file(
                targets.path(),
                "targets/dev.yaml",
                &format!(
                    "schema_version: 1\ngateway: capgw\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: '{}'\n",
                    f.data_dir.display()
                ),
            );
            let members = members_of(&f.project_dir());
            let borrowed: Vec<(&str, Vec<u8>)> = members
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.clone()))
                .collect();
            let mut gateway = FakeProjects::new()
                .with_project(PROJECT, None, false, &borrowed)
                .with_disk_mirror(&f.data_dir, registered);
            if owned {
                gateway = gateway.with_tag_tree(
                    PROVIDER,
                    serde_json::json!({"name":"","tagType":"Provider","tags":[
                        {"name":"_types_","tagType":"Folder"},
                        {"name":"Line1","tagType":"Folder","tags":[
                            {"name":"Count","tagType":"AtomicTag","dataType":"Int4",
                             "valueSource":"memory","value":9,"defaultValue":1,
                             "documentation":"old"}
                        ]}
                    ]}),
                );
            }
            let gateway = shape(gateway);
            let repo = f.checkout.clone();
            Rig {
                f,
                targets,
                gateway,
                repo,
                release: "v1.1.0".to_string(),
                discard: None,
                hazard_ack: None,
                tree: None,
                backup_interval: std::time::Duration::from_millis(5),
                fail_import_flag: false,
                rollback_designers: None,
            }
        }

        /// A tag-owning pair: the master's tree reader is supplied and the
        /// target declares `nodes.backup`.
        pub fn tagged_pair(registered: bool) -> Rig {
            let mut rig = Rig::tagged(registered);
            rig.tree = Some(rig.f.node());
            rig.add_backup();
            rig
        }

        pub fn input(&self) -> ProjectsInput {
            ProjectsInput {
                repo: self.repo.clone(),
                targets: self.targets.path().to_path_buf(),
                env: "dev".to_string(),
                release: self.release.clone(),
                allow_downgrade: false,
                allow_first_change: Some("first deploy of the rig".to_string()),
                discard_gateway_changes: self.discard.clone(),
                allow_git_sync_hazards: self.hazard_ack.clone(),
                allow_destructive: false,
                allow_manual_undo: false,
                artifact_sha256: None,
                tags_sha256: None,
            }
        }

        pub fn host(&self) -> StubHost {
            StubHost {
                inner: self.f.node(),
                token: None,
                program: None,
                read: None,
            }
        }

        /// A host whose host git is `program`.
        #[cfg(unix)]
        pub fn host_with_program(&self, program: &std::path::Path) -> StubHost {
            StubHost {
                inner: self.f.node(),
                token: None,
                program: Some(program.as_os_str().to_os_string()),
                read: None,
            }
        }

        /// `--rollback-run run_id` of this rig's project, as a fresh process
        /// on another runner would make it (27-21, D-08).
        pub async fn rollback_run(
            &self,
            run_id: &str,
            check: bool,
        ) -> Result<DeployProjectsResult, CoreError> {
            self.rollback_run_with(run_id, check, None).await
        }

        /// [`Rig::rollback_run`] on a pair: the backup node answers `backup`.
        pub async fn rollback_run_with(
            &self,
            run_id: &str,
            check: bool,
            backup: Option<(
                &dyn ignition_core::deploy::tag_converge::BackupTags,
                std::time::Duration,
            )>,
        ) -> Result<DeployProjectsResult, CoreError> {
            self.rollback_run_on(&self.host(), run_id, check, backup, None)
                .await
        }

        /// [`Rig::rollback_run_with`] through `host`, and through `git_writer`
        /// when the test supplies the master's git writer (27-34).
        pub async fn rollback_run_on(
            &self,
            host: &StubHost,
            run_id: &str,
            check: bool,
            backup: Option<(
                &dyn ignition_core::deploy::tag_converge::BackupTags,
                std::time::Duration,
            )>,
            git_writer: Option<&dyn GitStateWriter>,
        ) -> Result<DeployProjectsResult, CoreError> {
            let input = self.input();
            let loaded = ignition_core::actions::deploy_projects::load_project_for_rollback(&input)
                .expect("the checkout loads");
            ignition_core::actions::deploy_projects::deploy_projects_rollback_run(
                &loaded,
                &input,
                &ProjectsSurfaces {
                    gateway: &self.gateway,
                    host,
                    tree: self
                        .tree
                        .as_ref()
                        .map(|t| t as &dyn ignition_core::deploy::transport::TreeReader),
                    apply_host: None,
                    deploy: None,
                    tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
                },
                &ProjectsRunOptions {
                    repo: input.repo.clone(),
                    targets_root: input.targets.clone(),
                    target_file: loaded.target_file.clone(),
                    master: loaded.target.nodes.master.clone(),
                    record_writer: None,
                    tag_record_writer: None,
                    run_writer: None,
                    git_writer,
                    designers: self.rollback_designers.unwrap_or_default(),
                    children_settle: std::time::Duration::from_secs(1),
                    children_poll: std::time::Duration::from_millis(5),
                    tag_verify_settle: std::time::Duration::from_secs(1),
                    tag_verify_poll: std::time::Duration::from_millis(5),
                    backup: backup.map(|(node, _)| node),
                    backup_policy: ignition_core::deploy::pair::BackupPolicy {
                        budget: backup.map_or(std::time::Duration::from_secs(1), |(_, b)| b),
                        interval: self.backup_interval,
                    },
                    tag_delete_ready: std::time::Duration::from_millis(50),
                    tag_delete_poll: std::time::Duration::from_millis(5),
                },
                run_id,
                check,
                &NoProjectsObserver,
            )
            .await
        }

        /// Make the fake gateway hold what the project directory holds now.
        pub fn sync_live(&self) {
            for (path, bytes) in members_of(&self.f.project_dir()) {
                self.gateway.change_live(PROJECT, &path, &bytes);
            }
        }

        pub async fn plan_with(
            &self,
            host: &StubHost,
        ) -> Result<(LoadedProject, ProjectsPrepared), CoreError> {
            let input = self.input();
            let loaded = load_project(&input).expect("the checkout loads");
            let prepared = deploy_projects_prepare(
                &loaded,
                &input,
                &ProjectsSurfaces {
                    gateway: &self.gateway,
                    host,
                    tree: self
                        .tree
                        .as_ref()
                        .map(|t| t as &dyn ignition_core::deploy::transport::TreeReader),
                    apply_host: None,
                    deploy: None,
                    tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
                },
            )
            .await?;
            Ok((loaded, prepared))
        }

        pub async fn plan(&self) -> Result<(LoadedProject, ProjectsPrepared), CoreError> {
            self.plan_with(&self.host()).await
        }

        pub async fn check(&self) -> Result<DeployProjectsResult, CoreError> {
            Ok(self.plan().await?.1.into_check_result())
        }

        pub async fn run_prepared(
            &self,
            host: &StubHost,
            loaded: &LoadedProject,
            prepared: ProjectsPrepared,
            git_writer: Option<&dyn GitStateWriter>,
        ) -> Result<DeployProjectsResult, CoreError> {
            self.run_prepared_with(host, loaded, prepared, git_writer, Default::default())
                .await
        }

        pub async fn run_prepared_with(
            &self,
            host: &StubHost,
            loaded: &LoadedProject,
            prepared: ProjectsPrepared,
            git_writer: Option<&dyn GitStateWriter>,
            designers: DesignersPolicy,
        ) -> Result<DeployProjectsResult, CoreError> {
            self.run_prepared_full(host, loaded, prepared, git_writer, designers, None)
                .await
        }

        /// A redundant pair (27-22): the target gains `nodes.backup` over a
        /// second data dir that holds a byte copy of the master's clone. Call
        /// it after the master's repository is shaped and before the backup's
        /// is (the copy is taken now).
        pub fn add_backup(&self) {
            let path = self.targets.path().join("targets/dev.yaml");
            let mut text = std::fs::read_to_string(&path).expect("the target file");
            text.push_str(&format!(
                "  backup:\n    profile: devb\n    local:\n      data_dir: '{}'\n",
                self.f.backup_data_dir().display()
            ));
            std::fs::write(&path, text).expect("the target file is rewritten");
        }

        /// The fake backup node: a second fake gateway holding what the
        /// backup's data dir holds, with its own disk mirror (the module
        /// manages its project, so a sync deletes its `.git`, G5), that follows
        /// this rig's master after `lag` reads.
        pub fn backup_node(&self, lag: usize) -> FakeBackupNode<'_> {
            self.backup_node_with(lag, true)
        }

        /// [`Rig::backup_node`] whose project the module does or does not
        /// manage: with `managed` false the redundancy sync leaves the
        /// backup's `.git` in place (R-6), so the run finds it present once
        /// the backup's project converges.
        pub fn backup_node_with(&self, lag: usize, managed: bool) -> FakeBackupNode<'_> {
            self.backup_node_losing(lag, managed, None)
        }

        /// [`Rig::backup_node_with`] whose redundancy sync also does `fate`
        /// to the backup's `project.json` on disk (27-37).
        pub fn backup_node_losing(
            &self,
            lag: usize,
            managed: bool,
            fate: Option<common::fake_projects::ProjectJsonFate>,
        ) -> FakeBackupNode<'_> {
            let members = members_of(&self.f.backup_project_dir());
            let borrowed: Vec<(&str, Vec<u8>)> = members
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.clone()))
                .collect();
            let mut gw = FakeProjects::new()
                .with_project(PROJECT, None, false, &borrowed)
                .with_disk_mirror(&self.f.backup_data_dir(), managed);
            if let Some(fate) = fate {
                gw = gw.with_disk_mirror_project_json(fate);
            }
            FakeBackupNode {
                master: &self.gateway,
                gw,
                lag,
                reads: std::sync::Mutex::new(Vec::new()),
                drop_git_at_first_read: false,
                drop_project_json_at_first_read: false,
                tags_differ: false,
                backup_git: self.f.backup_git_dir(),
                on_tag_export: None,
                tag_exports: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        pub async fn run_prepared_full(
            &self,
            host: &StubHost,
            loaded: &LoadedProject,
            prepared: ProjectsPrepared,
            git_writer: Option<&dyn GitStateWriter>,
            designers: DesignersPolicy,
            backup: Option<(
                &dyn ignition_core::deploy::tag_converge::BackupTags,
                std::time::Duration,
            )>,
        ) -> Result<DeployProjectsResult, CoreError> {
            let input = self.input();
            let refuser = self.fail_import_flag.then(|| FlagRefuser {
                inner: self.f.node(),
            });
            deploy_projects_run(
                prepared,
                loaded,
                &input,
                &ProjectsSurfaces {
                    gateway: &self.gateway,
                    host,
                    tree: self
                        .tree
                        .as_ref()
                        .map(|t| t as &dyn ignition_core::deploy::transport::TreeReader),
                    apply_host: None,
                    deploy: None,
                    tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
                },
                &ProjectsRunOptions {
                    repo: input.repo.clone(),
                    targets_root: input.targets.clone(),
                    target_file: loaded.target_file.clone(),
                    master: loaded.target.nodes.master.clone(),
                    record_writer: None,
                    tag_record_writer: None,
                    run_writer: refuser.as_ref().map(|w| w as &dyn ProjectRunWriter),
                    git_writer,
                    designers,
                    children_settle: std::time::Duration::from_secs(1),
                    children_poll: std::time::Duration::from_millis(5),
                    tag_verify_settle: std::time::Duration::from_secs(1),
                    tag_verify_poll: std::time::Duration::from_millis(5),
                    backup: backup.map(|(node, _)| node),
                    backup_policy: ignition_core::deploy::pair::BackupPolicy {
                        budget: backup.map_or(std::time::Duration::from_secs(1), |(_, b)| b),
                        interval: self.backup_interval,
                    },
                    tag_delete_ready: std::time::Duration::from_millis(50),
                    tag_delete_poll: std::time::Duration::from_millis(5),
                },
                &NoProjectsObserver,
            )
            .await
        }

        /// An apply on a pair: the backup node answers `backup`, and the wait
        /// for it lasts `budget`.
        pub async fn apply_pair(
            &self,
            backup: &dyn ignition_core::deploy::tag_converge::BackupTags,
            budget: std::time::Duration,
        ) -> Result<DeployProjectsResult, CoreError> {
            self.apply_pair_on(&self.host(), backup, budget).await
        }

        /// [`Rig::apply_pair`] through `host` (a host whose git is a stub).
        pub async fn apply_pair_on(
            &self,
            host: &StubHost,
            backup: &dyn ignition_core::deploy::tag_converge::BackupTags,
            budget: std::time::Duration,
        ) -> Result<DeployProjectsResult, CoreError> {
            let (loaded, prepared) = self.plan_with(host).await?;
            self.run_prepared_full(
                host,
                &loaded,
                prepared,
                None,
                Default::default(),
                Some((backup, budget)),
            )
            .await
        }

        pub async fn apply(&self) -> Result<DeployProjectsResult, CoreError> {
            self.apply_with(&self.host()).await
        }

        /// An apply under the given `--designers` policy.
        pub async fn apply_designers(
            &self,
            designers: DesignersPolicy,
        ) -> Result<DeployProjectsResult, CoreError> {
            let host = self.host();
            let (loaded, prepared) = self.plan_with(&host).await?;
            self.run_prepared_with(&host, &loaded, prepared, None, designers)
                .await
        }

        /// The project directory holds the artifact (the import already
        /// happened outside this tool) and the gateway reports the same, so
        /// the plan is a `record`: the `.git` stays at C1 and is dirty
        /// against it.
        pub async fn make_live_equal_artifact(&self) {
            use std::io::Read as _;
            self.f.module_import(true, &[], &[], &[]);
            self.sync_live();
            // The gateway's directory holds the bytes its export carries (G2).
            let zip = ignition_core::actions::deploy_projects::ProjectGateway::export_bytes(
                &self.gateway,
                PROJECT,
            )
            .await
            .expect("export");
            let mut archive =
                zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("the export is a zip");
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).expect("entry");
                if entry.is_dir() {
                    continue;
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).expect("member");
                let path = self.f.project_dir().join(entry.name());
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(path, bytes).expect("write member");
            }
        }

        /// The run directory of `result`.
        pub fn run_path(&self, result: &DeployProjectsResult) -> PathBuf {
            self.repo
                .join(result.run_dir.as_deref().expect("the run has a directory"))
        }

        /// `events.ndjson` of the run, one JSON value per line.
        pub fn events(&self, result: &DeployProjectsResult) -> Vec<serde_json::Value> {
            std::fs::read_to_string(self.run_path(result).join("events.ndjson"))
                .expect("events")
                .lines()
                .map(|l| serde_json::from_str(l).expect("an event line"))
                .collect()
        }

        /// The names of the `git_*` events, in order.
        pub fn git_events(&self, result: &DeployProjectsResult) -> Vec<String> {
            self.events(result)
                .iter()
                .filter_map(|e| e["event"].as_str())
                .filter(|n| n.starts_with("git_"))
                .map(str::to_string)
                .collect()
        }

        pub async fn apply_with(&self, host: &StubHost) -> Result<DeployProjectsResult, CoreError> {
            let (loaded, prepared) = self.plan_with(host).await?;
            self.run_prepared(host, &loaded, prepared, None).await
        }

        /// Directories named `repo` below the project's `git-save` folder.
        pub fn save_trees(&self) -> Vec<PathBuf> {
            self.save_trees_in(&self.f.data_dir)
        }

        /// [`Rig::save_trees`] on the data dir `data`.
        pub fn save_trees_in(&self, data: &std::path::Path) -> Vec<PathBuf> {
            let root = data
                .join("ign-deploy/capgw/projects")
                .join(PROJECT)
                .join("git-save");
            let mut found = Vec::new();
            fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    return;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if path.file_name().is_some_and(|n| n == "repo") {
                            out.push(path.clone());
                        }
                        walk(&path, out);
                    }
                }
            }
            walk(&root, &mut found);
            found
        }

        pub fn run_dirs(&self) -> usize {
            std::fs::read_dir(self.repo.join(".ign/deploy/capgw"))
                .map(|d| d.count())
                .unwrap_or(0)
        }

        pub async fn git_manifest(
            &self,
        ) -> ignition_core::deploy::transport::projects::GitManifest {
            self.f.node().git_manifest(PROJECT).await.expect("manifest")
        }
    }

    pub fn git_of(result: &DeployProjectsResult) -> &GitReport {
        result.git.as_deref().expect("data.git is present")
    }

    /// The expected failure of an `Err` result.
    pub fn refusal<T: std::fmt::Debug>(r: Result<T, CoreError>) -> CoreError {
        r.expect_err("the run must refuse")
    }

    #[allow(dead_code)]
    fn _keep() {
        let _ = refusal::<()>;
    }

    /// W4: every published reason word has a producer row, and the words
    /// of the host classification and of the restore are all listed.
    #[test]
    fn reason_words_have_producers() {
        use ignition_core::actions::deploy_projects::git::{PRODUCED_HERE, REASON_PRODUCERS};
        use ignition_core::deploy::git_handover::RESTORE_REASONS;
        use ignition_core::deploy::git_state::HOST_REASONS;
        let listed: Vec<&str> = REASON_PRODUCERS.iter().map(|(w, _)| *w).collect();
        for word in HOST_REASONS
            .iter()
            .chain(RESTORE_REASONS)
            .chain(PRODUCED_HERE)
        {
            assert!(listed.contains(word), "{word} has no producer row");
        }
        for word in [
            "remote_mismatch",
            "release_not_on_branch",
            "branch_required",
            "nodes_disagree",
            "backup_unsynced",
            "post_status_mismatch",
            "designer_attached",
        ] {
            assert!(listed.contains(&word), "{word} is published without a row");
        }
        assert!(REASON_PRODUCERS.iter().all(|(_, p)| !p.is_empty()));
    }

    #[tokio::test]
    async fn published_managed_deploy_advances() {
        let rig = Rig::new();
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert!(result.ok && result.slug.is_none());
        let git = git_of(&result);
        assert_eq!(git.release_commit, rig.f.c2);
        let node = git.master();
        assert_eq!(node.state, "managed");
        assert_eq!(node.branch.as_deref(), Some(BRANCH));
        assert!(!node.adopted);
        assert_eq!(node.dirty, Some(Vec::new()));
        assert_eq!(node.dirty_source, Some("release_store"));
        let restore = node.restore.as_ref().expect("restore reported");
        assert!(restore.restored && restore.advanced && restore.verified);
        assert_eq!(restore.post_status, Some("matched"));
        assert_eq!(restore.reason, None);
        assert!(!restore.save_kept);
        assert!(git.expected_drift_complete);
        let drift = git
            .expected_drift
            .clone()
            .expect("drift known after advance");
        assert_eq!(git.expected_drift_count, Some(drift.len()));
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        assert_eq!(rig.f.status_paths(), drift);
        assert!(rig.save_trees().is_empty(), "the save is removed");
        // The release record names the release (the deploy stays).
        assert!(result.record_written);
    }

    #[tokio::test]
    async fn check_reports_git_and_writes_nothing() {
        let rig = Rig::new();
        let before = rig.git_manifest().await;
        let result = rig.check().await.expect("the check completes");
        assert_eq!(result.outcome, ProjectOutcome::Checked);
        let git = git_of(&result);
        assert_eq!(git.expected_drift, None);
        assert_eq!(git.expected_drift_count, None);
        assert!(!git.expected_drift_complete);
        let node = git.master();
        assert_eq!(node.state, "managed");
        assert_eq!(node.dirty, Some(Vec::new()));
        assert_eq!(node.dirty_count, Some(0));
        assert_eq!(node.dirty_source, Some("release_store"));
        assert!(node.save.is_none() && node.restore.is_none());
        assert!(rig.save_trees().is_empty());
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert_eq!(rig.git_manifest().await, before, ".git is byte-identical");
        assert!(rig.gateway.import_calls().is_empty());
        let json = serde_json::to_value(git).expect("json");
        assert!(json["expected_drift"].is_null());
        assert_eq!(json["expected_drift_complete"], false);
    }

    #[tokio::test]
    async fn no_git_unchanged() {
        let rig = Rig::new();
        std::fs::remove_dir_all(rig.f.git_dir()).expect("no .git");
        // A host git that cannot even start: any call to it fails the run.
        let host = StubHost {
            inner: rig.f.node(),
            token: None,
            program: Some(OsString::from("/nonexistent/git-must-not-run")),
            read: None,
        };
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, None)
            .await
            .expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert!(result.git.is_none());
        let json = serde_json::to_value(&result).expect("json");
        assert!(json["git"].is_null());
        let _ = GIT_RESTORE_FAILED_SLUG;
    }

    // ---- Task 3: refusals, preflight refusals and the D-09 outcomes ----

    /// A check and an apply refuse alike and write nothing: no import, no run
    /// directory, no save.
    async fn refused_alike(rig: &Rig) -> CoreError {
        let first = refusal(rig.check().await);
        let second = refusal(rig.apply().await);
        assert_eq!(first.code(), second.code(), "check and apply agree");
        assert_eq!(first.to_string(), second.to_string());
        assert!(
            rig.gateway.import_calls().is_empty(),
            "nothing was imported"
        );
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert!(rig.save_trees().is_empty(), "no save");
        first
    }

    fn gate_of(err: &CoreError) -> (&str, &[String], usize) {
        match err {
            CoreError::DestructiveChangeRefused { gate, keys, count } => {
                (gate.as_str(), keys.as_slice(), *count)
            }
            other => panic!("expected destructive_change_refused, got {other:?}"),
        }
    }

    fn preflight_checks(err: &CoreError) -> Vec<String> {
        match err {
            CoreError::DeployPreflightFailed { failures } => {
                failures.iter().map(|f| f.check.clone()).collect()
            }
            other => panic!("expected deploy_preflight_failed, got {other:?}"),
        }
    }

    fn reason_of(err: &CoreError) -> &str {
        match err {
            CoreError::ProjectGitManaged { reason, .. } => reason.as_str(),
            other => panic!("expected project_git_managed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn refuses_unpushed() {
        let rig = Rig::new();
        let d = rig.f.designer_commit(false);
        rig.sync_live();
        let err = refused_alike(&rig).await;
        let (gate, keys, count) = gate_of(&err);
        assert_eq!(gate, "git_unpushed");
        assert!(keys.contains(&d), "{keys:?}");
        assert_eq!(count, keys.len());
    }

    #[tokio::test]
    async fn refuses_not_merged() {
        let rig = Rig::new();
        let g = rig.f.breakfix_on_production();
        rig.sync_live();
        let err = refused_alike(&rig).await;
        let (gate, keys, count) = gate_of(&err);
        assert_eq!(gate, "git_not_merged");
        assert_eq!(keys, std::slice::from_ref(&g));
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn refuses_dirty_naming_paths() {
        let rig = Rig::new();
        let view = "com.inductiveautomation.perspective/views/V/view.json";
        rig.f.live_change(view, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        let err = refused_alike(&rig).await;
        let (gate, keys, count) = gate_of(&err);
        assert_eq!(gate, "git_dirty");
        assert_eq!(keys, [view.to_string()]);
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn refuses_release_not_on_branch() {
        let mut rig = Rig::new();
        rig.f.tag_release_on("feature", "v1.2.0");
        rig.release = "v1.2.0".to_string();
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "release_not_on_branch");
        assert!(err.hint().expect("hint").contains("tracked branch"));
    }

    #[tokio::test]
    async fn accepts_newer_remote_tip() {
        let rig = Rig::new();
        rig.f.push_to_production(
            "ignition/script-python/cap/util/code.py",
            "def f():\n    return 3\n",
        );
        let result = rig.apply().await.expect("a newer tip is accepted");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
    }

    #[tokio::test]
    async fn refuses_remote_mismatch() {
        let rig = Rig::new();
        rig.f
            .checkout_with_userinfo_remote("https://user:hunter2@example.invalid/other/repo.git");
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "remote_mismatch");
        let text = format!("{err} {err:?}");
        assert!(
            !text.contains("hunter2") && !text.contains("user:"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn refuses_detached_head() {
        let rig = Rig::new();
        rig.f.git_in_project(&["checkout", "-q", "--detach"]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "detached_head");
    }

    /// WR-A5 (27-27): a gateway branch whose upstream is another remote
    /// branch is not managed: the gate would check the wrong branch. Check and
    /// apply refuse alike, before any write.
    #[tokio::test]
    async fn refuses_upstream_of_another_branch() {
        let rig = Rig::new();
        rig.f
            .git_in_project(&["config", "branch.production.merge", "refs/heads/other"]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "no_upstream");
    }

    #[tokio::test]
    async fn refuses_shallow_checkout() {
        let mut rig = Rig::new();
        let shallow = rig.f.shallow_checkout();
        put_file(
            &shallow,
            "deploy/project.yaml",
            &format!("schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\n"),
        );
        put_file(&shallow, ".gitignore", ".ign/\n");
        rig.repo = shallow;
        let err = refused_alike(&rig).await;
        assert_eq!(preflight_checks(&err), ["git_checkout"]);
        assert!(err.to_string().contains("shallow"), "{err}");
    }

    /// 27-20 (checker W16): the token is scoped by ign from the matched
    /// remote's own URL, so an ssh origin cannot take it: `--check` and the
    /// apply refuse `git_remote` and say what to do, before the remote is
    /// asked anything. Without a token the same fixture reaches the remote
    /// step as before.
    #[cfg(unix)]
    #[tokio::test]
    async fn git_token_origin_contract() {
        const TOKEN: &str = "tok-7f3a91c2-not-a-real-token";
        let rig = Rig::new();
        let ssh = "git@git.example.test:org/capapp.git";
        rig.f.git_in_project(&["remote", "set-url", REMOTE, ssh]);
        rig.f.checkout_with_userinfo_remote(ssh);
        let scratch = tempfile::tempdir().expect("scratch");
        // The remote read fails like an unreachable host; nothing else does.
        let (program, log) = git_wrapper(scratch.path(), "--heads");
        let mut host = rig.host_with_program(&program);

        // No token: the remote read runs (and fails) as in 27-05.
        for err in [
            refusal(rig.plan_with(&host).await),
            refusal(rig.apply_with(&host).await),
        ] {
            assert_eq!(preflight_checks(&err), ["git_remote"], "{err}");
            assert!(!err.to_string().contains("git token"), "{err}");
        }
        let calls = std::fs::read_to_string(&log).expect("the log");
        assert_eq!(calls.matches("ls-remote --heads").count(), 2, "{calls}");

        // A token: refused before the remote is read, naming the fix.
        host.token = Some(TOKEN.to_string());
        for err in [
            refusal(rig.plan_with(&host).await),
            refusal(rig.apply_with(&host).await),
        ] {
            assert_eq!(preflight_checks(&err), ["git_remote"], "{err}");
            let text = format!("{err} {err:?}");
            assert!(
                text.contains(
                    "a git token applies only to https remotes; this remote uses ssh: give the \
                     machine running ign ssh access to it and leave git-token empty"
                ),
                "{text}"
            );
            assert!(!text.contains(TOKEN), "{text}");
        }
        let calls = std::fs::read_to_string(&log).expect("the log");
        assert_eq!(
            calls.matches("ls-remote --heads").count(),
            2,
            "a refused token reads no remote:\n{calls}"
        );
        assert!(rig.gateway.import_calls().is_empty());
        assert_eq!(rig.run_dirs(), 0);
    }

    /// A host git that runs git, logs its arguments and fails on request.
    #[cfg(unix)]
    fn git_wrapper(dir: &std::path::Path, fail_on: &str) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let log = dir.join("git-calls.log");
        let program = dir.join("git-wrapper");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\nfor a in \"$@\"; do\n  case \"$a\" in\n    {fail_on}) echo 'stub: refused' >&2; exit 3;;\n  esac\ndone\nexec git \"$@\"\n",
            log = log.display(),
        );
        std::fs::write(&program, script).expect("script");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        (program, log)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn advance_prep_failure_refuses_before_write() {
        let rig = Rig::new();
        let scratch = tempfile::tempdir().expect("scratch");
        let (program, _log) = git_wrapper(scratch.path(), "pack-objects");
        let host = rig.host_with_program(&program);
        // `--check` and the apply share one prepare, which builds the advance
        // input (B5): a failure there refuses before any write in both.
        let err = refusal(rig.plan_with(&host).await);
        assert_eq!(preflight_checks(&err), ["git_host"], "{err}");
        let err = refusal(rig.apply_with(&host).await);
        assert_eq!(preflight_checks(&err), ["git_host"], "{err}");
        assert!(rig.gateway.import_calls().is_empty());
        assert_eq!(rig.run_dirs(), 0);
        assert!(rig.save_trees().is_empty());
    }

    #[tokio::test]
    async fn stale_repo_between_check_and_save() {
        let rig = Rig::new();
        let host = rig.host();
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        // The gateway's HEAD moves after the plan was drawn.
        rig.f
            .git_in_project(&["commit", "-q", "--allow-empty", "-m", "moved"]);
        let err = refusal(rig.run_prepared(&host, &loaded, prepared, None).await);
        assert_eq!(err.code(), "plan_stale");
        assert!(err.to_string().contains(PROJECT), "{err}");
        assert!(
            rig.gateway.import_calls().is_empty(),
            "nothing was imported"
        );
        assert!(rig.save_trees().is_empty());
    }

    /// Makes a directory writable again so the temp dir can be removed.
    #[cfg(unix)]
    struct Unlock(PathBuf);
    #[cfg(unix)]
    impl Drop for Unlock {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn restore_failure_outcome() {
        let rig = Rig::with(|g| g.with_disk_mirror_readonly_after_import());
        let _unlock = Unlock(rig.f.project_dir());
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert!(!result.ok);
        assert_eq!(result.slug.as_deref(), Some(GIT_RESTORE_FAILED_SLUG));
        assert_eq!(result.failure_exit_code(), Some(6));
        assert!(result.record_written, "the deploy and its record stay");
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore");
        assert_eq!(restore.reason.as_deref(), Some("restore_failed"));
        assert!(!restore.advanced && restore.save_kept);
        let save = node.save.as_ref().expect("the save is named");
        assert!(save.path.contains("git-save"), "{}", save.path);
        assert!(rig.f.data_dir.join(&save.path).is_dir(), "the save is kept");
    }

    /// A git writer that lets the restore through and fails the advance.
    struct FailAdvance(LocalFsTransport);

    #[async_trait::async_trait]
    impl GitStateWriter for FailAdvance {
        async fn save_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.save_git(gateway, project, run_id).await
        }
        async fn restore_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.restore_git(gateway, project, run_id).await
        }
        async fn write_git_files(&self, _: &str, _: &[GitWrite]) -> Result<(), CoreError> {
            Err(CoreError::TransportFailed {
                operation: "write".to_string(),
                node: "nodes.master".to_string(),
                detail: "the write was refused".to_string(),
            })
        }
        async fn remove_git_save(&self, g: &str, p: &str, r: &str) -> Result<(), CoreError> {
            self.0.remove_git_save(g, p, r).await
        }
        async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
            self.0.probe_write(project, run_id).await
        }
    }

    #[tokio::test]
    async fn advance_failure_outcome() {
        let rig = Rig::new();
        let host = rig.host();
        let writer = FailAdvance(rig.f.node());
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, Some(&writer))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert!(!result.ok && result.record_written);
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore");
        assert_eq!(restore.reason.as_deref(), Some("advance_failed"));
        assert!(restore.restored && !restore.advanced && restore.save_kept);
        assert!(!rig.save_trees().is_empty(), "the save is kept");
    }

    /// A git writer that lets the restore through, makes the first write of
    /// the advance (the index) and then answers that the second target is no
    /// longer what the run read: what a host's compare-under-lock answers
    /// (27-45, F4).
    struct ChangedAfterIndex(LocalFsTransport);

    #[async_trait::async_trait]
    impl GitStateWriter for ChangedAfterIndex {
        async fn save_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.save_git(gateway, project, run_id).await
        }
        async fn restore_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.restore_git(gateway, project, run_id).await
        }
        async fn write_git_files(
            &self,
            project: &str,
            writes: &[GitWrite],
        ) -> Result<(), CoreError> {
            let index = writes
                .iter()
                .position(|w| matches!(w.name, GitWriteName::Index))
                .expect("the index is written");
            self.0.write_git_files(project, &writes[..=index]).await?;
            let next = writes.get(index + 1).expect("a write follows the index");
            Err(CoreError::TransportFailed {
                operation: GIT_CHANGED_OPERATION.to_string(),
                node: "nodes.master".to_string(),
                detail: format!("{} changed after it was read", next.name.rel()),
            })
        }
        async fn remove_git_save(&self, g: &str, p: &str, r: &str) -> Result<(), CoreError> {
            self.0.remove_git_save(g, p, r).await
        }
        async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
            self.0.probe_write(project, run_id).await
        }
    }

    /// 27-45 (F4): a target that changed under the lock ends the node with the
    /// restore reason `git_changed` under `git_restore_failed` (exit 6, the
    /// existing slug). The save is kept and named, the run's warning names the
    /// changed file and says the index write before it stays, and the event
    /// carries the same word. It is not `advance_failed`.
    #[tokio::test]
    async fn restore_and_advance_maps_a_changed_target_to_the_checkpoint_word() {
        let rig = Rig::new();
        let host = rig.host();
        let writer = ChangedAfterIndex(rig.f.node());
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, Some(&writer))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert!(!result.ok && result.record_written);
        assert_eq!(result.slug.as_deref(), Some(GIT_RESTORE_FAILED_SLUG));
        assert_eq!(result.failure_exit_code(), Some(6));
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore");
        assert_eq!(restore.reason.as_deref(), Some("git_changed"));
        assert!(restore.restored && !restore.advanced && restore.save_kept);
        let save = node.save.as_ref().expect("the save is named");
        assert!(rig.f.data_dir.join(&save.path).is_dir(), "the save is kept");
        assert!(!rig.save_trees().is_empty(), "the save is kept");
        let warning = result
            .warnings
            .iter()
            .find(|w| w.contains("git_changed"))
            .unwrap_or_else(|| panic!("no git_changed warning: {:?}", result.warnings));
        assert!(warning.contains("changed after it was read"), "{warning}");
        assert!(
            warning.contains("index") && warning.contains("in place"),
            "the earlier index write is reported: {warning}"
        );
        assert!(warning.contains(&save.path), "{warning}");
        let events = rig.events(&result);
        let failed = events
            .iter()
            .find(|e| e["event"] == "git_restore_failed")
            .expect("logged");
        assert_eq!(failed["reason"], "git_changed");
    }

    #[tokio::test]
    async fn post_advance_status_mismatch_fails() {
        let extra = "ignition/script-python/cap/extra.txt";
        let rig = Rig::with(|g| g.with_disk_mirror_extra_on_disk(extra));
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert_eq!(result.failure_exit_code(), Some(6));
        assert!(result.record_written);
        let restore = git_of(&result).master().restore.clone().expect("restore");
        assert!(restore.advanced && restore.verified);
        assert_eq!(restore.post_status, Some("mismatch"));
        assert_eq!(restore.reason.as_deref(), Some("post_status_mismatch"));
        assert!(restore.save_kept);
        let note = restore.post_status_note.expect("the paths are named");
        assert!(note.contains(extra) && note.contains("D-25"), "{note}");
        assert!(!rig.save_trees().is_empty(), "the save is kept");
    }

    const VIEW_RES: &str = "com.inductiveautomation.perspective/views/V/resource.json";
    const UTIL2_RES: &str = "ignition/script-python/cap/util2/resource.json";

    /// The live over-count of 2026-11-18 (27-LIVE-CAPTURES.md, `git_prodscale_rig`
    /// after X3). The disk holds release-form bytes for the resources the
    /// release leaves alone, the import rewrites only the resources whose
    /// content changed, and every export renders the gateway's form. The run
    /// predicted every `resource.json` and `project.json` while the disk status
    /// listed only the rewritten ones.
    #[tokio::test]
    async fn unchanged_resources_keep_release_bytes_on_disk() {
        let rig = Rig::with(|g| {
            g.with_disk_mirror_rewrites_changed_only()
                .with_disk_mirror_gateway_form(&[VIEW_RES, UTIL_RES, UTIL2_RES])
        });
        let project = rig.f.project_dir();
        let view_before = std::fs::read(project.join(VIEW_RES)).expect("V descriptor");
        let project_json_before = std::fs::read(project.join("project.json")).expect("manifest");
        let result = rig.apply().await.expect("the run ends with a verdict");
        let restore = git_of(&result).master().restore.clone().expect("restore");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?} {:?} {:?}",
            restore.reason,
            restore.post_status,
            restore.post_status_note
        );
        assert!(result.ok && result.slug.is_none());
        assert!(restore.advanced && restore.verified, "{restore:?}");
        assert_eq!(restore.post_status, Some("matched"));
        // The disk of the unchanged resource and of the manifest is untouched.
        assert_eq!(
            std::fs::read(project.join(VIEW_RES)).expect("V descriptor"),
            view_before,
            "the import left V alone"
        );
        assert_eq!(
            std::fs::read(project.join("project.json")).expect("manifest"),
            project_json_before,
            "the import left project.json alone"
        );
        // What JGit lists is what the run predicted: the changed resource and
        // the new one, never the unchanged ones.
        let drift = git_of(&result)
            .expected_drift
            .clone()
            .expect("drift known after advance");
        assert_eq!(drift, vec![UTIL_RES.to_string(), UTIL2_RES.to_string()]);
        assert!(!drift.iter().any(|p| p == VIEW_RES || p == "project.json"));
        assert_eq!(rig.f.status_paths(), drift);
        assert!(rig.save_trees().is_empty(), "the save is removed");
    }

    /// The case 27-24 measured: every descriptor and the manifest on disk are
    /// already in the gateway's form (the module's clone wrote them so). The
    /// unchanged resource is not rewritten and still differs from the release,
    /// so it is drift, and the run is green. A regression guard: green before and
    /// after the 27-40 fix.
    #[tokio::test]
    async fn gateway_form_disk_predicts_every_rewritten_form() {
        let rig = Rig::with(|g| {
            g.with_disk_mirror_rewrites_changed_only()
                .with_disk_mirror_gateway_form(&[VIEW_RES, UTIL_RES, UTIL2_RES])
        });
        let project = rig.f.project_dir();
        for rel in [VIEW_RES, UTIL_RES] {
            let text = std::fs::read(project.join(rel)).expect("descriptor");
            let value: serde_json::Value = serde_json::from_slice(&text).expect("json");
            let mut pretty = serde_json::to_vec_pretty(&value).expect("pretty");
            pretty.push(b'\n');
            assert_ne!(pretty, text, "{rel} must change when re-serialised");
            std::fs::write(project.join(rel), pretty).expect("the gateway form");
        }
        // The manifest in the form an export carries it (`description` and
        // `parent` added, C8b/C8c), so the unchanged manifest is gateway form.
        std::fs::write(
            project.join("project.json"),
            br#"{"description":"","parent":"","title":"CapApp"}"#,
        )
        .expect("the gateway form of the manifest");
        let result = rig.apply().await.expect("the run ends with a verdict");
        let restore = git_of(&result).master().restore.clone().expect("restore");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?} {:?} {:?}",
            restore.reason,
            restore.post_status,
            restore.post_status_note
        );
        assert_eq!(restore.post_status, Some("matched"));
        let drift = git_of(&result)
            .expected_drift
            .clone()
            .expect("drift known after advance");
        assert_eq!(
            drift,
            vec![
                VIEW_RES.to_string(),
                UTIL_RES.to_string(),
                UTIL2_RES.to_string(),
                "project.json".to_string(),
            ]
        );
        assert_eq!(rig.f.status_paths(), drift);
    }

    /// A git writer that does the advance and then edits one file of the work
    /// tree, as a change between the prediction and the post-advance status.
    struct EditAfterAdvance(LocalFsTransport, PathBuf);

    #[async_trait::async_trait]
    impl GitStateWriter for EditAfterAdvance {
        async fn save_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.save_git(gateway, project, run_id).await
        }
        async fn restore_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.restore_git(gateway, project, run_id).await
        }
        async fn write_git_files(
            &self,
            project: &str,
            writes: &[GitWrite],
        ) -> Result<(), CoreError> {
            self.0.write_git_files(project, writes).await?;
            let mut bytes = std::fs::read(&self.1).expect("the file to edit");
            bytes.extend_from_slice(b" ");
            std::fs::write(&self.1, bytes).expect("the edit");
            Ok(())
        }
        async fn remove_git_save(&self, g: &str, p: &str, r: &str) -> Result<(), CoreError> {
            self.0.remove_git_save(g, p, r).await
        }
        async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
            self.0.probe_write(project, run_id).await
        }
    }

    /// D-25 is not loosened: a file changed on disk after the prediction (here
    /// the unchanged resource the import left alone) still ends the run with
    /// `post_status_mismatch`, one path being enough.
    #[tokio::test]
    async fn post_status_mismatch_still_fails_on_one_extra_path() {
        let rig = Rig::with(|g| {
            g.with_disk_mirror_rewrites_changed_only()
                .with_disk_mirror_gateway_form(&[VIEW_RES, UTIL_RES, UTIL2_RES])
        });
        let host = rig.host();
        let writer = EditAfterAdvance(rig.f.node(), rig.f.project_dir().join(VIEW_RES));
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, Some(&writer))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert_eq!(result.failure_exit_code(), Some(6));
        let restore = git_of(&result).master().restore.clone().expect("restore");
        assert!(restore.advanced && restore.verified, "{restore:?}");
        assert_eq!(restore.post_status, Some("mismatch"));
        assert_eq!(restore.reason.as_deref(), Some("post_status_mismatch"));
        assert!(restore.save_kept);
        let note = restore.post_status_note.expect("the path is named");
        assert!(note.contains(VIEW_RES) && note.contains("1 path"), "{note}");
    }

    #[test]
    fn post_advance_status_not_checked_on_ssh() {
        use ignition_core::actions::deploy_projects::git::{PostStatusMode, RestoreInfo};
        let (word, note) = PostStatusMode::of(None)
            .not_checked_words()
            .expect("an ssh node is not checked");
        let info = RestoreInfo {
            restored: true,
            advanced: true,
            verified: true,
            post_status: Some(word),
            post_status_note: Some(note.to_string()),
            dropped: Vec::new(),
            dropped_count: 0,
            reason: None,
            save_kept: false,
        };
        let json = serde_json::to_value(&info).expect("json");
        assert_eq!(json["post_status"], "not_checked");
        assert!(
            json["post_status_note"]
                .as_str()
                .is_some_and(|n| !n.is_empty())
        );
        assert!(
            PostStatusMode::of(Some(PathBuf::from("/data")))
                .not_checked_words()
                .is_none(),
            "a local node runs the disk status"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn post_advance_status_never_uses_gateway_git_dir() {
        let rig = Rig::new();
        let scratch = tempfile::tempdir().expect("scratch");
        let (program, log) = git_wrapper(scratch.path(), "never-matches-anything");
        let host = rig.host_with_program(&program);
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, None)
            .await
            .expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let calls = std::fs::read_to_string(&log).expect("the log");
        assert!(
            calls.contains("--work-tree="),
            "the disk status ran:\n{calls}"
        );
        let gateway_git = rig.f.git_dir().display().to_string();
        assert!(
            !calls.contains(&gateway_git),
            "host git was pointed at the gateway .git:\n{calls}"
        );
        // The checkout is the git dir of every status.
        assert!(calls.contains("--git-dir="), "{calls}");
    }

    /// A git read whose object fetch fails.
    struct FailingFetch(LocalFsTransport);

    #[async_trait::async_trait]
    impl GitHostRead for FailingFetch {
        async fn project_file_digests(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_advance::ProjectDisk, CoreError> {
            self.0.project_file_digests(project).await
        }
        async fn read_git_state(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_state::GitFiles, CoreError> {
            self.0.read_git_state(project).await
        }
        async fn git_manifest(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.git_manifest(project).await
        }
        async fn save_manifest(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.save_manifest(gateway, project, run_id).await
        }
        async fn git_file_digest(
            &self,
            project: &str,
            name: &GitWriteName,
        ) -> Result<Option<String>, CoreError> {
            self.0.git_file_digest(project, name).await
        }
        async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
            self.0.read_git_yaml().await
        }
        async fn project_dir_entry(
            &self,
            project: &str,
            dir: ignition_core::deploy::transport::projects::ImportDir,
        ) -> Result<ignition_core::deploy::transport::projects::GitEntry, CoreError> {
            self.0.project_dir_entry(project, dir).await
        }
        async fn free_bytes(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
            self.0.free_bytes(gateway, project).await
        }
        async fn git_size(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitSize, CoreError> {
            self.0.git_size(project).await
        }
        async fn list_git_objects(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_objects::GitObjectList, CoreError> {
            self.0.list_git_objects(project).await
        }
        async fn fetch_git_objects(
            &self,
            _project: &str,
            _files: &[ignition_core::deploy::git_objects::GitObjectFile],
            _dest: &std::path::Path,
        ) -> Result<(), CoreError> {
            Err(CoreError::TransportFailed {
                operation: "read".to_string(),
                node: "nodes.master".to_string(),
                detail: "the object read was refused".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn gateway_objects_failure_refuses() {
        let rig = Rig::new();
        rig.f.designer_commit(false);
        rig.sync_live();
        let failing = StubHost {
            inner: rig.f.node(),
            token: None,
            program: None,
            read: Some(Box::new(FailingFetch(rig.f.node()))),
        };
        // `--check` and the apply refuse before any write, before the gates
        // are mapped.
        for _ in 0..2 {
            let err = refusal(rig.plan_with(&failing).await);
            assert_eq!(preflight_checks(&err), ["git_gateway_objects"], "{err}");
        }
        assert!(rig.gateway.import_calls().is_empty());
        assert_eq!(rig.run_dirs(), 0);

        // With a working read the same fixture refuses `git_unpushed`, and the
        // prepared report's dirty list came from the gateway's objects.
        use ignition_core::actions::deploy_projects::git::{prepare_git_host, prepare_git_live};
        use ignition_core::deploy::projects::ProjectAction;
        let host = rig.host();
        let input = rig.input();
        let loaded = load_project(&input).expect("loads");
        let surfaces = ProjectsSurfaces {
            gateway: &rig.gateway,
            host: &host,
            tree: None,
            apply_host: None,
            deploy: None,
            tag_delete: TagDeleteSurface::NotConfigured { missing: vec![] },
        };
        let mut prepared = prepare_git_host(&loaded, &input, &surfaces, GitEntry::Directory)
            .await
            .expect("the host half")
            .expect("a managed repo");
        let zip = ignition_core::actions::deploy_projects::ProjectGateway::export_bytes(
            &rig.gateway,
            PROJECT,
        )
        .await
        .expect("export");
        let live = canonical_digest(&zip).expect("digest");
        let mut warnings = Vec::new();
        let err = prepare_git_live(
            &mut prepared,
            Some(&live),
            ProjectAction::Import,
            &loaded,
            &surfaces,
            &mut warnings,
        )
        .await
        .expect_err("unpushed refuses");
        assert_eq!(gate_of(&err).0, "git_unpushed");
        let node = prepared.report.master();
        assert_eq!(node.dirty, Some(Vec::new()));
        assert_eq!(node.dirty_source, Some("gateway_objects"));
    }

    // ---- 27-19: the write preflight (D-22, D-G1) ----

    /// A git read that answers like the local transport, records which
    /// operations were asked for, and answers `free_bytes` with a number of
    /// the test's choosing.
    struct Metered {
        inner: LocalFsTransport,
        free: Option<u64>,
        calls: Calls,
    }

    type Calls = std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>;

    impl Metered {
        fn log(&self, op: &'static str) {
            self.calls.lock().expect("calls").push(op);
        }
    }

    #[async_trait::async_trait]
    impl GitHostRead for Metered {
        async fn project_file_digests(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_advance::ProjectDisk, CoreError> {
            self.inner.project_file_digests(project).await
        }
        async fn read_git_state(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_state::GitFiles, CoreError> {
            self.log("read_git_state");
            self.inner.read_git_state(project).await
        }
        async fn git_manifest(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.log("git_manifest");
            self.inner.git_manifest(project).await
        }
        async fn save_manifest(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.log("save_manifest");
            self.inner.save_manifest(gateway, project, run_id).await
        }
        async fn git_file_digest(
            &self,
            project: &str,
            name: &GitWriteName,
        ) -> Result<Option<String>, CoreError> {
            self.log("git_file_digest");
            self.inner.git_file_digest(project, name).await
        }
        async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
            self.log("read_git_yaml");
            self.inner.read_git_yaml().await
        }
        async fn project_dir_entry(
            &self,
            project: &str,
            dir: ignition_core::deploy::transport::projects::ImportDir,
        ) -> Result<ignition_core::deploy::transport::projects::GitEntry, CoreError> {
            self.log("project_dir_entry");
            self.inner.project_dir_entry(project, dir).await
        }
        async fn free_bytes(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
            self.log("free_bytes");
            match self.free {
                Some(free) => Ok(free),
                None => self.inner.free_bytes(gateway, project).await,
            }
        }
        async fn git_size(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitSize, CoreError> {
            self.log("git_size");
            self.inner.git_size(project).await
        }
        async fn list_git_objects(
            &self,
            project: &str,
        ) -> Result<ignition_core::deploy::git_objects::GitObjectList, CoreError> {
            self.log("list_git_objects");
            self.inner.list_git_objects(project).await
        }
        async fn fetch_git_objects(
            &self,
            project: &str,
            files: &[ignition_core::deploy::git_objects::GitObjectFile],
            dest: &std::path::Path,
        ) -> Result<(), CoreError> {
            self.log("fetch_git_objects");
            self.inner.fetch_git_objects(project, files, dest).await
        }
    }

    /// A host that answers `free_bytes` with `free` (the local answer when
    /// `None`) and records every git read it serves.
    fn metered(rig: &Rig, free: Option<u64>) -> (StubHost, Calls) {
        let calls = Calls::default();
        let host = StubHost {
            inner: rig.f.node(),
            token: None,
            program: None,
            read: Some(Box::new(Metered {
                inner: rig.f.node(),
                free,
                calls: calls.clone(),
            })),
        };
        (host, calls)
    }

    /// The failures of a `deploy_preflight_failed`, as `(check, detail)`.
    fn preflight_details(err: &CoreError) -> Vec<(String, String)> {
        match err {
            CoreError::DeployPreflightFailed { failures } => failures
                .iter()
                .map(|f| (f.check.clone(), f.detail.clone()))
                .collect(),
            other => panic!("expected deploy_preflight_failed, got {other:?}"),
        }
    }

    /// Whether this account ignores mode bits (root): the denial rows cannot
    /// be set up then.
    #[cfg(unix)]
    fn ignores_modes(dir: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        let probe = dir.join("ign-mode-probe");
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let writable = std::fs::write(&probe, b"x").is_ok();
        let _ = std::fs::remove_file(&probe);
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        writable
    }

    /// What a D-22 write-preflight test does when the account may ignore
    /// mode bits (WR-D6).
    #[cfg(unix)]
    #[derive(Debug, PartialEq, Eq)]
    enum RootGuard {
        /// The account honours mode bits: run the assertions.
        Run,
        /// The account ignores mode bits and the operator said so on
        /// purpose: print a countable line and return.
        Skip,
        /// The account ignores mode bits and nobody said so: fail.
        Refuse,
    }

    /// The pure rule: `ignores` is `ignores_modes`, `allow` is the value of
    /// `IGN_ALLOW_ROOT_SKIP`. Only the exact value `1` allows the skip.
    #[cfg(unix)]
    fn root_guard_decision(ignores: bool, allow: Option<&str>) -> RootGuard {
        match (ignores, allow) {
            (false, _) => RootGuard::Run,
            (true, Some("1")) => RootGuard::Skip,
            (true, _) => RootGuard::Refuse,
        }
    }

    /// WR-D6: the D-22 write-preflight tests never pass vacuously as root.
    /// Returns true when the test must return now (a deliberate skip, with a
    /// `SKIPPED-AS-ROOT <test>` line); panics when the account ignores mode
    /// bits and `IGN_ALLOW_ROOT_SKIP=1` is not set.
    #[cfg(unix)]
    fn root_guard(test: &str, dir: &std::path::Path) -> bool {
        let allow = std::env::var("IGN_ALLOW_ROOT_SKIP").ok();
        match root_guard_decision(ignores_modes(dir), allow.as_deref()) {
            RootGuard::Run => false,
            RootGuard::Skip => {
                println!("SKIPPED-AS-ROOT {test}");
                true
            }
            RootGuard::Refuse => panic!(
                "{test}: run as a non-root user (or set IGN_ALLOW_ROOT_SKIP=1 to skip on purpose)"
            ),
        }
    }

    /// WR-D6: the guard rule itself, testable as any user.
    #[cfg(unix)]
    #[test]
    fn root_guard_rule() {
        assert_eq!(root_guard_decision(false, None), RootGuard::Run);
        assert_eq!(root_guard_decision(false, Some("1")), RootGuard::Run);
        assert_eq!(root_guard_decision(true, None), RootGuard::Refuse);
        assert_eq!(root_guard_decision(true, Some("0")), RootGuard::Refuse);
        assert_eq!(root_guard_decision(true, Some("")), RootGuard::Refuse);
        assert_eq!(root_guard_decision(true, Some("yes")), RootGuard::Refuse);
        assert_eq!(root_guard_decision(true, Some("1")), RootGuard::Skip);
    }

    /// No probe file is left in the project directory or in `.git`.
    fn assert_no_probe_left(rig: &Rig) {
        for dir in [rig.f.project_dir(), rig.f.git_dir()] {
            let left: Vec<String> = std::fs::read_dir(&dir)
                .expect("read_dir")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(".ign-probe-"))
                .collect();
            assert!(left.is_empty(), "{left:?} left in {}", dir.display());
        }
    }

    /// D-22: a project directory the deploy account cannot write refuses
    /// before the run directory exists, naming the directory; nothing is
    /// imported, saved or left behind.
    #[cfg(unix)]
    #[tokio::test]
    async fn write_preflight_refuses_no_run_dir() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::new();
        if root_guard("write_preflight_refuses_no_run_dir", &rig.f.project_dir()) {
            return;
        }
        let host = rig.host();
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        std::fs::set_permissions(rig.f.project_dir(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        let _unlock = Unlock(rig.f.project_dir());
        let err = refusal(rig.run_prepared(&host, &loaded, prepared, None).await);
        let failures = preflight_details(&err);
        assert_eq!(failures.len(), 1, "{err}");
        assert_eq!(failures[0].0, "git_write_access", "{err}");
        assert!(
            failures[0].1.contains("projects/CapApp"),
            "{}",
            failures[0].1
        );
        assert!(
            !failures[0].1.contains("projects/CapApp/.git"),
            "the project directory is named, not .git: {}",
            failures[0].1
        );
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert!(!rig.repo.join(".ign").exists(), "no .ign directory");
        assert!(rig.gateway.import_calls().is_empty(), "nothing imported");
        assert!(rig.save_trees().is_empty(), "nothing saved");
    }

    /// A writable project directory over a read-only `.git` names `.git`.
    #[cfg(unix)]
    #[tokio::test]
    async fn write_preflight_git_dir() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::new();
        if root_guard("write_preflight_git_dir", &rig.f.project_dir()) {
            return;
        }
        let host = rig.host();
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        std::fs::set_permissions(rig.f.git_dir(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        let _unlock = Unlock(rig.f.git_dir());
        let err = refusal(rig.run_prepared(&host, &loaded, prepared, None).await);
        let failures = preflight_details(&err);
        assert_eq!(failures[0].0, "git_write_access", "{err}");
        assert!(
            failures[0].1.contains("projects/CapApp/.git"),
            "{}",
            failures[0].1
        );
        assert_eq!(rig.run_dirs(), 0);
        assert!(rig.gateway.import_calls().is_empty());
        assert_no_probe_left(&rig);
    }

    /// `--check` builds no writer and never probes: a project directory the
    /// apply refuses for is checked clean, and no size or free space is read.
    #[cfg(unix)]
    #[tokio::test]
    async fn preflight_not_in_check() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::new();
        if root_guard("preflight_not_in_check", &rig.f.project_dir()) {
            return;
        }
        std::fs::set_permissions(rig.f.project_dir(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        let _unlock = Unlock(rig.f.project_dir());
        let (host, calls) = metered(&rig, Some(0));
        let (_, prepared) = rig.plan_with(&host).await.expect("the plan");
        let result = prepared.into_check_result();
        assert_eq!(result.outcome, ProjectOutcome::Checked);
        let asked = calls.lock().expect("calls").clone();
        assert!(
            !asked.contains(&"free_bytes") && !asked.contains(&"git_size"),
            "the check read no free space and no size: {asked:?}"
        );
        assert_eq!(rig.run_dirs(), 0);
        assert_no_probe_left(&rig);
    }

    /// Free space below the need refuses with both numbers, before the run
    /// directory and before any import.
    #[tokio::test]
    async fn free_space_refuses() {
        let rig = Rig::new();
        let (host, _calls) = metered(&rig, Some(1_048_576));
        let err = refusal(rig.apply_with(&host).await);
        let failures = preflight_details(&err);
        assert_eq!(failures.len(), 1, "{err}");
        assert_eq!(failures[0].0, "git_free_space", "{err}");
        let detail = &failures[0].1;
        assert!(detail.contains("1048576"), "free bytes named: {detail}");
        let git = rig.f.node().git_size(PROJECT).await.expect("size").bytes;
        assert!(
            detail.contains(&format!("2 x {git} bytes")),
            "the .git size is named: {detail}"
        );
        assert!(
            detail.contains(" are needed"),
            "the need is named: {detail}"
        );
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert!(rig.gateway.import_calls().is_empty());
        assert!(rig.save_trees().is_empty());
        assert_no_probe_left(&rig);
    }

    /// The need counts the prepared pack: exactly `2 x .git + 64 MiB` is not
    /// enough when the prepare holds a pack, and the full need passes.
    #[tokio::test]
    async fn free_space_counts_pack() {
        let rig = Rig::new();
        let git = rig.f.node().git_size(PROJECT).await.expect("size").bytes;
        let floor = 2 * git + 64 * 1024 * 1024;
        let (host, _) = metered(&rig, Some(floor));
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let ProjectsPrepared::Ready {
            git: Some(held), ..
        } = &prepared
        else {
            panic!("a managed plan is ready");
        };
        let pack = held
            .prep
            .as_ref()
            .and_then(|p| p.pack.as_ref())
            .map_or(0, |p| (p.pack.len() + p.idx.len()) as u64);
        assert!(pack > 0, "the release needs a pack the gateway lacks");
        let err = refusal(rig.run_prepared(&host, &loaded, prepared, None).await);
        let failures = preflight_details(&err);
        assert_eq!(failures[0].0, "git_free_space", "{err}");
        assert!(
            failures[0].1.contains(&(floor + pack).to_string()),
            "the need is 2 x .git + pack + 64 MiB = {}: {}",
            floor + pack,
            failures[0].1
        );
        assert_eq!(rig.run_dirs(), 0);

        // The full need is enough.
        let (host, _) = metered(&rig, Some(floor + pack));
        let result = rig.apply_with(&host).await.expect("enough space");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
    }

    /// W-5: the preflight reads the size and the free space and never the
    /// manifest (which hashes every non-object file).
    #[tokio::test]
    async fn preflight_size_only() {
        let rig = Rig::new();
        let (host, calls) = metered(&rig, None);
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let planned = calls.lock().expect("calls").len();
        let result = rig
            .run_prepared(&host, &loaded, prepared, None)
            .await
            .expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let asked = calls.lock().expect("calls").clone();
        let during = &asked[planned..];
        let at = |op: &str| during.iter().position(|c| *c == op);
        let size = at("git_size").expect("the preflight read the size");
        let free = at("free_bytes").expect("the preflight read the free space");
        // The run's own first read is the state, re-read right before the
        // save: the preflight's reads all come before it.
        let first_state = at("read_git_state").expect("the run re-reads the state");
        assert!(size < first_state && free < first_state, "{during:?}");
        // No manifest was computed before the preflight ended: the manifest
        // and the save manifest belong to the save and the restore.
        for op in ["git_manifest", "save_manifest"] {
            if let Some(index) = at(op) {
                assert!(index > first_state, "{op} ran inside the preflight");
            }
        }
        assert_eq!(
            during.iter().filter(|c| **c == "git_size").count(),
            1,
            "{during:?}"
        );
        assert_no_probe_left(&rig);
    }

    // ---- 27-13: events, run.json, the record advance, K11, the Designer gate ----

    /// `--designers wait` for `timeout_ms`, polling every 5 ms.
    fn wait_ms(timeout_ms: u64) -> DesignersPolicy {
        DesignersPolicy {
            mode: DesignersMode::Wait,
            timeout: std::time::Duration::from_millis(timeout_ms),
            interval: std::time::Duration::from_millis(5),
        }
    }

    /// `--designers fail` (the default mode) with a fast poll, so the gate's
    /// 30 s wait is only ever spent on the polls a test scripts.
    fn fail_fast_poll() -> DesignersPolicy {
        DesignersPolicy {
            mode: DesignersMode::Fail,
            timeout: std::time::Duration::from_secs(600),
            interval: std::time::Duration::from_millis(5),
        }
    }

    fn run_json(rig: &Rig, result: &DeployProjectsResult) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(rig.run_path(result).join("run.json")).expect("run"))
            .expect("run.json parses")
    }

    #[tokio::test]
    async fn git_events_in_order() {
        let rig = Rig::new();
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert_eq!(
            rig.git_events(&result),
            [
                "git_state_read",
                "git_saved",
                "git_restored",
                "git_advanced",
                "git_save_removed"
            ]
        );
        let events = rig.events(&result);
        let named = |name: &str| {
            events
                .iter()
                .find(|e| e["event"] == name)
                .unwrap_or_else(|| panic!("{name} is logged"))
                .clone()
        };
        let read = named("git_state_read");
        assert_eq!(read["node"], "nodes.master");
        assert_eq!(read["state"], "managed");
        assert_eq!(read["head"], rig.f.c1.as_str());
        assert_eq!(named("git_advanced")["release_commit"], rig.f.c2.as_str());
        let saved = named("git_saved");
        assert!(
            saved["path"]
                .as_str()
                .is_some_and(|p| p.contains("git-save"))
        );

        // Names, counts, ids and the save path only: no repo file content and
        // no URL userinfo reach the log (T-27-63).
        let log =
            std::fs::read_to_string(rig.run_path(&result).join("events.ndjson")).expect("events");
        for (path, bytes) in members_of(&rig.f.project_dir()) {
            let text = String::from_utf8_lossy(&bytes).trim().to_string();
            if text.len() > 8 {
                assert!(!log.contains(&text), "{path} content is in the event log");
            }
        }
        assert!(!log.contains("@"), "no userinfo shaped text: {log}");
        let json = serde_json::to_string(&result).expect("result json");
        assert!(
            !json.contains("password") && !json.contains("token"),
            "{json}"
        );
    }

    #[tokio::test]
    async fn run_json_records_git() {
        let rig = Rig::new();
        let result = rig.apply().await.expect("the apply completes");
        let run = run_json(&rig, &result);
        assert_eq!(run["git"]["state"], "managed");
        assert_eq!(run["git"]["head"], rig.f.c1.as_str());
        assert_eq!(run["git"]["release_commit"], rig.f.c2.as_str());
        assert_eq!(
            run["git"].as_object().map(|o| o.len()),
            Some(3),
            "state, head and release_commit only: {run}"
        );

        // A project without `.git` has `git: null`.
        let rig = Rig::new();
        std::fs::remove_dir_all(rig.f.git_dir()).expect("no .git");
        let result = rig.apply().await.expect("the apply completes");
        assert!(run_json(&rig, &result)["git"].is_null());
        assert!(rig.git_events(&result).is_empty());
    }

    #[tokio::test]
    async fn git_restore_failed_event() {
        let rig = Rig::new();
        let host = rig.host();
        let writer = FailAdvance(rig.f.node());
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, Some(&writer))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert_eq!(
            rig.git_events(&result),
            [
                "git_state_read",
                "git_saved",
                "git_restored",
                "git_restore_failed"
            ]
        );
        let events = rig.events(&result);
        let failed = events
            .iter()
            .find(|e| e["event"] == "git_restore_failed")
            .expect("logged");
        assert_eq!(failed["reason"], "advance_failed");
        let save = git_of(&result).master().save.clone().expect("save named");
        assert_eq!(failed["path"], save.path.as_str());
        let last = events.last().expect("events");
        assert_eq!(last["event"], "finished");
        assert_eq!(last["outcome"], "git_restore_failed");
    }

    #[tokio::test]
    async fn record_action_advances_in_place() {
        let rig = Rig::new();
        rig.make_live_equal_artifact().await;
        let result = rig.apply().await.expect("the record completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Recorded,
            "{:?} {:?}",
            result.warnings,
            result.git.as_ref().map(|g| g.master().restore.clone())
        );
        assert!(result.ok && result.record_written);
        assert!(rig.gateway.import_calls().is_empty(), "no import");
        let git = git_of(&result);
        let restore = git.master().restore.as_ref().expect("restore reported");
        assert!(!restore.restored, "no restore: the .git was never removed");
        assert!(restore.advanced && restore.verified);
        assert_eq!(restore.post_status, Some("matched"));
        assert!(!restore.save_kept);
        assert!(git.expected_drift_complete);
        let drift = git.expected_drift.clone().expect("drift after the advance");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        assert_eq!(rig.f.status_paths(), drift);
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert_eq!(
            rig.git_events(&result),
            [
                "git_state_read",
                "git_saved",
                "git_advanced",
                "git_save_removed"
            ]
        );
    }

    #[tokio::test]
    async fn record_refuses_unpushed_commits() {
        // A record moves the branch to the release: commits no remote has
        // would be orphaned, so it refuses like an import (27-13).
        let rig = Rig::new();
        rig.f.designer_commit(false);
        rig.make_live_equal_artifact().await;
        let err = refusal(rig.apply().await);
        assert_eq!(gate_of(&err).0, "git_unpushed");
        assert_eq!(rig.run_dirs(), 0);
        assert!(rig.save_trees().is_empty());
    }

    #[tokio::test]
    async fn unverified_restores_unchanged() {
        let rig = Rig::new();
        rig.gateway.mutate_child_after_import(PROJECT, 1000);
        let before = rig.git_manifest().await;
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::ImportedUnverified);
        assert!(!result.ok && !result.record_written);
        assert_eq!(result.failure_exit_code(), Some(6));
        let git = git_of(&result);
        let restore = git.master().restore.as_ref().expect("restore reported");
        assert!(restore.restored && !restore.advanced, "{restore:?}");
        assert_eq!(restore.reason, None);
        assert!(
            !restore.save_kept,
            "the save equals the repo and is removed"
        );
        assert_eq!(rig.git_manifest().await, before, ".git is as it was saved");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c1);
        assert!(rig.save_trees().is_empty());
        assert!(!git.expected_drift_complete && git.expected_drift.is_none());
        assert_eq!(
            rig.git_events(&result),
            [
                "git_state_read",
                "git_saved",
                "git_restored",
                "git_save_removed"
            ]
        );
    }

    #[tokio::test]
    async fn k11_git_warning_names_the_git_dir() {
        // WR-06: the printed restore command is a plain overwrite import, and
        // an import removes the gateway's .git again. The run says so, and
        // that the repository must be copied aside first and put back after.
        let rig = Rig::new();
        rig.gateway.mutate_child_after_import(PROJECT, 1000);
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::ImportedUnverified);
        let command = result
            .restore_command
            .as_deref()
            .expect("an overwrite has a restore command");
        assert!(
            command.starts_with("ign project import ")
                && command.contains("--collision-policy overwrite")
                && !command.contains('\n')
                && !command.contains("&&"),
            "the command keeps its single-command form: {command}"
        );
        let warning = result
            .warnings
            .iter()
            .find(|w| w.contains(&format!("data/projects/{PROJECT}/.git")))
            .unwrap_or_else(|| panic!("a warning names the .git: {:?}", result.warnings));
        assert!(
            warning.contains("removes") && warning.contains("copy"),
            "{warning}"
        );
        assert!(
            warning.contains("aside") && warning.contains("put the copy back"),
            "{warning}"
        );
    }

    /// CodeRabbit #57: a Designer that attaches while the `.git` is being
    /// saved stops the run at the check made after the saves, right before
    /// the import: nothing is imported, the save goes (the `.git` was never
    /// touched) and the repository is as it was.
    #[tokio::test]
    async fn designer_attached_during_the_save_stops_the_import() {
        let session = ("alice".to_string(), PROJECT.to_string(), "ws-1".to_string());
        let rig = Rig::with(|g| g.with_designers_script(vec![vec![], vec![session]]));
        let head = rig.f.git_in_project(&["rev-parse", "HEAD"]);
        let outcome = rig.apply().await;
        assert!(
            rig.gateway.import_calls().is_empty(),
            "no import: {outcome:?}"
        );
        let err = outcome.expect_err("the Designer check refuses");
        assert!(err.to_string().contains("alice"), "{err}");
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), head);
    }

    /// CodeRabbit #57 round 2: under `--designers wait` a Designer that
    /// commits in the gateway repo while attached, after the saves, and then
    /// closes, would have that commit removed by the import (the save
    /// predates it). The repository is read again after the wait: a change
    /// since the plan refuses `plan_stale` before the import, the commit
    /// stays and the save goes.
    #[tokio::test]
    async fn designer_commit_during_the_wait_refuses_a_stale_plan() {
        let session = ("alice".to_string(), PROJECT.to_string(), "ws-1".to_string());
        let rig = Rig::with(|g| g.with_designers_script(vec![vec![], vec![session], vec![]]));
        let project = rig.f.project_dir();
        rig.gateway.on_designers_read(1, move || {
            common::git_fixture::git(
                &project,
                &[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "designer while attached",
                ],
            );
        });
        let outcome = rig.apply_designers(wait_ms(5_000)).await;
        assert!(
            rig.gateway.import_calls().is_empty(),
            "no import: {outcome:?}"
        );
        let err = outcome.expect_err("a stale plan");
        assert_eq!(err.code(), "plan_stale", "{err}");
        assert_eq!(
            rig.f.git_in_project(&["log", "-1", "--format=%s"]),
            "designer while attached",
            "the Designer's commit is still there"
        );
        assert!(rig.save_trees().is_empty(), "the save is removed");
    }

    /// CodeRabbit #57 R1: under `--designers wait` a Designer that saves
    /// project work while attached, after the saves, and then closes, leaves
    /// HEAD, refs and config as they were, but the import would replace the
    /// saved work, which the pre-image predates. The live project is read
    /// again when a session was seen at the last check: a digest that differs
    /// from the plan's is `plan_stale` before the import, and the save goes.
    #[tokio::test]
    async fn designer_save_during_the_wait_refuses_a_stale_plan() {
        let session = ("alice".to_string(), PROJECT.to_string(), "ws-1".to_string());
        let rig = Rig::with(|g| {
            g.with_designers_script(vec![vec![], vec![session], vec![]])
                .with_project_edited_on_designers_read(1, PROJECT)
        });
        // CodeRabbit #57 S2: the read that reports Alice comes after the save
        let saves = rig.f.data_dir.join("ign-deploy");
        let saved_at_poll = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = saved_at_poll.clone();
        rig.gateway.on_designers_read(1, move || {
            let taken = std::fs::read_dir(saves.join("capgw/projects/CapApp/git-save"))
                .map(|runs| runs.flatten().any(|run| run.path().join("repo").is_dir()))
                .unwrap_or(false);
            seen.store(taken, std::sync::atomic::Ordering::SeqCst);
        });
        let outcome = rig.apply_designers(wait_ms(5_000)).await;
        assert!(
            rig.gateway.import_calls().is_empty(),
            "no import: {outcome:?}"
        );
        let err = outcome.expect_err("a stale plan");
        assert_eq!(err.code(), "plan_stale", "{err}");
        assert!(
            saved_at_poll.load(std::sync::atomic::Ordering::SeqCst),
            "the save was taken before the poll that reported the session"
        );
        // and the run logged the save, then its removal
        let run = std::fs::read_dir(rig.repo.join(".ign/deploy/capgw"))
            .expect("run dirs")
            .flatten()
            .next()
            .expect("one run directory")
            .path();
        let events: Vec<String> = std::fs::read_to_string(run.join("events.ndjson"))
            .expect("events")
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter_map(|e| e["event"].as_str().map(str::to_string))
            .filter(|e| e == "git_saved" || e == "git_save_removed")
            .collect();
        assert_eq!(events, ["git_saved", "git_save_removed"], "{events:?}");
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert!(rig.f.git_dir().is_dir(), "the .git was never touched");
    }

    /// CodeRabbit #57 S1: a Designer that opens, saves project work and
    /// closes between two Designer polls is never seen as a session, and a
    /// project save leaves `.git` as it was. The live project is read again
    /// on every overwrite, after the repo checks and right before the import:
    /// a digest that differs from the plan's is `plan_stale`, and the save goes.
    #[tokio::test]
    async fn designer_save_between_polls_refuses_a_stale_plan() {
        let rig = Rig::with(|g| {
            g.with_designers_script(vec![vec![]])
                .with_project_edited_on_designers_read(1, PROJECT)
        });
        let outcome = rig.apply().await;
        assert!(
            rig.gateway.import_calls().is_empty(),
            "no import: {outcome:?}"
        );
        let err = outcome.expect_err("a stale plan");
        assert_eq!(err.code(), "plan_stale", "{err}");
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert!(rig.f.git_dir().is_dir(), "the .git was never touched");
    }

    #[tokio::test]
    async fn designer_attached_at_restore() {
        // the session opens after the import's read-back (the third export):
        // one open before the import is refused by the check after the saves
        let rig = Rig::with(|g| g.with_designers_after_exports(3, 100_000));
        let result = rig
            .apply_designers(wait_ms(60))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert!(!result.ok);
        assert_eq!(result.slug.as_deref(), Some(GIT_RESTORE_FAILED_SLUG));
        assert_eq!(result.failure_exit_code(), Some(6));
        assert!(result.record_written, "the deploy and its record stay");
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore reported");
        assert_eq!(restore.reason.as_deref(), Some("designer_attached"));
        assert!(!restore.restored && !restore.advanced && restore.save_kept);
        let save = node.save.as_ref().expect("the save is named");
        assert!(rig.f.data_dir.join(&save.path).is_dir(), "the save is kept");
        assert!(
            !rig.f.git_dir().exists(),
            "nothing was written to .git: it is still absent"
        );
        let warning = result
            .warnings
            .iter()
            .find(|w| w.contains("designer_attached"))
            .expect("the refusal is reported");
        assert!(
            warning.contains("dana") && warning.contains(&save.path),
            "{warning}"
        );
        assert!(
            rig.git_events(&result)
                .contains(&"git_restore_failed".to_string())
        );
        assert!(
            !rig.git_events(&result)
                .contains(&"git_advanced".to_string())
        );
        // The gate polled under the run's own `--designers wait` timeout.
        let polls = rig
            .events(&result)
            .iter()
            .filter(|e| e["event"] == "designers_poll")
            .count();
        assert!(polls >= 2, "the gate waited and polled: {polls}");
    }

    #[tokio::test]
    async fn designer_detaches_within_wait() {
        // The run's mode is `fail`: the gate still waits (30 s at most, 5 ms
        // between reads here) and the session closes after two reads.
        let rig = Rig::with(|g| g.with_designers_after_exports(3, 2));
        let result = rig
            .apply_designers(fail_fast_poll())
            .await
            .expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert!(result.ok);
        let restore = git_of(&result).master().restore.clone().expect("restore");
        assert!(restore.restored && restore.advanced && restore.reason.is_none());
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        let opens: Vec<u64> = rig
            .events(&result)
            .iter()
            .filter(|e| e["event"] == "designers_poll")
            .filter_map(|e| e["open"].as_u64())
            .collect();
        // One read at the pre-check (fail mode, no poll event of its own is
        // logged for it by `wait`), then the gate's three reads.
        assert!(opens.ends_with(&[1, 1, 0]), "{opens:?}");
    }

    /// D-08: a tag-owning managed project whose project import reads back
    /// different is rolled back; the saved pre-run `.git` is put back
    /// unchanged and never advanced.
    #[tokio::test]
    async fn rollback_restores_git_unchanged() {
        let rig = Rig::tagged(true);
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let before = rig.git_manifest().await;
        let result = rig.apply().await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok);
        assert_eq!(result.failure_exit_code(), Some(6));
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore reported");
        assert!(restore.restored && !restore.advanced, "{restore:?}");
        assert_eq!(restore.reason, None);
        assert!(!restore.save_kept, "the save equals the repo and goes");
        assert_eq!(rig.git_manifest().await, before, ".git is as it was saved");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c1);
        assert!(rig.save_trees().is_empty());
        assert!(!git_of(&result).expected_drift_complete);
        assert_eq!(
            rig.git_events(&result),
            [
                "git_state_read",
                "git_saved",
                "git_restored",
                "git_save_removed"
            ]
        );
    }

    /// R-6: the project import did not remove `.git` (the module does not
    /// manage the directory), so the rollback writes nothing to it.
    #[tokio::test]
    async fn rollback_git_still_present_left_alone() {
        let rig = Rig::tagged(false);
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let before = rig.git_manifest().await;
        let result = rig.apply().await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        let restore = git_of(&result)
            .master()
            .restore
            .clone()
            .expect("restore reported");
        assert!(!restore.restored && !restore.advanced, "{restore:?}");
        assert!(!restore.save_kept);
        assert_eq!(rig.git_manifest().await, before, "nothing was written");
        assert!(rig.save_trees().is_empty());
        assert!(
            !rig.git_events(&result)
                .contains(&"git_restored".to_string())
        );
    }

    /// The project import is refused, nothing was applied: `.git` is intact,
    /// the save is removed, nothing is restored.
    #[tokio::test]
    async fn rollback_import_refused_git_intact() {
        let rig = Rig::tagged_with(true, |g| g.with_project_import_denied());
        let before = rig.git_manifest().await;
        let result = rig.apply().await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        let restore = git_of(&result)
            .master()
            .restore
            .clone()
            .expect("restore reported");
        assert!(!restore.restored && !restore.advanced && !restore.save_kept);
        assert_eq!(rig.git_manifest().await, before);
        assert!(rig.save_trees().is_empty());
    }

    /// D-07, checker W-4: the restore of a rollback passes the Designer gate.
    /// A session that stays attached leaves `.git` absent and the save kept,
    /// and the rollback's `left` list names the save path with
    /// `designer_attached`; a session that closes within the wait lets the
    /// restore proceed.
    #[tokio::test]
    async fn rollback_waits_for_designer() {
        let rig = Rig::tagged_with(true, |g| g.with_designers_after_exports(3, 100_000));
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let err = refusal(rig.apply_designers(wait_ms(60)).await);
        let CoreError::RollbackFailed(failure) = &err else {
            panic!("the rollback must end rollback_failed: {err:?}");
        };
        assert_eq!(err.code(), "rollback_failed");
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save is kept");
        let save = trees[0]
            .strip_prefix(&rig.f.data_dir)
            .expect("below the data dir")
            .to_string_lossy()
            // ign names a host path with `/` on every platform
            .replace('\\', "/");
        assert!(
            failure.cause.contains("designer_attached") && failure.cause.contains(&save),
            "the left list names the save path and the reason: {}",
            failure.cause
        );
        assert!(!rig.f.git_dir().exists(), ".git is still absent");
        // The run directory's events record the failed restore.
        let run_dir = std::fs::read_dir(rig.repo.join(".ign/deploy/capgw"))
            .expect("run dirs")
            .next()
            .expect("one run")
            .expect("entry")
            .path();
        let events = std::fs::read_to_string(run_dir.join("events.ndjson")).expect("events");
        assert!(events.contains("git_restore_failed"), "{events}");
        assert!(!events.contains("git_save_removed"), "{events}");

        // The session closes within the wait: the restore proceeds.
        let rig = Rig::tagged_with(true, |g| g.with_designers_after_exports(3, 2));
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let before = rig.git_manifest().await;
        let result = rig
            .apply_designers(wait_ms(2_000))
            .await
            .expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        let restore = git_of(&result).master().restore.clone().expect("restore");
        assert!(restore.restored && restore.reason.is_none(), "{restore:?}");
        assert_eq!(rig.git_manifest().await, before);
    }

    /// `--rollback-run` no longer refuses a managed `.git`: it saves it,
    /// re-imports the pre-image and puts the repository back unchanged.
    #[tokio::test]
    async fn rollback_run_restores_git_unchanged() {
        let rig = Rig::tagged(true);
        let applied = rig.apply().await.expect("green");
        assert_eq!(
            applied.outcome,
            ProjectOutcome::Applied,
            "{:?}",
            applied.warnings
        );
        let run_id = applied.run_id.clone().expect("a run id");
        let before = rig.git_manifest().await;
        let imports = rig.gateway.import_calls().len();

        let undo = rig.rollback_run(&run_id, false).await.expect("a verdict");
        assert_eq!(
            undo.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            undo.warnings
        );
        assert_eq!(rig.gateway.import_calls().len(), imports + 1, "pre-image");
        assert_eq!(rig.git_manifest().await, before, ".git is unchanged");
        let node = git_of(&undo).master();
        let restore = node.restore.as_ref().expect("restore reported");
        assert!(restore.restored && !restore.advanced, "{restore:?}");
        assert!(!restore.save_kept);
        assert!(rig.save_trees().is_empty());
        assert_eq!(
            rig.git_events(&undo),
            ["git_saved", "git_restored", "git_save_removed"]
        );
        // The check reports the repository without writing anything.
        let checked = rig.rollback_run(&run_id, true).await.expect("a check");
        assert_eq!(git_of(&checked).master().state, "managed");
        assert!(git_of(&checked).master().save.is_none());
        assert!(rig.save_trees().is_empty());
    }

    /// A shape the deploy cannot hand over still refuses `project_git_managed`
    /// with its reason, before anything is written.
    #[tokio::test]
    async fn rollback_run_unsupported_refuses() {
        let rig = Rig::tagged(true);
        let applied = rig.apply().await.expect("green");
        let run_id = applied.run_id.clone().expect("a run id");
        rig.f.git_in_project(&["checkout", "-q", "--detach"]);
        let before = rig.git_manifest().await;
        let imports = rig.gateway.import_calls().len();
        for check in [true, false] {
            let err = refusal(rig.rollback_run(&run_id, check).await);
            assert_eq!(reason_of(&err), "detached_head", "{err}");
        }
        assert_eq!(rig.gateway.import_calls().len(), imports, "nothing written");
        assert_eq!(rig.git_manifest().await, before);
        assert!(rig.save_trees().is_empty());
    }

    /// Pitfall 1, D-07, D-09: a Designer opened while the import had removed
    /// `.git` and the module recreated one. It is never overwritten: the run
    /// ends `git_restore_failed` with reason `git_recreated`, the recreated
    /// repository byte-identical afterwards, the save kept and named.
    #[tokio::test]
    async fn designer_recreated_git_during_window() {
        let rig = Rig::with(|g| g.with_disk_mirror_recreates_git());
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok && result.record_written, "the deploy stays");
        assert_eq!(result.failure_exit_code(), Some(6));
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore reported");
        assert_eq!(restore.reason.as_deref(), Some("git_recreated"));
        assert!(!restore.advanced && restore.save_kept, "{restore:?}");
        let save = node.save.as_ref().expect("the save is named");
        assert!(rig.f.data_dir.join(&save.path).is_dir(), "the save is kept");
        // The recreated repository is the module's partial shape and is
        // untouched: no branch ref, no index, still the unborn master.
        let dot_git = rig.f.git_dir();
        assert!(dot_git.is_dir());
        assert!(!dot_git.join("index").exists(), "nothing was advanced");
        assert_eq!(
            rig.f.git_in_project(&["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );
        assert_eq!(rig.f.git_in_project(&["for-each-ref", "refs/heads"]), "");
        let warning = result
            .warnings
            .iter()
            .find(|w| w.contains("git_recreated"))
            .expect("the refusal is reported");
        assert!(
            warning.contains(&save.path) && warning.contains("left untouched"),
            "{warning}"
        );
        let events = rig.git_events(&result);
        assert!(events.contains(&"git_restore_failed".to_string()));
        assert!(!events.contains(&"git_advanced".to_string()));
        assert!(!events.contains(&"git_save_removed".to_string()));
    }

    /// D-08 and the prohibition "no path overwrites a `.git` that differs from
    /// the save": a repository the module recreated during the rollback's own
    /// import is left exactly as it is, the save is kept and named, and the
    /// rollback ends `rollback_failed` naming it.
    #[tokio::test]
    async fn rollback_recreated_git_is_left_alone() {
        let rig = Rig::tagged_with(true, |g| g.with_disk_mirror_recreates_git());
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let err = refusal(rig.apply().await);
        let CoreError::RollbackFailed(failure) = &err else {
            panic!("the rollback must end rollback_failed: {err:?}");
        };
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save is kept");
        let save = trees[0]
            .strip_prefix(&rig.f.data_dir)
            .expect("below the data dir")
            .to_string_lossy()
            // ign names a host path with `/` on every platform
            .replace('\\', "/");
        assert!(
            failure.cause.contains(&save) && failure.cause.contains("not the one saved"),
            "{}",
            failure.cause
        );
        let dot_git = rig.f.git_dir();
        assert!(dot_git.is_dir() && !dot_git.join("index").exists());
        assert_eq!(
            rig.f.git_in_project(&["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );
        assert_eq!(rig.f.git_in_project(&["for-each-ref", "refs/heads"]), "");
    }

    #[tokio::test]
    async fn post_import_paths_wait_for_designer() {
        // The record action: the advance is refused while a session is
        // attached, and `.git` stays exactly as it was.
        let rig = Rig::with(|g| g.with_designers_after_verify(100_000));
        rig.make_live_equal_artifact().await;
        let before = rig.git_manifest().await;
        let result = rig
            .apply_designers(wait_ms(60))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::GitRestoreFailed);
        assert_eq!(result.failure_exit_code(), Some(6));
        assert!(result.record_written);
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore reported");
        assert_eq!(restore.reason.as_deref(), Some("designer_attached"));
        assert!(!restore.advanced && restore.save_kept);
        assert_eq!(rig.git_manifest().await, before, ".git is unchanged");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c1);
        let save = node.save.as_ref().expect("the save is named");
        assert!(rig.f.data_dir.join(&save.path).is_dir(), "the save is kept");

        // The unverified import: the outcome stays `imported_unverified`, the
        // repo is not put back while a session is attached.
        let rig = Rig::with(|g| g.with_designers_after_exports(3, 100_000));
        rig.gateway.mutate_child_after_import(PROJECT, 1000);
        let result = rig
            .apply_designers(wait_ms(60))
            .await
            .expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::ImportedUnverified);
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore reported");
        assert_eq!(restore.reason.as_deref(), Some("designer_attached"));
        assert!(!restore.restored && restore.save_kept);
        assert!(!rig.f.git_dir().exists(), ".git is still absent");
        assert!(!rig.save_trees().is_empty(), "the save is kept");
    }

    #[test]
    fn restore_gate_policy_follows_the_run() {
        use ignition_core::actions::deploy_projects::git::{
            GIT_RESTORE_DESIGNER_WAIT_S, restore_policy,
        };
        use ignition_core::deploy::apply::preflight::DESIGNERS_POLL_S;
        let default = restore_policy(&DesignersPolicy::default());
        assert_eq!(default.mode, DesignersMode::Wait);
        assert_eq!(
            default.timeout,
            std::time::Duration::from_secs(GIT_RESTORE_DESIGNER_WAIT_S)
        );
        assert_eq!(GIT_RESTORE_DESIGNER_WAIT_S, 30);
        assert_eq!(
            default.interval,
            std::time::Duration::from_secs(DESIGNERS_POLL_S)
        );
        let proceed = restore_policy(&DesignersPolicy {
            mode: DesignersMode::Proceed,
            ..DesignersPolicy::default()
        });
        assert_eq!(proceed.mode, DesignersMode::Wait, "whatever the run used");
        assert_eq!(proceed.timeout.as_secs(), GIT_RESTORE_DESIGNER_WAIT_S);
        let waiting = restore_policy(&wait_ms(1234));
        assert_eq!(waiting.timeout, std::time::Duration::from_millis(1234));
    }

    // ---- 27-15: `discard_gateway_changes` (D-05) ----

    const VIEW_MEMBER: &str = "com.inductiveautomation.perspective/views/V/view.json";
    const REASON: &str = "the break/fix on the gateway is abandoned (ticket 42)";

    /// A rig whose run gives `--discard-gateway-changes`.
    fn discarding() -> Rig {
        let mut rig = Rig::new();
        rig.discard = Some(REASON.to_string());
        rig
    }

    /// The gateway repo's refs, as `git for-each-ref` prints them.
    fn refs_of(rig: &Rig) -> String {
        rig.f
            .git_in_project(&["for-each-ref", "--format=%(refname) %(objectname)"])
    }

    /// The discarded block, which must be present.
    fn discarded_of(result: &DeployProjectsResult) -> &DiscardedReport {
        git_of(result)
            .discarded
            .as_ref()
            .expect("data.git.discarded is present")
    }

    use ignition_core::actions::deploy_projects::git::{DiscardDirty, DiscardedReport};

    fn discarded_json(rig: &Rig, result: &DeployProjectsResult) -> serde_json::Value {
        serde_json::from_slice(
            &std::fs::read(rig.run_path(result).join("git/discarded.json"))
                .expect("git/discarded.json exists"),
        )
        .expect("discarded.json parses")
    }

    #[tokio::test]
    async fn check_would_discard() {
        // Without the override the gate still refuses.
        let rig = Rig::new();
        let d = rig.f.designer_commit(false);
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        let err = refusal(rig.check().await);
        assert_eq!(gate_of(&err).0, "git_unpushed");

        // With it, --check reports what would go and writes nothing.
        let mut rig = rig;
        rig.discard = Some(REASON.to_string());
        let before = rig.git_manifest().await;
        let result = rig.check().await.expect("the check completes");
        assert_eq!(result.outcome, ProjectOutcome::Checked);
        let git = git_of(&result);
        let would = git.would_discard.as_ref().expect("would_discard");
        assert_eq!(would.dirty, DiscardDirty::Listed);
        assert_eq!(would.paths, [VIEW_MEMBER.to_string()]);
        assert_eq!(would.paths_count, 1);
        assert_eq!(would.commits, [d]);
        assert_eq!(would.evidence, "export/CapApp.zip");
        assert!(git.discarded.is_none());
        let json = serde_json::to_value(git).expect("json");
        assert_eq!(json["would_discard"]["dirty"], "listed");
        assert!(json["discarded"].is_null());
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert!(rig.save_trees().is_empty());
        assert_eq!(rig.git_manifest().await, before, ".git is byte-identical");
        assert!(rig.gateway.import_calls().is_empty());
    }

    /// B5: an unpushed gateway HEAD is unknown to ign's checkout. The pack is
    /// built in the prepare from the ids ign's store holds, the repo ends at
    /// the release, and the abandoned HEAD stays reachable.
    #[tokio::test]
    async fn override_unpushed_advances_and_keeps_head() {
        let rig = discarding();
        let d = rig.f.designer_commit(false);
        rig.sync_live();
        let host = rig.host();
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let ProjectsPrepared::Ready { git, .. } = &prepared else {
            panic!("a changed project is Ready");
        };
        let bases = &git
            .as_ref()
            .and_then(|g| g.prep.as_ref())
            .expect("the advance was prepared")
            .pack_bases;
        assert!(!bases.contains(&d), "the unpushed HEAD is never a base");
        assert!(bases.contains(&rig.f.c1), "{bases:?}");

        let result = rig
            .run_prepared(&host, &loaded, prepared, None)
            .await
            .expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert!(result.ok);
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        rig.f.git_in_project(&["fsck", "--connectivity-only"]);
        let run_id = result.run_id.clone().expect("run id");
        assert!(
            refs_of(&rig).contains(&format!("refs/ign-discarded/{run_id} {d}")),
            "{}",
            refs_of(&rig)
        );
        rig.f.git_in_project(&["cat-file", "-e", &d]);
        assert!(!rig.save_trees().is_empty(), "the save is kept");
        let node = git_of(&result).master();
        let restore = node.restore.as_ref().expect("restore");
        assert!(restore.advanced && restore.verified && restore.save_kept);
        let discarded = discarded_of(&result);
        assert_eq!(discarded.commits, std::slice::from_ref(&d));
        assert_eq!(
            discarded.keep_ref.as_deref(),
            Some(format!("refs/ign-discarded/{run_id}").as_str())
        );
        assert!(
            git_of(&result).would_discard.is_none(),
            "the result says what happened"
        );
    }

    #[tokio::test]
    async fn override_not_merged_lists_commits() {
        let rig = discarding();
        let b = rig.f.breakfix_on_production();
        rig.sync_live();
        let check = rig.check().await.expect("the check completes");
        let would = git_of(&check)
            .would_discard
            .as_ref()
            .expect("would_discard");
        assert_eq!(would.commits, std::slice::from_ref(&b));
        assert_eq!(would.dirty, DiscardDirty::None);
        assert!(would.paths.is_empty() && would.paths_count == 0);

        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        let discarded = discarded_of(&result);
        assert_eq!(discarded.commits, std::slice::from_ref(&b));
        assert_eq!(discarded.dirty, DiscardDirty::None);
        let run_id = result.run_id.clone().expect("run id");
        assert!(
            refs_of(&rig).contains(&format!("refs/ign-discarded/{run_id} {b}")),
            "{}",
            refs_of(&rig)
        );
        rig.f.git_in_project(&["cat-file", "-e", &b]);
        assert!(!rig.save_trees().is_empty(), "the save is kept");
    }

    #[tokio::test]
    async fn override_does_not_pass_release_not_on_branch() {
        let mut rig = discarding();
        rig.f.designer_commit(false);
        rig.f.tag_release_on("feature", "v1.2.0");
        rig.release = "v1.2.0".to_string();
        rig.sync_live();
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "release_not_on_branch");
    }

    #[tokio::test]
    async fn override_does_not_pass_project_git_managed() {
        let rig = discarding();
        rig.f.git_in_project(&["checkout", "-q", "--detach"]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "detached_head");
    }

    #[tokio::test]
    async fn override_does_not_pass_a_preflight_failure() {
        let mut rig = discarding();
        rig.f.designer_commit(false);
        rig.sync_live();
        let shallow = rig.f.shallow_checkout();
        put_file(
            &shallow,
            "deploy/project.yaml",
            &format!("schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\n"),
        );
        put_file(&shallow, ".gitignore", ".ign/\n");
        rig.repo = shallow;
        let err = refused_alike(&rig).await;
        assert_eq!(preflight_checks(&err), ["git_checkout"]);
    }

    #[tokio::test]
    async fn clean_run_has_no_discard() {
        let rig = discarding();
        let check = rig.check().await.expect("the check completes");
        assert!(git_of(&check).would_discard.is_none());
        assert!(git_of(&check).discarded.is_none());

        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let git = git_of(&result);
        assert!(git.would_discard.is_none() && git.discarded.is_none());
        let json = serde_json::to_value(git).expect("json");
        assert!(json["would_discard"].is_null() && json["discarded"].is_null());
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert!(!git.master().restore.as_ref().expect("restore").save_kept);
        assert!(
            !refs_of(&rig).contains("ign-discarded"),
            "{}",
            refs_of(&rig)
        );
        assert!(
            !rig.run_path(&result).join("git/discarded.json").exists(),
            "no evidence of nothing"
        );
        assert!(
            !rig.git_events(&result)
                .contains(&"git_discarded".to_string())
        );
    }

    /// D-24: an unpushed Designer commit that ign's store lacks, plus an
    /// uncommitted live change: the dirty paths are listed from the gateway's
    /// own objects.
    #[tokio::test]
    async fn override_unpushed_dirty_listed() {
        let rig = discarding();
        let d = rig
            .f
            .designer_commit_changing(CODE, "def f():\n    return 7\n", false);
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();

        let check = rig.check().await.expect("the check completes");
        let git = git_of(&check);
        let would = git.would_discard.as_ref().expect("would_discard");
        assert_eq!(would.dirty, DiscardDirty::Listed);
        assert_eq!(would.paths, [VIEW_MEMBER.to_string()]);
        assert_eq!(would.paths_count, 1);
        assert_eq!(would.commits, std::slice::from_ref(&d));
        assert_eq!(would.evidence, "export/CapApp.zip");
        let node = git.master();
        assert_eq!(
            node.dirty.as_deref(),
            Some([VIEW_MEMBER.to_string()].as_slice())
        );
        assert_eq!(node.dirty_source, Some("gateway_objects"));

        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        let discarded = discarded_of(&result);
        assert_eq!(discarded.dirty, DiscardDirty::Listed);
        assert_eq!(discarded.paths, [VIEW_MEMBER.to_string()]);
        assert_eq!(
            serde_json::to_value(discarded.dirty).expect("json"),
            "listed"
        );

        // Task 2: the evidence names the same paths and the pre-image holds
        // the live content.
        let evidence = discarded_json(&rig, &result);
        assert_eq!(evidence["dirty"], "listed");
        assert_eq!(evidence["paths"], serde_json::json!([VIEW_MEMBER]));
        assert_eq!(evidence["paths_count"], 1);
        assert_eq!(evidence["pre_image"], "export/CapApp.zip");
        assert_eq!(evidence["commits"], serde_json::json!([d]));
        let pre = std::fs::read(rig.run_path(&result).join("export/CapApp.zip"))
            .expect("the pre-image export");
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(pre)).expect("the pre-image is a zip");
        let mut member = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name(VIEW_MEMBER).expect("the changed member"),
            &mut member,
        )
        .expect("member text");
        assert!(member.contains("\"live\":true"), "{member}");
    }

    // ---- 27-15 task 2: evidence, run.json, event and hints ----

    #[tokio::test]
    async fn override_passes_dirty() {
        let rig = discarding();
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        let discarded = discarded_of(&result);
        assert_eq!(discarded.reason, REASON);
        assert_eq!(discarded.dirty, DiscardDirty::Listed);
        assert_eq!(discarded.paths, [VIEW_MEMBER.to_string()]);
        assert_eq!(discarded.paths_count, 1);
        assert!(discarded.commits.is_empty() && discarded.keep_ref.is_none());
        assert_eq!(discarded.evidence, "export/CapApp.zip");

        let path = rig.run_path(&result).join("git/discarded.json");
        let evidence = discarded_json(&rig, &result);
        assert_eq!(evidence["node"], "nodes.master");
        assert_eq!(evidence["reason"], REASON);
        assert_eq!(evidence["branch"], BRANCH);
        assert_eq!(evidence["abandoned_head"], rig.f.c1.as_str());
        assert_eq!(evidence["dirty"], "listed");
        assert_eq!(evidence["paths"], serde_json::json!([VIEW_MEMBER]));
        assert_eq!(evidence["commits"], serde_json::json!([]));
        assert_eq!(evidence["pre_image"], "export/CapApp.zip");
        let save_path = evidence["save_path"].as_str().expect("save path");
        assert!(save_path.contains("git-save"), "{save_path}");
        // Ids, paths and the reason: no file content and no commit message.
        let text = std::fs::read_to_string(&path).expect("evidence text");
        assert!(
            !text.contains("\"live\"") && !text.contains("designer") && !text.contains("init"),
            "{text}"
        );
        let mut keys: Vec<&str> = evidence
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "abandoned_head",
                "branch",
                "commits",
                "dirty",
                "node",
                "paths",
                "paths_count",
                "pre_image",
                "reason",
                "save_path"
            ]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &std::path::Path| {
                std::fs::metadata(p).expect("meta").permissions().mode() & 0o777
            };
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().expect("git dir")), 0o700);
            assert_eq!(
                mode(&rig.run_path(&result).join("export/CapApp.zip")),
                0o600
            );
        }
        // Dirty work alone abandons no commit: the save is removed.
        assert!(rig.save_trees().is_empty());
        assert!(!refs_of(&rig).contains("ign-discarded"));
    }

    #[tokio::test]
    async fn override_evidence_before_save() {
        let rig = discarding();
        rig.f.designer_commit(false);
        rig.sync_live();
        let result = rig.apply().await.expect("the apply completes");
        let events = rig.events(&result);
        let position = |name: &str| {
            events
                .iter()
                .position(|e| e["event"] == name)
                .unwrap_or_else(|| panic!("{name} is logged"))
        };
        assert!(position("pre_image_exported") < position("git_discarded"));
        assert!(position("git_discarded") < position("git_saved"));
        assert!(position("git_discarded") < position("import_sent"));
        let event = &events[position("git_discarded")];
        assert_eq!(event["evidence"], "git/discarded.json");
        assert_eq!(event["commits"], 1);
        assert!(
            rig.run_path(&result)
                .join(event["evidence"].as_str().expect("path"))
                .exists()
        );
        // The reason is operator text: it is in run.json and the evidence,
        // never in an event.
        let log =
            std::fs::read_to_string(rig.run_path(&result).join("events.ndjson")).expect("events");
        assert!(!log.contains(REASON), "{log}");
        assert_eq!(
            rig.git_events(&result),
            [
                "git_state_read",
                "git_discarded",
                "git_saved",
                "git_restored",
                "git_advanced"
            ]
        );
    }

    #[tokio::test]
    async fn override_recorded_in_run_json() {
        let rig = discarding();
        rig.f.designer_commit(false);
        rig.sync_live();
        let result = rig.apply().await.expect("the apply completes");
        let run = run_json(&rig, &result);
        assert_eq!(run["discard_gateway_changes"], REASON);
        assert_eq!(run["git"].as_object().map(|o| o.len()), Some(3));

        // Without the override the key is null.
        let plain = Rig::new();
        let result = plain.apply().await.expect("the apply completes");
        assert!(run_json(&plain, &result)["discard_gateway_changes"].is_null());
    }

    /// The reason is data: whatever it holds is stored as a JSON string and
    /// reaches no shell, no event and no message.
    #[tokio::test]
    async fn override_reason_is_data() {
        let mut rig = Rig::new();
        let reason =
            "x\"; touch /tmp/ign-27-15-pwned; $(touch /tmp/ign-27-15-pwned) `id` \u{1b}[31m\nline2";
        rig.discard = Some(reason.to_string());
        rig.f.designer_commit(false);
        rig.sync_live();
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(discarded_of(&result).reason, reason);
        assert_eq!(discarded_json(&rig, &result)["reason"], reason);
        assert_eq!(run_json(&rig, &result)["discard_gateway_changes"], reason);
        let log =
            std::fs::read_to_string(rig.run_path(&result).join("events.ndjson")).expect("events");
        assert!(!log.contains("pwned") && !log.contains('\u{1b}'), "{log}");
        for warning in &result.warnings {
            assert!(!warning.contains("pwned"), "{warning}");
        }
        assert!(!std::path::Path::new("/tmp/ign-27-15-pwned").exists());
    }

    #[tokio::test]
    async fn override_record_keeps_commits_reachable() {
        let rig = discarding();
        let d = rig.f.designer_commit(false);
        rig.make_live_equal_artifact().await;
        let result = rig.apply().await.expect("the record completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Recorded,
            "{:?}",
            result.warnings
        );
        assert!(rig.gateway.import_calls().is_empty(), "no import");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        let discarded = discarded_of(&result);
        assert_eq!(discarded.commits, std::slice::from_ref(&d));
        assert_eq!(discarded.dirty, DiscardDirty::None);
        let run_id = result.run_id.clone().expect("run id");
        assert!(refs_of(&rig).contains(&format!("refs/ign-discarded/{run_id} {d}")));
        rig.f.git_in_project(&["fsck", "--connectivity-only"]);
        assert!(!rig.save_trees().is_empty(), "the save is kept");
        assert!(rig.run_path(&result).join("git/discarded.json").exists());
    }

    /// K11 with the override: the import replaced the live content (the
    /// pre-image has it) but the repo is put back as saved, so no commit was
    /// abandoned, no keep-alive ref exists and nothing was discarded: the
    /// result keeps `would_discard` and carries no `discarded` block (27-35,
    /// WR-11, D-05). The evidence file was written before the import.
    #[tokio::test]
    async fn discarded_absent_after_unverified() {
        let rig = discarding();
        let d = rig.f.designer_commit(false);
        rig.sync_live();
        rig.gateway.mutate_child_after_import(PROJECT, 1000);
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(result.outcome, ProjectOutcome::ImportedUnverified);
        let git = git_of(&result);
        assert!(
            git.discarded.is_none(),
            "nothing was discarded: {:?}",
            git.discarded
        );
        let would = git.would_discard.as_ref().expect("would_discard stays");
        assert_eq!(would.commits, std::slice::from_ref(&d));
        assert!(
            !refs_of(&rig).contains("ign-discarded"),
            "{}",
            refs_of(&rig)
        );
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), d);
        assert!(rig.run_path(&result).join("git/discarded.json").exists());
        assert!(rig.run_path(&result).join("export/CapApp.zip").exists());
        let json = serde_json::to_value(git).expect("json");
        assert!(json["discarded"].is_null(), "{json}");
        assert!(!json["would_discard"].is_null(), "{json}");
    }

    /// WR-11: a run that fails after a tag write and rolls back put the
    /// project and the repository back, so `data.git.discarded` is null and
    /// `would_discard` is still there. `git/discarded.json` was written before
    /// the import and stays.
    #[tokio::test]
    async fn discarded_absent_after_rollback() {
        let mut rig = Rig::tagged(true);
        rig.discard = Some(REASON.to_string());
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let result = rig.apply().await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        let git = git_of(&result);
        assert!(
            git.discarded.is_none(),
            "a rolled-back run discarded nothing: {:?}",
            git.discarded
        );
        let would = git.would_discard.as_ref().expect("would_discard stays");
        assert_eq!(would.paths, [VIEW_MEMBER.to_string()]);
        let json = serde_json::to_value(git).expect("json");
        assert!(json["discarded"].is_null(), "{json}");
        assert_eq!(json["would_discard"]["dirty"], "listed", "{json}");
        // The evidence was written before the import and stays.
        let evidence = discarded_json(&rig, &result);
        assert_eq!(evidence["reason"], REASON);
        assert_eq!(evidence["paths"], serde_json::json!([VIEW_MEMBER]));
        assert!(
            rig.git_events(&result)
                .contains(&"git_discarded".to_string())
        );
        assert!(!refs_of(&rig).contains("ign-discarded"));
    }

    /// WR-11, the unchanged half: a green override run reports `discarded`
    /// with the commits and the keep-alive ref the advance wrote, and no
    /// `would_discard`.
    #[tokio::test]
    async fn discarded_after_advance() {
        let rig = discarding();
        let d = rig.f.designer_commit(false);
        rig.sync_live();
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        let git = git_of(&result);
        assert!(git.would_discard.is_none(), "{:?}", git.would_discard);
        let discarded = discarded_of(&result);
        assert_eq!(discarded.commits, std::slice::from_ref(&d));
        let run_id = result.run_id.clone().expect("run id");
        assert_eq!(
            discarded.keep_ref.as_deref(),
            Some(format!("refs/ign-discarded/{run_id}").as_str())
        );
        assert_eq!(discarded.reason, REASON);
    }

    #[tokio::test]
    async fn git_gate_hint_names_override() {
        let rig = Rig::new();
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        let err = refusal(rig.check().await);
        assert_eq!(gate_of(&err).0, "git_dirty");
        for gate in ["git_unpushed", "git_not_merged", "git_dirty"] {
            let err = CoreError::DestructiveChangeRefused {
                gate: gate.to_string(),
                keys: vec!["a".to_string()],
                count: 1,
            };
            let text = err.to_string();
            let hint = err.hint().expect("hint");
            assert!(
                text.contains("--discard-gateway-changes <reason>"),
                "{text}"
            );
            assert!(hint.contains("--discard-gateway-changes"), "{hint}");
            for kept in [
                "pre-image",
                "refs/ign-discarded/<run-id>",
                "saved repository",
            ] {
                assert!(hint.contains(kept), "{gate}: {kept} is not named: {hint}");
            }
            assert!(hint.contains("push and merge it"), "{hint}");
        }
        // The gate no override passes keeps its own wording.
        let err = CoreError::DestructiveChangeRefused {
            gate: "downgrade".to_string(),
            keys: vec!["P".to_string()],
            count: 1,
        };
        assert!(!err.to_string().contains("--discard-gateway-changes"));
    }

    // ---- 27-17: adopting the partial repo `setupLocalRepo` leaves ----

    const ADOPT_BRANCH: &str = "production";

    /// A rig whose gateway repo is the G6 partial shape.
    fn adopting() -> Rig {
        let rig = Rig::new();
        rig.f.partial_setup_local_repo();
        rig
    }

    /// The target file gains `git_branch: <branch>`.
    fn set_target_branch(rig: &Rig, branch: &str) {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(rig.targets.path().join("targets/dev.yaml"))
            .expect("the target file");
        writeln!(file, "git_branch: {branch}").expect("append");
    }

    fn files_below(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files_below(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    /// The dummy credential of `git.yaml` is in no envelope field, event
    /// line, run-directory file or warning (T-27-37).
    fn assert_secret_free(rig: &Rig, result: &DeployProjectsResult) {
        use common::git_fixture::GIT_YAML_SECRET;
        let mut texts = vec![
            serde_json::to_string(result).expect("json"),
            format!("{result:?}"),
            result.warnings.join("\n"),
        ];
        let mut files = Vec::new();
        files_below(&rig.repo.join(".ign"), &mut files);
        assert!(!files.is_empty(), "the run left files to scan");
        for file in files {
            texts.push(String::from_utf8_lossy(&std::fs::read(&file).expect("read")).into_owned());
        }
        for text in texts {
            assert!(!text.contains(GIT_YAML_SECRET), "the credential leaked");
            assert!(!text.contains("dummy-user"), "the user name leaked");
        }
    }

    #[tokio::test]
    async fn adopt_from_git_yaml() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert!(result.ok && result.slug.is_none());
        let git = git_of(&result);
        let node = git.master();
        assert_eq!(node.state, "adoptable");
        assert!(node.adopted);
        assert_eq!(node.branch.as_deref(), Some(ADOPT_BRANCH));
        assert_eq!(node.branch_source, Some("git_yaml"));
        assert_eq!(node.remote.as_deref(), Some("origin"));
        assert_eq!(node.head, None, "an unborn HEAD has no commit");
        let restore = node.restore.as_ref().expect("restore reported");
        assert!(restore.advanced && restore.verified && restore.reason.is_none());
        assert_eq!(restore.post_status, Some("matched"));
        assert!(!restore.save_kept);
        let drift = git.expected_drift.clone().expect("drift known after adopt");
        assert!(git.expected_drift_complete);

        assert_eq!(
            rig.f.git_in_project(&["symbolic-ref", "HEAD"]),
            format!("refs/heads/{ADOPT_BRANCH}")
        );
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        assert_eq!(
            rig.f
                .git_in_project(&["rev-parse", &format!("refs/remotes/origin/{ADOPT_BRANCH}")]),
            rig.f.c2
        );
        assert_eq!(
            rig.f
                .git_in_project(&["config", &format!("branch.{ADOPT_BRANCH}.merge")]),
            format!("refs/heads/{ADOPT_BRANCH}")
        );
        assert_eq!(
            rig.f
                .git_in_project(&["config", &format!("branch.{ADOPT_BRANCH}.remote")]),
            "origin"
        );
        // What the module left in the config stays.
        assert_eq!(rig.f.git_in_project(&["config", "http.sslVerify"]), "false");
        assert_eq!(rig.f.status_paths(), drift);
        rig.f.git_in_project(&["fsck", "--connectivity-only"]);
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert!(
            rig.git_events(&result)
                .contains(&"git_advanced".to_string())
        );

        let json = serde_json::to_value(git).expect("json");
        assert_eq!(json["nodes"][0]["adopted"], true);
        assert_eq!(json["nodes"][0]["branch_source"], "git_yaml");
        assert!(json["nodes"][0]["dirty"].is_null());
        assert_secret_free(&rig, &result);
    }

    /// No git.yaml: the target's `git_branch` names the branch. The repo here
    /// holds objects only up to C1, so the adoption sends the release as a
    /// pack and the repo still passes fsck.
    #[tokio::test]
    async fn adopt_from_target_key() {
        let rig = Rig::new();
        rig.f.partial_behind();
        set_target_branch(&rig, ADOPT_BRANCH);
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let node = git_of(&result).master();
        assert_eq!(node.state, "adoptable");
        assert!(node.adopted);
        assert_eq!(node.branch.as_deref(), Some(ADOPT_BRANCH));
        assert_eq!(node.branch_source, Some("target"));
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        rig.f.git_in_project(&["fsck", "--connectivity-only"]);
        assert_eq!(
            rig.f
                .git_in_project(&["config", &format!("branch.{ADOPT_BRANCH}.remote")]),
            "origin"
        );
    }

    /// git.yaml first, then the target key.
    #[tokio::test]
    async fn adopt_branch_source_order() {
        // git.yaml wins over the target key.
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        set_target_branch(&rig, "main");
        let result = rig.check().await.expect("check");
        let node = git_of(&result).master();
        assert_eq!(node.branch.as_deref(), Some(ADOPT_BRANCH));
        assert_eq!(node.branch_source, Some("git_yaml"));

        // An entry without repo_branch, or for another project, falls through.
        for entries in [
            vec![(PROJECT, None)],
            vec![("OtherProject", Some("main"))],
            vec![(PROJECT, Some("main")), (PROJECT, Some(ADOPT_BRANCH))],
        ] {
            let rig = adopting();
            rig.f.with_git_yaml(&entries);
            set_target_branch(&rig, ADOPT_BRANCH);
            let result = rig.check().await.expect("check");
            let node = git_of(&result).master();
            assert_eq!(node.branch.as_deref(), Some(ADOPT_BRANCH), "{entries:?}");
            assert_eq!(node.branch_source, Some("target"), "{entries:?}");
        }
    }

    #[tokio::test]
    async fn adopt_branch_required() {
        let rig = adopting();
        let before = rig.git_manifest().await;
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "branch_required");
        assert_eq!(rig.git_manifest().await, before, ".git is untouched");
        let hint = err.hint().expect("a hint");
        assert!(
            hint.contains("repo_branch") && hint.contains("git_branch"),
            "{hint}"
        );

        // A git.yaml that does not name a branch for this project is no source.
        for entries in [
            vec![(PROJECT, None)],
            vec![("OtherProject", Some(ADOPT_BRANCH))],
        ] {
            let rig = adopting();
            rig.f.with_git_yaml(&entries);
            let err = refused_alike(&rig).await;
            assert_eq!(reason_of(&err), "branch_required", "{entries:?}");
            assert!(
                !err.to_string()
                    .contains(common::git_fixture::GIT_YAML_SECRET)
            );
        }
    }

    /// WR-A4 (27-26): a git.yaml the adoption cannot read is a preflight
    /// failure with the check word `git_yaml` (exit 6), never a damaged run
    /// directory. It names `git.yaml` and a line, and echoes no file value.
    #[tokio::test]
    async fn adopt_malformed_git_yaml_refuses_preflight() {
        use common::git_fixture::GIT_YAML_SECRET;
        for body in [
            format!("- ignition_projectName: [{GIT_YAML_SECRET}\n  repo_branch: x\n"),
            format!("repo_branch: {GIT_YAML_SECRET}\n"),
        ] {
            let rig = adopting();
            put_file(&rig.f.data_dir, "git.yaml", &body);
            let before = rig.git_manifest().await;
            let err = refused_alike(&rig).await;
            assert_eq!(err.code(), "deploy_preflight_failed", "{err}");
            assert_eq!(err.exit_code(), 6);
            assert_eq!(preflight_checks(&err), ["git_yaml"]);
            let text = err.to_string();
            assert!(text.contains("git.yaml"), "{text}");
            assert!(!text.contains(GIT_YAML_SECRET), "{text}");
            assert!(!text.contains("run unknown"), "{text}");
            let hint = err.hint().unwrap_or_default();
            assert!(!hint.contains("run directory"), "{hint}");
            assert_eq!(rig.git_manifest().await, before, ".git is untouched");
        }
        // A line is named for YAML that does not parse.
        let rig = adopting();
        put_file(&rig.f.data_dir, "git.yaml", "- a: [b\n");
        let text = refusal(rig.check().await).to_string();
        assert!(text.contains("line"), "{text}");

        // The host read refuses a git.yaml that is a symlink or over the cap
        // the same way: the same check word, not a damaged run directory.
        #[cfg(unix)]
        {
            let rig = adopting();
            put_file(&rig.f.data_dir, "real.yaml", "- ignition_projectName: A\n");
            std::os::unix::fs::symlink("real.yaml", rig.f.data_dir.join("git.yaml"))
                .expect("symlink");
            let err = refused_alike(&rig).await;
            assert_eq!(err.code(), "deploy_preflight_failed", "{err}");
            assert_eq!(preflight_checks(&err), ["git_yaml"]);
            assert!(err.to_string().contains("git.yaml"), "{err}");
        }
        let rig = adopting();
        put_file(
            &rig.f.data_dir,
            "git.yaml",
            &"#".repeat(ignition_core::deploy::yamlio::YAML_MAX_BYTES + 1),
        );
        let err = refused_alike(&rig).await;
        assert_eq!(preflight_checks(&err), ["git_yaml"], "{err}");
    }

    /// JGit's default `master` in the unborn HEAD is never taken as intent,
    /// even when the remote has a `master`.
    #[tokio::test]
    async fn adopt_never_takes_the_unborn_head_name() {
        let rig = adopting();
        rig.f.push_branch_without_release("master");
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "branch_required");
    }

    #[tokio::test]
    async fn adopt_release_not_on_branch() {
        let rig = adopting();
        // `main` holds the first commit only; the release is on production.
        rig.f.with_git_yaml(&[(PROJECT, Some("main"))]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "release_not_on_branch");

        // A branch the remote does not have holds nothing.
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some("no-such-branch"))]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "release_not_on_branch");
    }

    #[tokio::test]
    async fn adopt_remote_mismatch() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.f
            .set_gateway_remote_url("https://git.example/other/elsewhere.git");
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "remote_mismatch");
    }

    /// D-13: the partial repo holds no commit, so a live project that differs
    /// from every commit still adopts and no dirty, unpushed or not-merged
    /// fact is reported.
    #[tokio::test]
    async fn adopt_skips_dirty_and_unpushed() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let node = git_of(&result).master();
        assert!(node.adopted);
        assert_eq!(node.dirty, None);
        assert_eq!(node.dirty_count, None);
        assert_eq!(node.dirty_source, None);
        assert!(node.unpushed.is_empty() && node.not_merged.is_empty());
        assert_eq!(node.not_merged_count, 0);
        assert!(git_of(&result).would_discard.is_none());
        assert!(git_of(&result).discarded.is_none());
        let json = serde_json::to_value(git_of(&result)).expect("json");
        assert!(json["nodes"][0]["dirty"].is_null());
    }

    /// A `record` (live already equals the artifact) adopts too: the repo is
    /// advanced where it stands, never replaced.
    #[tokio::test]
    async fn adopt_record_action_adopts() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.make_live_equal_artifact().await;
        let result = rig.apply().await.expect("the record completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Recorded,
            "{:?}",
            result.warnings
        );
        assert!(rig.gateway.import_calls().is_empty(), "no import");
        let node = git_of(&result).master();
        assert!(node.adopted, "{node:?}");
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        assert_eq!(
            rig.f.git_in_project(&["symbolic-ref", "HEAD"]),
            format!("refs/heads/{ADOPT_BRANCH}")
        );
        rig.f.git_in_project(&["fsck", "--connectivity-only"]);
    }

    /// An unborn HEAD that is not the adoptable shape keeps its reason word.
    #[tokio::test]
    async fn adopt_not_adoptable_keeps_unborn_head() {
        // A local branch.
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.f
            .git_in_project(&["update-ref", "refs/heads/local-work", &rig.f.c1]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "unborn_head");

        // An index.
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.f.git_in_project(&["read-tree", &rig.f.c1]);
        assert!(rig.f.git_dir().join("index").exists());
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "unborn_head");

        // Another remote.
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.f
            .git_in_project(&["remote", "add", "second", "https://git.example/x/y.git"]);
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "multiple_remotes");

        // A merge in progress.
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        std::fs::write(
            rig.f.git_dir().join("MERGE_HEAD"),
            format!("{}\n", rig.f.c1),
        )
        .expect("marker");
        let err = refused_alike(&rig).await;
        assert_eq!(reason_of(&err), "mid_operation");
    }

    /// The `.git` is never deleted: not by a refusal, not by an adoption.
    #[tokio::test]
    async fn adopt_never_deletes_the_gateway_git() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some("main"))]);
        let before = rig.git_manifest().await;
        let _ = refused_alike(&rig).await;
        assert_eq!(rig.git_manifest().await, before);
        assert!(rig.f.git_dir().is_dir());
    }

    /// `--check` reports the plan and writes nothing.
    #[tokio::test]
    async fn adopt_check_writes_nothing() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        let before = rig.git_manifest().await;
        let result = rig.check().await.expect("the check completes");
        assert_eq!(result.outcome, ProjectOutcome::Checked);
        let git = git_of(&result);
        let node = git.master();
        assert_eq!(node.state, "adoptable");
        assert_eq!(node.branch.as_deref(), Some(ADOPT_BRANCH));
        assert_eq!(node.branch_source, Some("git_yaml"));
        assert!(!node.adopted, "nothing was adopted by a check");
        assert_eq!(node.dirty, None);
        assert!(node.save.is_none() && node.restore.is_none());
        assert_eq!(git.expected_drift, None);
        assert_eq!(git.expected_drift_count, None);
        assert!(!git.expected_drift_complete);
        let json = serde_json::to_value(git).expect("json");
        assert!(json["expected_drift"].is_null());
        assert!(rig.save_trees().is_empty());
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert_eq!(rig.git_manifest().await, before, ".git is byte-identical");
        assert!(rig.gateway.import_calls().is_empty());
        assert_eq!(
            rig.f.git_in_project(&["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );
    }

    /// The dummy credential in `git.yaml` reaches nothing the run writes or
    /// says, on success and on every refusal.
    #[tokio::test]
    async fn adopt_secret_never_reported() {
        use common::git_fixture::GIT_YAML_SECRET;
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        let result = rig.apply().await.expect("the apply completes");
        assert_secret_free(&rig, &result);

        // A branch name git does not accept refuses without echoing anything.
        for branch in ["-x", "a..b", "x.lock"] {
            let rig = adopting();
            rig.f.with_git_yaml(&[(PROJECT, Some(branch))]);
            let err = refusal(rig.apply().await);
            assert_eq!(err.code(), "deploy_preflight_failed", "{branch}");
            let text = format!("{err} {err:?}");
            assert!(!text.contains(GIT_YAML_SECRET), "{text}");
            assert!(!text.contains(branch), "the value is not echoed: {text}");
        }

        // A git.yaml that does not parse names the file and nothing else.
        let rig = adopting();
        common::git_fixture::put(
            &rig.f.data_dir,
            "git.yaml",
            &format!("- ignition_projectName: CapApp\n  user_password: [{GIT_YAML_SECRET}\n"),
        );
        let err = refusal(rig.apply().await);
        assert_eq!(err.code(), "deploy_preflight_failed");
        assert_eq!(preflight_checks(&err), ["git_yaml"]);
        let text = format!("{err} {err:?}");
        assert!(text.contains("git.yaml"), "{text}");
        assert!(!text.contains(GIT_YAML_SECRET), "{text}");

        // So does one over the size cap.
        let rig = adopting();
        common::git_fixture::put(
            &rig.f.data_dir,
            "git.yaml",
            &"#".repeat(ignition_core::deploy::yamlio::YAML_MAX_BYTES + 1),
        );
        let err = refusal(rig.apply().await);
        assert_eq!(err.code(), "deploy_preflight_failed");
        assert_eq!(preflight_checks(&err), ["git_yaml"]);
        assert!(err.to_string().contains("git.yaml"));
    }

    /// A config somebody already pointed elsewhere is not rewritten.
    #[tokio::test]
    async fn adopt_conflicting_upstream_refuses() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        rig.f.git_in_project(&[
            "config",
            &format!("branch.{ADOPT_BRANCH}.remote"),
            "elsewhere",
        ]);
        let before = rig.git_manifest().await;
        let err = refusal(rig.apply().await);
        assert_eq!(preflight_checks(&err), ["git_host"]);
        assert!(err.to_string().contains("already sets"), "{err}");
        assert_eq!(rig.git_manifest().await, before);
        assert!(rig.gateway.import_calls().is_empty());
    }

    /// A config that changed between the plan and the save is a stale plan:
    /// the adopted config would replace it whole.
    #[tokio::test]
    async fn adopt_config_changed_after_the_plan_is_stale() {
        let rig = adopting();
        rig.f.with_git_yaml(&[(PROJECT, Some(ADOPT_BRANCH))]);
        let host = rig.host();
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        rig.f.git_in_project(&["config", "core.comment", "changed"]);
        let err = refusal(rig.run_prepared(&host, &loaded, prepared, None).await);
        assert_eq!(err.code(), "plan_stale", "{err}");
        assert!(
            rig.gateway.import_calls().is_empty(),
            "nothing was imported"
        );
        assert!(rig.save_trees().is_empty(), "nothing was saved");
        assert_eq!(
            rig.f.git_in_project(&["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );
    }

    // ---- the adoption's advance, below the action ----

    fn write_label(name: &GitWriteName) -> String {
        match name {
            GitWriteName::PackFile(_) => "pack".to_string(),
            GitWriteName::PackIndex(_) => "idx".to_string(),
            other => other.rel(),
        }
    }

    /// The adoption's writes come in the fixed order: pack, pack index, branch
    /// ref, remote-tracking ref, config, HEAD, index; a managed advance has
    /// no config write.
    #[tokio::test]
    async fn adopt_writes_in_the_fixed_order() {
        use ignition_core::deploy::git_advance::finish_advance;
        let f = GitFixture::new();
        f.partial_behind();
        let host = HostGit::new();
        let (_state, prep) = f.prepare_adopt(&host, BRANCH).await;
        assert!(prep.adopted);
        assert!(
            prep.pack.is_some(),
            "the release is missing: it is sent as a pack"
        );
        let config = String::from_utf8(prep.config.clone().expect("config")).expect("utf8");
        assert!(
            config.contains(&format!("[branch \"{BRANCH}\"]")),
            "{config}"
        );
        f.module_import(true, &[], &[], &[]);
        let disk = f.node().project_file_digests(PROJECT).await.expect("disk");
        let plan = finish_advance(&host, &f.checkout, &prep, &f.export_zip(), &disk)
            .await
            .expect("the plan");
        let order: Vec<String> = plan.writes.iter().map(|w| write_label(&w.name)).collect();
        assert_eq!(
            order,
            [
                "pack".to_string(),
                "idx".to_string(),
                "index".to_string(),
                "config".to_string(),
                format!("refs/remotes/origin/{BRANCH}"),
                format!("refs/heads/{BRANCH}"),
                "HEAD".to_string(),
            ]
        );
        // The first write is only planned: nothing was written.
        assert_eq!(
            f.git_in_project(&["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );

        // A managed repo has no config write.
        let m = GitFixture::new();
        let (_state, prep) = m.prepare(&host).await;
        assert!(!prep.adopted && prep.config.is_none() && prep.config_base.is_none());
        m.module_import(true, &[], &[], &[]);
        let disk = m.node().project_file_digests(PROJECT).await.expect("disk");
        let plan = finish_advance(&host, &m.checkout, &prep, &m.export_zip(), &disk)
            .await
            .expect("the plan");
        assert!(
            plan.writes
                .iter()
                .all(|w| !matches!(w.name, GitWriteName::Config))
        );
    }

    /// The whole handover with a pack: the module's import deletes `.git`, the
    /// save is restored and the adoption is written; the release objects the
    /// partial repo lacked arrive and fsck passes.
    #[tokio::test]
    async fn adopt_handover_sends_the_missing_release() {
        let f = GitFixture::new();
        f.partial_behind();
        let host = HostGit::new();
        let node = f.node();
        let (state, prep) = f.prepare_adopt(&host, BRANCH).await;
        let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
            .await
            .expect("save");
        f.module_import(false, &[UTIL_RES], &[], &[]);
        let report = restore_and_advance(
            &node,
            &node,
            &host,
            &f.checkout,
            &state,
            &saved,
            &prep,
            &f.export_zip(),
            None,
        )
        .await
        .expect("the handover");
        assert!(
            report.restored && report.advanced && report.verified,
            "{report:?}"
        );
        assert_eq!(f.git_in_project(&["rev-parse", "HEAD"]), f.c2);
        f.git_in_project(&["fsck", "--connectivity-only"]);
        assert_eq!(f.status_paths(), report.expected_drift);
        assert_eq!(
            f.git_in_project(&["config", &format!("branch.{BRANCH}.merge")]),
            format!("refs/heads/{BRANCH}")
        );
    }

    /// The adopted config is written only over the config it was built from.
    #[tokio::test]
    async fn adopt_refuses_a_config_that_changed() {
        let f = GitFixture::new();
        f.partial_setup_local_repo();
        let host = HostGit::new();
        let node = f.node();
        let (state, prep) = f.prepare_adopt(&host, BRANCH).await;
        let saved = save(&node, &node, LedgerRoot::DataDir, GATEWAY, PROJECT, RUN)
            .await
            .expect("save");
        f.module_import(true, &[], &[], &[]);
        f.git_in_project(&["config", "core.comment", "changed"]);
        let failure = restore_and_advance(
            &node,
            &node,
            &host,
            &f.checkout,
            &state,
            &saved,
            &prep,
            &f.export_zip(),
            None,
        )
        .await
        .expect_err("a changed config is refused");
        assert_eq!(failure.reason, "git_recreated");
        assert_eq!(failure.paths, [".git/config"]);
        assert_eq!(
            f.git_in_project(&["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );
        assert!(
            !f.git_dir().join("refs/heads").join(BRANCH).exists(),
            "no branch ref was written"
        );
        assert!(!f.git_dir().join("index").exists(), "no index was written");
    }

    // ---- 27-18: the module's start-sync hazards (D-21, D-G3) ----

    const TAGS_FLAG: (&str, &str) = ("commissioning_importTags", "true");
    const STARTUP_FLAG: (&str, &str) = ("tags_importOnStartup", "true");
    const IMAGES_FLAG: (&str, &str) = ("commissioning_importImages", "true");
    const THEMES_FLAG: (&str, &str) = ("commissioning_importThemes", "true");
    const ENFORCE_FLAG: (&str, &str) = ("commissioning_enforceBranch", "true");
    const HAZARD_REASON: &str = "the module imports tags at the start on purpose (ticket 7)";

    /// A rig whose `git.yaml` sets `flags` for the project (the clone's
    /// `tags/` is present, as on a gateway before an import).
    fn hazardous(flags: &[(&str, &str)]) -> Rig {
        let rig = Rig::new();
        rig.f.with_git_yaml_flags(&[(PROJECT, flags)]);
        rig
    }

    fn warned(result: &DeployProjectsResult, needle: &str) -> bool {
        result.warnings.iter().any(|w| w.contains(needle))
    }

    /// D-G3: a tag import flag with `tags/` present on the node refuses, for
    /// both flags, in `--check` and in the apply, and nothing is written.
    #[tokio::test]
    async fn hazard_tags_refuses() {
        for flag in [TAGS_FLAG, STARTUP_FLAG] {
            let rig = hazardous(&[flag]);
            assert!(rig.f.project_dir().join("tags").is_dir());
            let before = rig.git_manifest().await;
            for err in [refusal(rig.check().await), refusal(rig.apply().await)] {
                let (gate, keys, count) = gate_of(&err);
                assert_eq!(gate, "git_sync_hazard", "{flag:?}");
                assert_eq!(keys, ["import_tags".to_string()], "{flag:?}");
                assert_eq!(count, 1);
                assert_eq!(err.code(), "destructive_change_refused");
            }
            assert_eq!(rig.git_manifest().await, before, "nothing was written");
            assert!(rig.save_trees().is_empty());
        }
    }

    /// The acknowledgement passes the refusal, is recorded in `data.git`,
    /// `run.json` and a warning that does not echo it, and `--check` shows it
    /// too.
    #[tokio::test]
    async fn hazard_ack_passes() {
        let mut rig = hazardous(&[TAGS_FLAG]);
        rig.hazard_ack = Some(HAZARD_REASON.to_string());
        let checked = rig.check().await.expect("the check completes");
        assert_eq!(checked.outcome, ProjectOutcome::Checked);
        assert_eq!(
            git_of(&checked).hazards_acknowledged.as_deref(),
            Some(HAZARD_REASON)
        );
        assert_eq!(git_of(&checked).master().hazards, ["import_tags"]);

        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert!(result.ok && result.slug.is_none());
        let git = git_of(&result);
        assert_eq!(git.hazards_acknowledged.as_deref(), Some(HAZARD_REASON));
        assert_eq!(git.master().hazards, ["import_tags"]);
        let json = serde_json::to_value(git).expect("json");
        assert_eq!(json["hazards_acknowledged"], HAZARD_REASON);
        assert_eq!(json["nodes"][0]["hazards"][0], "import_tags");
        assert_eq!(
            run_json(&rig, &result)["allow_git_sync_hazards"],
            HAZARD_REASON
        );
        assert!(warned(&result, "import_tags"));
        assert!(warned(&result, "acknowledged"));
        assert!(
            !result.warnings.iter().any(|w| w.contains(HAZARD_REASON)),
            "the operator's text is data, not part of a message"
        );
        assert_secret_free(&rig, &result);
    }

    /// The acknowledgement of a run with nothing to acknowledge is not
    /// recorded as one.
    #[tokio::test]
    async fn hazard_ack_without_a_hazard_is_not_recorded() {
        let mut rig = Rig::new();
        rig.hazard_ack = Some(HAZARD_REASON.to_string());
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert!(git_of(&result).hazards_acknowledged.is_none());
        assert_eq!(
            run_json(&rig, &result)["allow_git_sync_hazards"],
            HAZARD_REASON,
            "run.json records what the operator passed"
        );
    }

    /// D-G3: with `tags/` absent on the node the true flag is reported and
    /// the run goes on (ign's own import removes `tags/`).
    #[tokio::test]
    async fn hazard_tags_absent_reports() {
        let rig = hazardous(&[TAGS_FLAG, STARTUP_FLAG]);
        rig.f.without_tags_dir();
        let checked = rig.check().await.expect("the check completes");
        let node = git_of(&checked).master();
        assert_eq!(node.hazards, ["import_tags"]);
        assert!(git_of(&checked).hazards_acknowledged.is_none());
        assert!(warned(&checked, "tags/ is absent on the node"));
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert_eq!(git_of(&result).master().hazards, ["import_tags"]);
        assert!(git_of(&result).hazards_acknowledged.is_none());
    }

    /// D-G3: `commissioning_importImages` and `commissioning_importThemes`
    /// are advisory: reported, warned about, never refused, with their
    /// directories present or not.
    #[tokio::test]
    async fn hazard_harmless_flag_warns() {
        let rig = hazardous(&[IMAGES_FLAG, THEMES_FLAG]);
        assert!(rig.f.project_dir().join("images").is_dir());
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let git = git_of(&result);
        assert_eq!(git.master().hazards, ["import_images", "import_themes"]);
        assert!(git.hazards_acknowledged.is_none());
        assert!(warned(&result, "git.yaml sets import_images"));
        assert!(warned(&result, "git.yaml sets import_themes"));

        let rig = hazardous(&[IMAGES_FLAG]);
        let _ = std::fs::remove_dir_all(rig.f.project_dir().join("images"));
        let checked = rig.check().await.expect("the check completes");
        assert_eq!(git_of(&checked).master().hazards, ["import_images"]);
    }

    /// G6: `commissioning_enforceBranch: true` refuses whatever the node
    /// holds, and the acknowledgement passes it.
    #[tokio::test]
    async fn hazard_enforce_branch() {
        let mut rig = hazardous(&[ENFORCE_FLAG]);
        rig.f.without_tags_dir();
        let err = refusal(rig.check().await);
        let (gate, keys, _) = gate_of(&err);
        assert_eq!(gate, "git_sync_hazard");
        assert_eq!(keys, ["enforce_branch".to_string()]);
        rig.hazard_ack = Some(HAZARD_REASON.to_string());
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        assert_eq!(
            git_of(&result).hazards_acknowledged.as_deref(),
            Some(HAZARD_REASON)
        );
    }

    /// Every refusing hazard is named in one refusal, in hazard order.
    #[tokio::test]
    async fn hazard_refusal_names_every_refusing_word() {
        let rig = hazardous(&[ENFORCE_FLAG, TAGS_FLAG, IMAGES_FLAG]);
        let err = refusal(rig.check().await);
        let (gate, keys, count) = gate_of(&err);
        assert_eq!(gate, "git_sync_hazard");
        assert_eq!(
            keys,
            ["import_tags".to_string(), "enforce_branch".to_string()]
        );
        assert_eq!(count, 2);
    }

    /// No `git.yaml`: the hazards are unknown, a warning and no refusal.
    #[tokio::test]
    async fn hazard_unknown_without_git_yaml() {
        let rig = Rig::new();
        let result = rig.check().await.expect("the check completes");
        assert!(git_of(&result).master().hazards.is_empty());
        assert!(
            warned(&result, "git.yaml is absent"),
            "{:?}",
            result.warnings
        );
        assert!(warned(&result, "hazards are unknown"));
        let applied = rig.apply().await.expect("the apply completes");
        assert_eq!(applied.outcome, ProjectOutcome::Imported);
    }

    /// A `git.yaml` that does not parse leaves the hazards unknown too, and
    /// no word of it reaches a message.
    #[tokio::test]
    async fn hazard_unreadable_git_yaml_warns() {
        use common::git_fixture::GIT_YAML_SECRET;
        let rig = Rig::new();
        put_file(
            &rig.f.data_dir,
            "git.yaml",
            &format!(
                "- ignition_projectName: [{GIT_YAML_SECRET}\n  commissioning_importTags: true\n"
            ),
        );
        let result = rig.check().await.expect("the check completes");
        assert!(git_of(&result).master().hazards.is_empty());
        assert!(
            warned(&result, "git.yaml is unreadable"),
            "{:?}",
            result.warnings
        );
        assert!(!result.warnings.join("\n").contains(GIT_YAML_SECRET));
    }

    /// An entry for another project does not count; a file that names this
    /// project as well as another reads only this project's.
    #[tokio::test]
    async fn hazard_other_project_ignored() {
        let rig = Rig::new();
        rig.f.with_git_yaml_flags(&[
            ("Other", &[TAGS_FLAG, ENFORCE_FLAG]),
            (PROJECT, &[("commissioning_importImages", "false")]),
        ]);
        let result = rig.check().await.expect("the check completes");
        assert!(git_of(&result).master().hazards.is_empty());
        assert!(!warned(&result, "hazards are unknown"));
        assert!(!warned(&result, "git.yaml sets"));
    }

    /// A `record` hands the kept `.git` over like an import, so it refuses
    /// too; a `skip` hands nothing over, so it only warns.
    #[tokio::test]
    async fn hazard_record_refuses_and_skip_warns() {
        let rig = Rig::new();
        rig.make_live_equal_artifact().await;
        rig.f.with_git_yaml_flags(&[(PROJECT, &[ENFORCE_FLAG])]);
        let err = refusal(rig.check().await);
        assert_eq!(gate_of(&err).0, "git_sync_hazard");

        // A green apply with the acknowledgement, then the same plan again is
        // a `skip` (the repo and the record name the release): no refusal.
        let mut rig = rig;
        rig.hazard_ack = Some(HAZARD_REASON.to_string());
        let result = rig.apply().await.expect("the apply completes");
        assert_eq!(result.outcome, ProjectOutcome::Recorded);
        rig.hazard_ack = None;
        let again = rig.check().await.expect("a skip does not refuse");
        assert_eq!(
            serde_json::to_value(&again).expect("json")["action"],
            "skip"
        );
        assert!(warned(&again, "git.yaml sets enforce_branch"));
        assert!(git_of(&again).hazards_acknowledged.is_none());
    }

    /// An adopted partial repo is kept too: the same hazards apply.
    #[tokio::test]
    async fn hazard_adopted_repo_refuses() {
        let rig = adopting();
        rig.f
            .with_git_yaml_flags(&[(PROJECT, &[("repo_branch", ADOPT_BRANCH), ENFORCE_FLAG])]);
        let err = refusal(rig.check().await);
        assert_eq!(gate_of(&err).0, "git_sync_hazard");
        assert_eq!(gate_of(&err).1, ["enforce_branch".to_string()]);
    }

    /// A git writer that records what the run asked for.
    struct RecordingWriter {
        inner: LocalFsTransport,
        calls: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl GitStateWriter for RecordingWriter {
        async fn save_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.calls
                .lock()
                .expect("lock")
                .push("save_git".to_string());
            self.inner.save_git(gateway, project, run_id).await
        }
        async fn restore_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.calls
                .lock()
                .expect("lock")
                .push("restore_git".to_string());
            self.inner.restore_git(gateway, project, run_id).await
        }
        async fn write_git_files(
            &self,
            project: &str,
            writes: &[GitWrite],
        ) -> Result<(), CoreError> {
            {
                let mut calls = self.calls.lock().expect("lock");
                calls.extend(
                    writes
                        .iter()
                        .map(|w| format!("write .git/{}", w.name.rel())),
                );
            }
            self.inner.write_git_files(project, writes).await
        }
        async fn remove_git_save(&self, g: &str, p: &str, r: &str) -> Result<(), CoreError> {
            self.calls
                .lock()
                .expect("lock")
                .push("remove_git_save".to_string());
            self.inner.remove_git_save(g, p, r).await
        }
        async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
            self.calls
                .lock()
                .expect("lock")
                .push("probe_write".to_string());
            self.inner.probe_write(project, run_id).await
        }
    }

    /// D-21: the run restores nothing but `.git`. The git writer's surface
    /// can only name `.git`, and no call it receives names `tags/`, `images/`
    /// or `themes/`; the files below those directories after the run are
    /// among those before it.
    #[tokio::test]
    async fn no_tags_restored() {
        let mut rig = hazardous(&[TAGS_FLAG, IMAGES_FLAG, THEMES_FLAG]);
        rig.hazard_ack = Some(HAZARD_REASON.to_string());
        let project_dir = rig.f.project_dir();
        let list = |dirs: &[&str]| -> std::collections::BTreeSet<PathBuf> {
            let mut found = Vec::new();
            for dir in dirs {
                files_below(&project_dir.join(dir), &mut found);
            }
            found.into_iter().collect()
        };
        let before = list(&["tags", "images", "themes"]);
        assert!(!before.is_empty(), "tags/ and images/ are present before");
        let host = rig.host();
        let writer = RecordingWriter {
            inner: rig.f.node(),
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, Some(&writer))
            .await
            .expect("the run completes");
        assert_eq!(result.outcome, ProjectOutcome::Imported);
        let calls = writer.calls.lock().expect("lock").clone();
        assert!(calls.iter().any(|c| c == "save_git"), "{calls:?}");
        assert!(
            calls.iter().any(|c| c.starts_with("write .git/")),
            "{calls:?}"
        );
        for call in &calls {
            for dir in ["tags", "images", "themes"] {
                assert!(!call.contains(&format!("{dir}/")), "{call}");
            }
            assert!(
                matches!(
                    call.as_str(),
                    "save_git" | "restore_git" | "remove_git_save" | "probe_write"
                ) || call.starts_with("write .git/"),
                "{call}"
            );
        }
        let after = list(&["tags", "images", "themes"]);
        assert!(
            after.is_subset(&before),
            "the run created files below tags/, images/ or themes/: {:?}",
            after.difference(&before).collect::<Vec<_>>()
        );
    }

    /// The refusal text and the hint name the flag and the git.yaml fix.
    #[test]
    fn git_sync_hazard_hint_names_the_flag() {
        let err = CoreError::DestructiveChangeRefused {
            gate: "git_sync_hazard".to_string(),
            keys: vec!["import_tags".to_string(), "enforce_branch".to_string()],
            count: 2,
        };
        let text = err.to_string();
        assert!(text.contains("--allow-git-sync-hazards <reason>"), "{text}");
        assert!(text.contains("import_tags, enforce_branch"), "{text}");
        let hint = err.hint().expect("hint");
        for needle in [
            "--allow-git-sync-hazards",
            "git.yaml",
            "set the flag false",
            "import_tags",
            "enforce_branch",
            "data.git.hazards_acknowledged",
            "run.json",
            "Nothing was written",
        ] {
            assert!(hint.contains(needle), "{needle}: {hint}");
        }
        assert!(!text.contains("--allow-destructive"));
    }

    // ---- 27-22: a redundant pair (D-16, D-19, D-G5) ----

    use ignition_core::client::redundancy::RedundancyStatusWire;
    use ignition_core::deploy::tag_converge::BackupTags;

    /// The backup node as the run sees it, and the sync that feeds it: a
    /// second fake gateway with its own disk mirror (the module manages its
    /// project, so every content-changing sync deletes its `.git`, G5 (b)),
    /// which follows the master's project after `lag` reads. The only call the
    /// run can make on it is `project_export`, a GET.
    pub struct FakeBackupNode<'a> {
        pub master: &'a FakeProjects,
        pub gw: FakeProjects,
        pub lag: usize,
        pub reads: std::sync::Mutex<Vec<String>>,
        /// The sync already deleted the backup's `.git` when the run reads it
        /// first (the content changed during the run).
        pub drop_git_at_first_read: bool,
        /// The sync already deleted the backup's `project.json` when the run
        /// reads it first (27-37): for a rollback, where the backup's project
        /// needs no import and so no mirror write takes it.
        pub drop_project_json_at_first_read: bool,
        pub backup_git: PathBuf,
        /// The backup's copy of the owned tags is not the master's: one
        /// documentation value differs (27-34).
        pub tags_differ: bool,
        /// Run the closure once, on the backup's `n`th (1-based) tag export:
        /// the redundancy sync reaching the backup late, after the run's wait
        /// for its project (27-55, Q3). Only the backup's tag check reads tag
        /// exports, so this fires inside `backup_step`.
        pub on_tag_export: Option<(usize, Box<dyn Fn() + Send + Sync>)>,
        pub tag_exports: std::sync::atomic::AtomicUsize,
    }

    impl FakeBackupNode<'_> {
        pub fn reads(&self) -> Vec<String> {
            self.reads.lock().expect("reads").clone()
        }
    }

    #[async_trait::async_trait]
    impl BackupTags for FakeBackupNode<'_> {
        async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
            // A tag-owning pair run also waits for the owned tags (26-13): the
            // backup answers with the master's export in the backup's shape
            // (no live values, children reversed).
            let nth = self
                .tag_exports
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            if let Some((at, hook)) = &self.on_tag_export
                && *at == nth
            {
                hook();
            }
            self.reads
                .lock()
                .expect("reads")
                .push("GET tag_export".into());
            use ignition_core::actions::deploy_projects::ProjectGateway as Gw;
            let bytes = Gw::tag_export(self.master, provider).await?;
            let mut tree: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| CoreError::Internal(format!("the master export: {e}")))?;
            fn shape(node: &mut serde_json::Value, in_types: bool) {
                let in_types = in_types || node["name"] == "_types_";
                if !in_types
                    && node["tagType"] == "AtomicTag"
                    && let Some(object) = node.as_object_mut()
                {
                    for key in ["value", "timestamp", "quality"] {
                        object.remove(key);
                    }
                }
                if let Some(children) = node.get_mut("tags").and_then(|t| t.as_array_mut()) {
                    children.reverse();
                    for child in children {
                        shape(child, in_types);
                    }
                }
            }
            shape(&mut tree, false);
            if self.tags_differ {
                fn alter(node: &mut serde_json::Value) {
                    if node["tagType"] == "AtomicTag"
                        && let Some(object) = node.as_object_mut()
                    {
                        object.insert("documentation".into(), "the backup's own".into());
                    }
                    if let Some(children) = node.get_mut("tags").and_then(|t| t.as_array_mut()) {
                        children.iter_mut().for_each(alter);
                    }
                }
                alter(&mut tree);
            }
            Ok(serde_json::to_vec(&tree).expect("json"))
        }
        async fn status(&self) -> Result<RedundancyStatusWire, CoreError> {
            self.reads.lock().expect("reads").push("GET status".into());
            Ok(serde_json::from_value(serde_json::json!({
                "role": "Backup", "activityLevel": "Cold", "projectState": "Good",
                "peerConnected": true, "syncPending": false,
                "lastSyncTimestamp": 1794179312498i64
            }))
            .expect("a status"))
        }
        async fn sync_exhausted(&self, _since_ms: i64) -> Result<bool, CoreError> {
            self.reads.lock().expect("reads").push("GET logs".into());
            Ok(false)
        }
        async fn project_export(&self, project: &str) -> Result<Vec<u8>, CoreError> {
            let n = {
                let mut reads = self.reads.lock().expect("reads");
                reads.push("GET project_export".into());
                reads.len()
            };
            if self.drop_git_at_first_read && n == 1 {
                std::fs::remove_dir_all(&self.backup_git).expect("the sync deletes .git");
            }
            if self.drop_project_json_at_first_read && n == 1 {
                let file = self.backup_git.with_file_name("project.json");
                std::fs::remove_file(file).expect("the sync deletes project.json");
            }
            if n > self.lag {
                use ignition_core::actions::deploy_projects::ProjectGateway as Gw;
                let current = Gw::export_bytes(self.master, project).await?;
                let mine = Gw::export_bytes(&self.gw, project).await?;
                if canonical_digest(&current)?.sha256 != canonical_digest(&mine)?.sha256 {
                    // The redundancy sync, on the backup's own side.
                    Gw::project_import(&self.gw, project, current, true).await?;
                }
            }
            ignition_core::actions::deploy_projects::ProjectGateway::export_bytes(&self.gw, project)
                .await
        }
    }

    const WAIT: std::time::Duration = std::time::Duration::from_millis(400);

    fn node_of<'a>(
        git: &'a GitReport,
        name: &str,
    ) -> &'a ignition_core::actions::deploy_projects::git::NodeGitReport {
        git.nodes
            .iter()
            .find(|n| n.node == name)
            .unwrap_or_else(|| panic!("{name} is in data.git.nodes: {:?}", git.nodes))
    }

    /// A pair whose two clones are the same module clone.
    fn pair() -> Rig {
        let rig = Rig::new();
        rig.add_backup();
        rig
    }

    #[tokio::test]
    async fn pair_both_managed_agree() {
        let rig = pair();
        let result = rig.check().await.expect("the check completes");
        let git = git_of(&result);
        assert_eq!(git.nodes.len(), 2, "{:?}", git.nodes);
        assert_eq!(git.nodes[0].node, "nodes.master");
        assert_eq!(git.nodes[1].node, "nodes.backup");
        for node in &git.nodes {
            assert_eq!(node.state, "managed", "{node:?}");
            assert_eq!(node.branch.as_deref(), Some(BRANCH));
            assert_eq!(node.head.as_deref(), Some(rig.f.c1.as_str()));
            // The gates ran on both nodes against the live export.
            assert_eq!(node.dirty, Some(Vec::new()), "{node:?}");
            assert_eq!(node.dirty_source, Some("release_store"));
        }
        let json = serde_json::to_value(git).expect("json");
        assert_eq!(json["nodes"][1]["node"], "nodes.backup");
    }

    #[tokio::test]
    async fn pair_heads_differ_refuses() {
        let rig = pair();
        // The master gains a pushed commit the backup lacks.
        let ahead = rig.f.designer_commit_changing(
            "ignition/script-python/cap/util/extra_designer.py",
            "x = 1\n",
            true,
        );
        rig.sync_live();
        let err = refusal(rig.check().await);
        assert_eq!(reason_of(&err), "nodes_disagree");
        let text = err.to_string();
        assert!(text.contains(&ahead[..12]), "{text}");
        assert!(text.contains(&rig.f.c1[..12]), "{text}");
        assert!(
            text.contains("nodes.master") && text.contains("nodes.backup"),
            "{text}"
        );
        // The apply refuses alike, before anything is written.
        let again = refusal(rig.apply().await);
        assert_eq!(again.to_string(), text);
        assert!(rig.gateway.import_calls().is_empty());
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert!(
            rig.save_trees().is_empty() && rig.save_trees_in(&rig.f.backup_data_dir()).is_empty()
        );
    }

    #[tokio::test]
    async fn pair_origin_differs_refuses() {
        let rig = pair();
        rig.f.git_in_backup(&[
            "config",
            "remote.capapp.url",
            "https://user:hunter2@other.example.invalid/elsewhere.git",
        ]);
        let err = refusal(rig.check().await);
        assert_eq!(reason_of(&err), "nodes_disagree");
        let text = format!("{err} {err:?}");
        assert!(text.contains("origin"), "{text}");
        assert!(
            !text.contains("hunter2") && !text.contains("user:"),
            "the URL is redacted: {text}"
        );
        assert!(text.contains("other.example.invalid"), "{text}");
    }

    #[tokio::test]
    async fn pair_branch_differs_refuses() {
        let rig = pair();
        rig.f.git_in_backup(&["branch", "-m", BRANCH, "staging"]);
        // `branch -m` carries the section over with `merge` still naming the
        // old branch; since 27-27 (WR-A5) that is `no_upstream`. The backup is
        // a managed clone on `staging`, tracking `staging`, so only the branch
        // name differs between the nodes.
        rig.f
            .git_in_backup(&["config", "branch.staging.merge", "refs/heads/staging"]);
        let err = refusal(rig.check().await);
        assert_eq!(reason_of(&err), "nodes_disagree");
        let text = err.to_string();
        assert!(
            text.contains("branch") && text.contains("staging") && text.contains(BRANCH),
            "{text}"
        );
    }

    #[tokio::test]
    async fn pair_backup_absent_independent() {
        let rig = pair();
        std::fs::remove_dir_all(rig.f.backup_git_dir()).expect("no .git on the backup");
        let result = rig.check().await.expect("the check completes");
        let git = git_of(&result);
        assert_eq!(git.nodes.len(), 2);
        assert_eq!(git.nodes[0].state, "managed");
        assert_eq!(node_of(git, "nodes.backup").state, "absent");
        assert!(node_of(git, "nodes.backup").restore.is_none());
        // A run handles the master as a single node; the backup is untouched.
        let backup = rig.backup_node(0);
        let applied = rig
            .apply_pair(&backup, WAIT)
            .await
            .expect("the apply completes");
        assert_eq!(applied.outcome, ProjectOutcome::Imported);
        assert!(applied.ok);
        let git = git_of(&applied);
        assert!(
            git.nodes[0]
                .restore
                .as_ref()
                .expect("master restore")
                .advanced
        );
        assert_eq!(node_of(git, "nodes.backup").state, "absent");
        assert!(
            !rig.f.backup_git_dir().exists(),
            "a node without .git stays without one"
        );
        assert!(
            backup.reads().is_empty(),
            "no wait for a node with nothing to hand over"
        );
    }

    #[tokio::test]
    async fn pair_master_absent_backup_managed() {
        let rig = pair();
        std::fs::remove_dir_all(rig.f.git_dir()).expect("no .git on the master");
        let check = rig.check().await.expect("the check completes");
        let git = git_of(&check);
        assert_eq!(git.nodes[0].state, "absent");
        assert_eq!(git.nodes[1].state, "managed");
        assert_eq!(git.nodes[1].dirty, Some(Vec::new()));
        // The run deploys the master as today and hands the backup's repo over.
        let backup = rig.backup_node(1);
        let result = rig
            .apply_pair(&backup, WAIT)
            .await
            .expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        let git = git_of(&result);
        assert_eq!(git.nodes[0].state, "absent");
        let restore = git.nodes[1].restore.as_ref().expect("the backup restore");
        assert!(
            restore.restored && restore.advanced && restore.verified,
            "{restore:?}"
        );
        assert!(!rig.f.git_dir().exists(), "the master stays without a .git");
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
    }

    /// WR-09 (27-35, D-05, D-19): the master holds no `.git`, the backup's
    /// repository is dirty and the run gives `--discard-gateway-changes`. The
    /// evidence names the backup, the event names it, and `data.git.discarded`
    /// follows the backup's advance.
    #[tokio::test]
    async fn pair_backup_only_discard_writes_evidence() {
        let mut rig = Rig::new();
        rig.f
            .live_change(VIEW_MEMBER, "{\"root\":{\"live\":true}}\n");
        rig.sync_live();
        rig.add_backup();
        std::fs::remove_dir_all(rig.f.git_dir()).expect("no .git on the master");
        rig.discard = Some(REASON.to_string());
        let check = rig.check().await.expect("the check completes");
        let would = git_of(&check)
            .would_discard
            .as_ref()
            .expect("the backup's would_discard");
        assert_eq!(would.paths, [VIEW_MEMBER.to_string()]);

        let backup = rig.backup_node(1);
        let result = rig
            .apply_pair(&backup, WAIT)
            .await
            .expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        let evidence = discarded_json(&rig, &result);
        assert_eq!(evidence["node"], "nodes.backup", "{evidence}");
        assert_eq!(evidence["reason"], REASON);
        assert_eq!(evidence["paths"], serde_json::json!([VIEW_MEMBER]));
        let events = rig.events(&result);
        let event = events
            .iter()
            .find(|e| e["event"] == "git_discarded")
            .expect("git_discarded is emitted");
        assert_eq!(event["node"], "nodes.backup", "{event}");
        assert_eq!(event["evidence"], "git/discarded.json");
        let git = git_of(&result);
        assert!(git.would_discard.is_none(), "{:?}", git.would_discard);
        let discarded = discarded_of(&result);
        assert_eq!(discarded.reason, REASON);
        assert_eq!(discarded.paths, [VIEW_MEMBER.to_string()]);
        assert_eq!(git.nodes[0].state, "absent");
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
    }

    #[tokio::test]
    async fn pair_backup_unsupported_refuses() {
        let rig = pair();
        rig.f.git_in_backup(&["checkout", "-q", "--detach"]);
        let err = refusal(rig.check().await);
        assert_eq!(reason_of(&err), "detached_head");
        let text = err.to_string();
        assert!(text.contains("nodes.backup"), "{text}");
        assert!(!text.contains("nodes.master"), "{text}");
        let again = refusal(rig.apply().await);
        assert_eq!(again.to_string(), text);
        assert!(rig.gateway.import_calls().is_empty());
        assert_eq!(rig.run_dirs(), 0);
    }

    /// D-24 on the backup: both nodes hold the same unpushed Designer commit
    /// (so they agree) that ign's store lacks, and the live project differs
    /// from it; each node's dirty list is read through that node's own host.
    #[tokio::test]
    async fn pair_backup_dirty_from_its_objects() {
        let mut rig = Rig::new();
        let d = rig.f.designer_commit(false);
        rig.f
            .live_change("ignition/script-python/cap/util/live.py", "y = 2\n");
        rig.sync_live();
        rig.add_backup();
        rig.discard = Some("pair rig".to_string());
        let result = rig.check().await.expect("the check completes");
        let git = git_of(&result);
        for node in &git.nodes {
            assert_eq!(node.head.as_deref(), Some(d.as_str()), "{node:?}");
            assert_eq!(node.dirty_source, Some("gateway_objects"), "{node:?}");
            assert!(
                node.dirty
                    .as_ref()
                    .is_some_and(|l| l.iter().any(|p| p.ends_with("live.py"))),
                "{node:?}"
            );
        }
        let would = git.would_discard.as_ref().expect("would_discard");
        assert_eq!(would.dirty, DiscardDirty::Listed);
        // The backup's own objects are what its read uses: remove one of its
        // loose objects and only its read fails, naming the node.
        let loose = {
            let objects = rig.f.backup_git_dir().join("objects");
            let hex = &d[..2];
            let rest = &d[2..];
            objects.join(hex).join(rest)
        };
        if loose.exists() {
            std::fs::remove_file(&loose).expect("drop the backup's commit object");
        } else {
            rig.f.git_in_backup(&["gc", "-q", "--prune=now"]);
            let idx_pack: Vec<_> = std::fs::read_dir(rig.f.backup_git_dir().join("objects/pack"))
                .expect("pack dir")
                .flatten()
                .map(|e| e.path())
                .collect();
            for path in idx_pack {
                std::fs::remove_file(path).expect("drop the backup's packs");
            }
        }
        let err = refusal(rig.check().await);
        assert_eq!(preflight_checks(&err), ["git_gateway_objects"], "{err}");
        assert!(err.to_string().contains("nodes.backup"), "{err}");
    }

    /// The 27-18 hand-off: hazards are read per node, through that node's own
    /// host, so a flag only the backup's `git.yaml` sets refuses naming the
    /// backup (and passes with the acknowledgement).
    #[tokio::test]
    async fn pair_hazards_come_from_each_nodes_own_git_yaml() {
        let mut rig = pair();
        put_file(
            &rig.f.backup_data_dir(),
            "git.yaml",
            &format!(
                "- ignition_projectName: {PROJECT}\n  {}: {}\n",
                ENFORCE_FLAG.0, ENFORCE_FLAG.1
            ),
        );
        let err = refusal(rig.check().await);
        let (gate, keys, _) = gate_of(&err);
        assert_eq!(gate, "git_sync_hazard");
        assert_eq!(keys, ["nodes.backup:enforce_branch"]);
        rig.hazard_ack = Some("pair rig".to_string());
        let result = rig.check().await.expect("acknowledged");
        let git = git_of(&result);
        assert!(git.nodes[0].hazards.is_empty(), "{:?}", git.nodes[0]);
        assert_eq!(git.nodes[1].hazards, ["enforce_branch"]);
    }

    #[tokio::test]
    async fn pair_check_reads_only() {
        let rig = pair();
        let master = rig.git_manifest().await;
        let backup = rig
            .f
            .backup_node()
            .git_manifest(PROJECT)
            .await
            .expect("manifest");
        let result = rig.check().await.expect("the check completes");
        assert_eq!(result.outcome, ProjectOutcome::Checked);
        let git = git_of(&result);
        assert!(
            git.nodes
                .iter()
                .all(|n| n.save.is_none() && n.restore.is_none())
        );
        assert_eq!(rig.git_manifest().await, master);
        assert_eq!(
            rig.f
                .backup_node()
                .git_manifest(PROJECT)
                .await
                .expect("manifest"),
            backup
        );
        assert!(rig.save_trees().is_empty());
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
        assert_eq!(rig.run_dirs(), 0);
        assert!(rig.gateway.import_calls().is_empty());
    }

    // ---- 27-22: the run on both nodes ----

    #[tokio::test]
    async fn pair_run_advances_both() {
        let rig = pair();
        let backup = rig.backup_node(2);
        let result = rig
            .apply_pair(&backup, WAIT)
            .await
            .expect("the apply completes");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert!(result.ok && result.slug.is_none());
        let git = git_of(&result);
        for name in ["nodes.master", "nodes.backup"] {
            let node = node_of(git, name);
            let restore = node
                .restore
                .as_ref()
                .unwrap_or_else(|| panic!("{name} restore"));
            assert!(
                restore.restored && restore.advanced && restore.verified,
                "{name}: {restore:?}"
            );
            assert_eq!(restore.reason, None);
            assert!(!restore.save_kept, "{name}: the save is removed");
            assert_eq!(restore.post_status, Some("matched"), "{name}");
        }
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
        assert_eq!(
            rig.f.status_paths(),
            git.expected_drift.clone().expect("drift")
        );
        assert!(rig.save_trees().is_empty());
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
        assert!(
            backup.reads().len() >= 3,
            "it polled until the export equalled: {:?}",
            backup.reads()
        );
        assert!(
            backup.reads().iter().all(|r| r == "GET project_export"),
            "GET only"
        );
        // Both nodes were saved before the import and the events name each one.
        let events = rig.events(&result);
        let saved: Vec<&str> = events
            .iter()
            .filter(|e| e["event"] == "git_saved")
            .filter_map(|e| e["node"].as_str())
            .collect();
        assert_eq!(saved, ["nodes.master", "nodes.backup"], "{events:?}");
        let order: Vec<&str> = events.iter().filter_map(|e| e["event"].as_str()).collect();
        let import_at = order
            .iter()
            .position(|e| *e == "import_sent")
            .expect("import event");
        let backup_save_at = events
            .iter()
            .position(|e| e["event"] == "git_saved" && e["node"] == "nodes.backup")
            .expect("backup save");
        assert!(
            backup_save_at < import_at,
            "both saves precede the import: {order:?}"
        );
    }

    #[tokio::test]
    async fn pair_backup_unconverged() {
        let rig = pair();
        let backup = rig.backup_node(usize::MAX);
        let result = rig
            .apply_pair(&backup, std::time::Duration::from_millis(80))
            .await
            .expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok);
        assert_eq!(result.failure_exit_code(), Some(6));
        let git = git_of(&result);
        // The master is advanced and recorded.
        let master = node_of(git, "nodes.master")
            .restore
            .as_ref()
            .expect("master");
        assert!(master.advanced && master.reason.is_none(), "{master:?}");
        assert!(result.record_written);
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c2);
        // The backup names the reason and keeps and names its save.
        let node = node_of(git, "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert_eq!(restore.reason.as_deref(), Some("backup_unsynced"));
        assert!(restore.save_kept && !restore.advanced, "{restore:?}");
        assert!(node.save.is_some(), "the save is reported");
        assert_eq!(rig.save_trees_in(&rig.f.backup_data_dir()).len(), 1);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("backup_unsynced") && w.contains("git-save")),
            "{:?}",
            result.warnings
        );
        // The backup's repository was never written.
        assert!(rig.f.backup_git_dir().exists());
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c1);
    }

    // ---- 27-55 (P6): each backup export is bounded by the remaining wait ----

    /// A backup whose project export answers `zip` after `delay` of real time.
    /// It is the only fake that stalls: the pair rig's fake backup answers
    /// without yielding, so the bound can never cut it.
    struct SlowBackup {
        zip: Vec<u8>,
        delay: std::time::Duration,
    }

    #[async_trait::async_trait]
    impl BackupTags for SlowBackup {
        async fn tag_export(&self, _provider: &str) -> Result<Vec<u8>, CoreError> {
            Err(CoreError::Internal("no tags here".to_string()))
        }
        async fn status(
            &self,
        ) -> Result<ignition_core::client::redundancy::RedundancyStatusWire, CoreError> {
            Err(CoreError::Internal("no status here".to_string()))
        }
        async fn sync_exhausted(&self, _since_ms: i64) -> Result<bool, CoreError> {
            Ok(false)
        }
        async fn project_export(&self, _project: &str) -> Result<Vec<u8>, CoreError> {
            tokio::time::sleep(self.delay).await;
            Ok(self.zip.clone())
        }
    }

    /// The export zip and canonical digest of a small project.
    async fn small_project() -> (Vec<u8>, String) {
        use ignition_core::actions::deploy_projects::ProjectGateway as Gw;
        let gateway = FakeProjects::new().with_project(
            PROJECT,
            None,
            false,
            &[("project.json", br#"{"title":"Cap App"}"#.to_vec())],
        );
        let zip = Gw::export_bytes(&gateway, PROJECT).await.expect("export");
        let digest = canonical_digest(&zip).expect("a project export").sha256;
        (zip, digest)
    }

    fn policy(budget_ms: u64, interval_ms: u64) -> ignition_core::deploy::pair::BackupPolicy {
        ignition_core::deploy::pair::BackupPolicy {
            budget: std::time::Duration::from_millis(budget_ms),
            interval: std::time::Duration::from_millis(interval_ms),
        }
    }

    /// P6: a backup export that stalls for 10 s ends the wait at the budget
    /// plus one interval, not at the client's request timeout.
    #[tokio::test]
    async fn backup_export_stall_is_bounded() {
        use ignition_core::actions::deploy_projects::git::{BackupWait, wait_for_backup_project};
        let (zip, want) = small_project().await;
        let backup = SlowBackup {
            zip,
            delay: std::time::Duration::from_secs(10),
        };
        let wait = BackupWait {
            backup: &backup,
            policy: policy(300, 50),
        };
        let started = std::time::Instant::now();
        let outcome = wait_for_backup_project(&wait, PROJECT, &want).await;
        let took = started.elapsed();
        let detail = outcome.expect_err("a stalled export is unsynced");
        eprintln!("backup_export_stall_is_bounded: returned after {took:?}");
        assert!(
            took < std::time::Duration::from_millis(300 + 50 + 200),
            "bounded by budget + interval + slack, took {took:?}"
        );
        assert!(detail.contains("did not answer"), "{detail}");
    }

    /// P6: an answer that arrives after the budget ran out but inside the
    /// grace window (one interval) is taken: a match proves the backup holds
    /// the release.
    #[tokio::test]
    async fn backup_export_matching_answer_in_the_grace_window_is_accepted() {
        use ignition_core::actions::deploy_projects::git::{BackupWait, wait_for_backup_project};
        let (zip, want) = small_project().await;
        // 30 ms after the 300 ms budget, inside the 50 ms interval
        let backup = SlowBackup {
            zip: zip.clone(),
            delay: std::time::Duration::from_millis(330),
        };
        let wait = BackupWait {
            backup: &backup,
            policy: policy(300, 50),
        };
        let got = wait_for_backup_project(&wait, PROJECT, &want)
            .await
            .expect("a matching answer inside the grace window is accepted");
        assert_eq!(got, zip);
    }

    /// P6: an answer after budget plus interval is `backup_unsynced`, and the
    /// reason says the export did not answer in time.
    #[tokio::test]
    async fn backup_export_matching_answer_after_the_grace_window_is_unsynced() {
        use ignition_core::actions::deploy_projects::git::{BackupWait, wait_for_backup_project};
        let (zip, want) = small_project().await;
        let backup = SlowBackup {
            zip,
            delay: std::time::Duration::from_millis(600),
        };
        let wait = BackupWait {
            backup: &backup,
            policy: policy(300, 50),
        };
        let detail = wait_for_backup_project(&wait, PROJECT, &want)
            .await
            .expect_err("too late");
        assert!(
            detail.contains("did not answer") && detail.contains("wait"),
            "{detail}"
        );
    }

    /// P6 (27-55, option-b): the pair rig's fake backup answers its export in
    /// one poll, without yielding, and tokio's `timeout` polls the request
    /// before its clock, so the bound never cuts it, however loaded the box
    /// is. That is what keeps every pair test on the unchanged 400 ms `WAIT`:
    /// a zero budget and a zero interval still take the answer.
    #[tokio::test]
    async fn backup_export_of_the_pair_fake_is_never_cut_by_the_bound() {
        use ignition_core::actions::deploy_projects::ProjectGateway as Gw;
        use ignition_core::actions::deploy_projects::git::{BackupWait, wait_for_backup_project};
        let rig = Rig::tagged_pair(true);
        let want = canonical_digest(
            &Gw::export_bytes(&rig.gateway, PROJECT)
                .await
                .expect("export"),
        )
        .expect("a project export")
        .sha256;
        let backup = rig.backup_node(0);
        let wait = BackupWait {
            backup: &backup,
            policy: policy(0, 0),
        };
        wait_for_backup_project(&wait, PROJECT, &want)
            .await
            .expect("the fake's answer is taken with nothing left of the wait");
    }

    // ---- 27-55 (Q3): the backup's save outlives backup_step and its resync ----

    /// The `git_*` and backup events of a pair run, `(name, node)` in order.
    fn pair_events(rig: &Rig, result: &DeployProjectsResult) -> Vec<(String, Option<String>)> {
        rig.events(result)
            .iter()
            .filter_map(|e| {
                let name = e["event"].as_str()?;
                (name.starts_with("git_") || name.starts_with("backup_"))
                    .then(|| (name.to_string(), e["node"].as_str().map(str::to_string)))
            })
            .collect()
    }

    /// Q3: on a tag-owning pair the redundancy resync that `backup_step` may
    /// send, or the sync reaching the backup late, deletes the backup's `.git`
    /// after the settle looked at it. Before 27-55 the settle had already
    /// removed the save: the backup ended with neither. Now the save stays
    /// through `backup_step`, the recheck finds `.git` absent, restores it from
    /// the save, advances it, and only then removes the save.
    #[cfg(unix)]
    #[tokio::test]
    async fn tag_pair_backup_git_deleted_by_the_resync_is_restored() {
        let rig = Rig::tagged_pair(true);
        let mut backup = rig.backup_node_with(1, false);
        let git_dir = rig.f.backup_git_dir();
        backup.on_tag_export = Some((
            1,
            Box::new(move || {
                std::fs::remove_dir_all(&git_dir).expect("the resync deletes the backup's .git");
            }),
        ));
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert!(
            rig.f.backup_git_dir().exists(),
            "the backup must not end without a .git: {:?}",
            result.warnings
        );
        assert_eq!(
            result.outcome,
            ProjectOutcome::Applied,
            "{:?}",
            result.warnings
        );
        assert_eq!(
            rig.f.git_in_backup(&["rev-parse", "HEAD"]),
            rig.f.c2,
            "the backup's repository is advanced again"
        );
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert!(restore.restored && restore.advanced, "{restore:?}");
        assert_eq!(restore.reason, None, "{restore:?}");
        assert!(!restore.save_kept, "{restore:?}");
        assert!(
            rig.save_trees_in(&rig.f.backup_data_dir()).is_empty(),
            "the save goes only after the restore"
        );
    }

    /// Q3: with no late delete the save of a tag-owning pair's backup is
    /// removed after `backup_step` (after its resync, or after its verdict when
    /// none was needed), never before.
    #[tokio::test]
    async fn tag_pair_backup_save_removed_only_after_the_recheck() {
        let rig = Rig::tagged_pair(true);
        let backup = rig.backup_node_with(1, false);
        let result = rig.apply_pair(&backup, WAIT).await.expect("green");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Applied,
            "{:?}",
            result.warnings
        );
        let events = pair_events(&rig, &result);
        let at = |name: &str, node: Option<&str>| {
            events
                .iter()
                .position(|(n, o)| n == name && (node.is_none() || o.as_deref() == node))
        };
        let verdict = at("backup_verdict", None).expect("the backup's tag verdict");
        let removed = at("git_save_removed", Some("nodes.backup")).expect("the backup's save goes");
        assert!(
            removed > verdict,
            "the save goes after backup_step, not before: {events:?}"
        );
        if let Some(resync) = at("backup_resync", None) {
            assert!(removed > resync, "{events:?}");
        }
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
    }

    /// Q3: a `.git` changed after the resync is not the repository that was
    /// advanced: the backup ends `git_recreated`, the repository is left as it
    /// is and the save is kept and named.
    #[cfg(unix)]
    #[tokio::test]
    async fn tag_pair_backup_git_changed_after_the_resync_is_git_recreated() {
        let rig = Rig::tagged_pair(true);
        let mut backup = rig.backup_node_with(1, false);
        let work = rig.f.backup_project_dir();
        backup.on_tag_export = Some((
            1,
            Box::new(move || {
                common::git_fixture::git(
                    &work,
                    &["commit", "-q", "--allow-empty", "-m", "late writer"],
                );
            }),
        ));
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert_eq!(
            restore.reason.as_deref(),
            Some("git_recreated"),
            "{restore:?}"
        );
        assert!(restore.save_kept, "{restore:?}");
        assert_eq!(
            rig.save_trees_in(&rig.f.backup_data_dir()).len(),
            1,
            "the save is kept"
        );
        assert!(rig.f.backup_git_dir().exists(), ".git is left as it is");
    }

    /// Q3 with 27-37: the `project.json` check still runs at the save's removal
    /// point, which is now the recheck: a `project.json` the resync takes after
    /// the settle fails the node `backup_project_json_missing` and the save is
    /// kept.
    #[cfg(unix)]
    #[tokio::test]
    async fn tag_pair_backup_project_json_removed_after_the_resync_is_found() {
        let rig = Rig::tagged_pair(true);
        let mut backup = rig.backup_node_with(1, false);
        let file = backup_project_json(&rig);
        backup.on_tag_export = Some((
            1,
            Box::new(move || {
                std::fs::remove_file(&file).expect("the resync deletes project.json");
            }),
        ));
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert_eq!(
            restore.reason.as_deref(),
            Some("backup_project_json_missing"),
            "{restore:?}"
        );
        assert!(restore.save_kept, "{restore:?}");
        assert_eq!(rig.save_trees_in(&rig.f.backup_data_dir()).len(), 1);
    }

    /// Q3: a pair without owned tags keeps the 27-35 settle exactly: the save
    /// goes at the settle, and nothing reads the backup's tags afterwards (no
    /// `backup_step`, so no recheck).
    #[tokio::test]
    async fn non_tag_pair_settle_is_unchanged() {
        let rig = pair();
        let backup = rig.backup_node_with(1, false);
        let result = rig.apply_pair(&backup, WAIT).await.expect("green");
        assert!(result.ok, "{:?}", result.warnings);
        assert!(
            rig.save_trees_in(&rig.f.backup_data_dir()).is_empty(),
            "the save went at the settle"
        );
        let events = backup_git_events(&rig, &result);
        assert!(events.iter().any(|e| e == "git_save_removed"), "{events:?}");
        assert!(
            !backup.reads().iter().any(|r| r == "GET tag_export"),
            "no tag check: {:?}",
            backup.reads()
        );
    }

    // ---- 27-35: the backup's `.git` settles before its save goes (WR-08) ----

    /// A host git that runs git and then, once, after the post-advance status
    /// of the backup's work tree (its `ls-files` call), runs `change`: the
    /// redundancy sync reaching the backup's `.git` after the advance wrote
    /// and read back, before the run looks again. The hook fires once, so the
    /// advance that follows a restore is not touched. The settle never reads
    /// the backup's REST export again, so the late write cannot ride on a
    /// read count; it rides on the run's own last read of the work tree.
    #[cfg(unix)]
    fn late_backup_git(scratch: &std::path::Path, rig: &Rig, change: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let program = scratch.join("git-late");
        let once = scratch.join("late-ran");
        let work = rig.f.backup_project_dir();
        let script = format!(
            "#!/bin/sh\ngit \"$@\"\nrc=$?\ncase \"$*\" in\n  *\"--work-tree={work} ls-files\"*)\n    [ -e '{once}' ] || {{ : > '{once}'; {change}; }}\n    ;;\nesac\nexit $rc\n",
            work = work.display(),
            once = once.display(),
        );
        std::fs::write(&program, script).expect("script");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        program
    }

    /// The `git_*` events that name the backup, in order.
    fn backup_git_events(rig: &Rig, result: &DeployProjectsResult) -> Vec<String> {
        rig.events(result)
            .iter()
            .filter(|e| e["node"] == "nodes.backup")
            .filter_map(|e| e["event"].as_str())
            .filter(|n| n.starts_with("git_"))
            .map(str::to_string)
            .collect()
    }

    /// WR-08 (the finding's ordering): the redundancy sync's delete of the
    /// backup's `.git` arrives after the backup's project converged, the
    /// `.git` was advanced in place, and the run has read it back. Before the
    /// settle the save was removed on the strength of that read and the sync
    /// then took the repository: the backup had neither. Now the save stays
    /// through the advance, the `.git` is probed again one poll interval
    /// later, found absent, and restored from the save and advanced.
    #[cfg(unix)]
    #[tokio::test]
    async fn pair_backup_git_removed_after_convergence_is_restored() {
        let mut rig = pair();
        rig.backup_interval = std::time::Duration::from_millis(100);
        let scratch = tempfile::tempdir().expect("scratch");
        let program = late_backup_git(
            scratch.path(),
            &rig,
            &format!("/bin/rm -rf '{}'", rig.f.backup_git_dir().display()),
        );
        let host = rig.host_with_program(&program);
        let backup = rig.backup_node_with(1, false);
        let result = rig
            .apply_pair_on(&host, &backup, WAIT)
            .await
            .expect("the apply completes");
        assert!(
            scratch.path().join("late-ran").exists(),
            "the late delete reached the backup's .git"
        );
        assert!(
            rig.f.backup_git_dir().exists(),
            "the backup must not end without a .git: {:?}",
            result.warnings
        );
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert!(result.ok);
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert!(
            restore.restored && restore.advanced && restore.verified,
            "{restore:?}"
        );
        assert_eq!(restore.reason, None);
        assert!(!restore.save_kept, "the save goes after the restore");
        assert_eq!(restore.post_status, Some("matched"));
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
        // The save is removed only after the restore.
        let events = backup_git_events(&rig, &result);
        let at = |name: &str| {
            events
                .iter()
                .rposition(|e| e == name)
                .unwrap_or_else(|| panic!("{name} is logged: {events:?}"))
        };
        assert!(at("git_restored") < at("git_save_removed"), "{events:?}");
        assert_eq!(events.last().map(String::as_str), Some("git_save_removed"));
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("removed after the advance")),
            "{:?}",
            result.warnings
        );
    }

    /// CodeRabbit #57: a run that discards gateway commits keeps each node's
    /// save (D-05), and the backup's `.git` still settles: the redundancy
    /// sync's late delete after the advance is found one poll interval later
    /// and the repository is restored from the save and advanced, and the save
    /// stays.
    #[cfg(unix)]
    #[tokio::test]
    async fn pair_backup_discarding_run_still_settles_and_keeps_save() {
        let mut rig = pair();
        rig.discard = Some(REASON.to_string());
        rig.backup_interval = std::time::Duration::from_millis(100);
        // the same unpushed gateway commit on both nodes
        let d = rig.f.designer_commit(false);
        let master_git = rig.f.git_dir().display().to_string();
        rig.f.git_in_backup(&["fetch", "-q", &master_git, &d]);
        rig.f.git_in_backup(&["reset", "-q", "--hard", &d]);
        rig.sync_live();
        let scratch = tempfile::tempdir().expect("scratch");
        let program = late_backup_git(
            scratch.path(),
            &rig,
            &format!("/bin/rm -rf '{}'", rig.f.backup_git_dir().display()),
        );
        let host = rig.host_with_program(&program);
        let backup = rig.backup_node_with(1, false);
        let result = rig
            .apply_pair_on(&host, &backup, WAIT)
            .await
            .expect("the apply completes");
        assert!(
            scratch.path().join("late-ran").exists(),
            "the late delete ran"
        );
        assert!(
            rig.f.backup_git_dir().exists(),
            "the backup must not end without a .git: {:?}",
            result.warnings
        );
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert!(restore.restored && restore.advanced, "{restore:?}");
        assert!(
            restore.save_kept,
            "a discarding run keeps the save: {restore:?}"
        );
        assert_eq!(rig.save_trees_in(&rig.f.backup_data_dir()).len(), 1);
    }

    /// WR-08, the quiet case: the sync leaves the backup's `.git` alone. It
    /// is advanced in place, probed again after one poll interval, found equal
    /// to the advanced state, and only then is the save removed.
    #[tokio::test]
    async fn pair_backup_git_in_place_settles_then_removes_save() {
        let mut rig = pair();
        rig.backup_interval = std::time::Duration::from_millis(150);
        let backup = rig.backup_node_with(1, false);
        let started = std::time::Instant::now();
        let result = rig
            .apply_pair(&backup, WAIT)
            .await
            .expect("the apply completes");
        assert!(
            started.elapsed() >= rig.backup_interval,
            "the backup waited one poll interval before the save went: {:?}",
            started.elapsed()
        );
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert!(result.ok);
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert!(
            !restore.restored && restore.advanced && restore.verified,
            "{restore:?}"
        );
        assert!(!restore.save_kept);
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
        let events = backup_git_events(&rig, &result);
        assert_eq!(
            events,
            [
                "git_state_read",
                "git_saved",
                "git_advanced",
                "git_save_removed"
            ],
            "{events:?}"
        );
    }

    /// WR-08: the backup's `.git` is changed during the settle (the sync
    /// rewrote the repository). It is not the state that was advanced: the
    /// node fails `git_recreated`, the repository is left as it is and the
    /// save is kept and named.
    #[cfg(unix)]
    #[tokio::test]
    async fn pair_backup_git_changed_during_settle_is_left() {
        let mut rig = pair();
        rig.backup_interval = std::time::Duration::from_millis(100);
        let scratch = tempfile::tempdir().expect("scratch");
        let program = late_backup_git(
            scratch.path(),
            &rig,
            &format!(
                "printf '[late]\\n\\tchange = yes\\n' >> '{}/config'",
                rig.f.backup_git_dir().display()
            ),
        );
        let host = rig.host_with_program(&program);
        let backup = rig.backup_node_with(1, false);
        let result = rig
            .apply_pair_on(&host, &backup, WAIT)
            .await
            .expect("a verdict");
        assert!(scratch.path().join("late-ran").exists());
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok);
        assert_eq!(result.slug.as_deref(), Some(GIT_RESTORE_FAILED_SLUG));
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert_eq!(restore.reason.as_deref(), Some("git_recreated"));
        assert!(restore.save_kept, "{restore:?}");
        assert!(node.save.is_some(), "the save is reported");
        assert_eq!(rig.save_trees_in(&rig.f.backup_data_dir()).len(), 1);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("git_recreated") && w.contains("git-save")),
            "{:?}",
            result.warnings
        );
        // The repository is left as the sync made it.
        let config = std::fs::read_to_string(rig.f.backup_git_dir().join("config"))
            .expect("the backup's config");
        assert!(config.contains("change = yes"), "{config}");
        let events = backup_git_events(&rig, &result);
        assert!(
            !events.contains(&"git_save_removed".to_string()),
            "{events:?}"
        );
        assert!(
            events.contains(&"git_restore_failed".to_string()),
            "{events:?}"
        );
    }

    /// WR-08 on a `record`: the project already is the artifact, so the
    /// backup's `.git` is saved, waited for and advanced in place; the same
    /// late delete is found one poll interval later and restored.
    #[cfg(unix)]
    #[tokio::test]
    async fn pair_record_backup_settles() {
        let mut rig = pair();
        rig.backup_interval = std::time::Duration::from_millis(100);
        rig.make_live_equal_artifact().await;
        let scratch = tempfile::tempdir().expect("scratch");
        let program = late_backup_git(
            scratch.path(),
            &rig,
            &format!("/bin/rm -rf '{}'", rig.f.backup_git_dir().display()),
        );
        let host = rig.host_with_program(&program);
        let backup = rig.backup_node_with(1, false);
        let result = rig
            .apply_pair_on(&host, &backup, WAIT)
            .await
            .expect("the record completes");
        assert!(scratch.path().join("late-ran").exists());
        assert_eq!(
            result.outcome,
            ProjectOutcome::Recorded,
            "{:?}",
            result.warnings
        );
        assert!(rig.gateway.import_calls().is_empty(), "no import");
        assert!(rig.f.backup_git_dir().exists(), "{:?}", result.warnings);
        let node = node_of(git_of(&result), "nodes.backup");
        let restore = node.restore.as_ref().expect("the backup restore");
        assert!(
            restore.restored && restore.advanced && restore.verified,
            "{restore:?}"
        );
        assert!(!restore.save_kept);
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
        let events = backup_git_events(&rig, &result);
        assert_eq!(events.last().map(String::as_str), Some("git_save_removed"));
        assert!(events.contains(&"git_restored".to_string()), "{events:?}");
    }

    #[tokio::test]
    async fn pair_rollback_restores_both_unchanged() {
        let rig = Rig::tagged_pair(true);
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let before_master = rig.git_manifest().await;
        let before_backup = rig
            .f
            .backup_node()
            .git_manifest(PROJECT)
            .await
            .expect("manifest");
        let mut backup = rig.backup_node(0);
        // The sync already removed the backup's `.git` when the content changed.
        backup.drop_git_at_first_read = true;
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok);
        let git = git_of(&result);
        for name in ["nodes.master", "nodes.backup"] {
            let restore = node_of(git, name)
                .restore
                .as_ref()
                .unwrap_or_else(|| panic!("{name}"));
            assert!(restore.restored && !restore.advanced, "{name}: {restore:?}");
            assert_eq!(restore.reason, None);
            assert!(!restore.save_kept, "{name}");
        }
        assert_eq!(
            rig.git_manifest().await,
            before_master,
            "the master is as saved"
        );
        assert_eq!(
            rig.f
                .backup_node()
                .git_manifest(PROJECT)
                .await
                .expect("manifest"),
            before_backup,
            "the backup is as saved"
        );
        assert_eq!(rig.f.git_in_project(&["rev-parse", "HEAD"]), rig.f.c1);
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c1);
        assert!(rig.save_trees().is_empty());
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
    }

    /// `--rollback-run` on a pair (D-08): both repositories are saved before
    /// the undo and put back as they were once the backup's project has
    /// followed the master's rolled-back one.
    #[tokio::test]
    async fn pair_rollback_run_restores_both_unchanged() {
        let rig = Rig::tagged_pair(true);
        let backup = rig.backup_node(0);
        let applied = rig.apply_pair(&backup, WAIT).await.expect("green");
        assert_eq!(
            applied.outcome,
            ProjectOutcome::Applied,
            "{:?}",
            applied.warnings
        );
        let run_id = applied.run_id.clone().expect("a run id");
        // Both repositories were advanced by the run.
        assert_eq!(rig.f.git_in_backup(&["rev-parse", "HEAD"]), rig.f.c2);
        let before_master = rig.git_manifest().await;
        let before_backup = rig
            .f
            .backup_node()
            .git_manifest(PROJECT)
            .await
            .expect("manifest");

        let checked = rig
            .rollback_run_with(&run_id, true, Some((&backup, WAIT)))
            .await
            .expect("a check");
        assert_eq!(node_of(git_of(&checked), "nodes.backup").state, "managed");
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());

        let undo = rig
            .rollback_run_with(&run_id, false, Some((&backup, WAIT)))
            .await
            .expect("a verdict");
        assert_eq!(
            undo.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            undo.warnings
        );
        for name in ["nodes.master", "nodes.backup"] {
            let restore = node_of(git_of(&undo), name)
                .restore
                .as_ref()
                .unwrap_or_else(|| panic!("{name} restore"));
            assert!(restore.restored && !restore.advanced, "{name}: {restore:?}");
            assert!(!restore.save_kept, "{name}");
        }
        assert_eq!(rig.git_manifest().await, before_master);
        assert_eq!(
            rig.f
                .backup_node()
                .git_manifest(PROJECT)
                .await
                .expect("manifest"),
            before_backup
        );
        assert!(rig.save_trees().is_empty());
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pair_preflight_both_nodes() {
        use std::os::unix::fs::PermissionsExt;
        let rig = pair();
        let dir = rig.f.backup_project_dir();
        // Probe BEFORE the chmod (CR-D1): after the restore the directory is
        // writable again for every user, so a late probe always skipped.
        if root_guard("pair_preflight_both_nodes", &dir) {
            return;
        }
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let _unlock = Unlock(dir.clone());
        let backup = rig.backup_node(0);
        let outcome = rig.apply_pair(&backup, WAIT).await;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        let err = refusal(outcome);
        assert_eq!(preflight_checks(&err), ["git_write_access"], "{err}");
        assert!(err.to_string().contains("nodes.backup"), "{err}");
        assert_eq!(rig.run_dirs(), 0, "no run directory");
        assert!(rig.gateway.import_calls().is_empty());
        assert!(backup.reads().is_empty());
    }

    #[tokio::test]
    async fn pair_backup_rest_get_only() {
        let rig = pair();
        let backup = rig.backup_node(1);
        let result = rig
            .apply_pair(&backup, WAIT)
            .await
            .expect("the apply completes");
        assert!(result.ok, "{:?}", result.warnings);
        // The only call the run made on the backup's REST handle is a GET of
        // its project export; nothing else (and the trait has no write).
        let reads = backup.reads();
        assert!(!reads.is_empty());
        assert!(reads.iter().all(|r| r.starts_with("GET ")), "{reads:?}");
        // The source guard: the backup handle's trait has reads only.
        // LF line ends whatever the checkout gave
        let trait_src = include_str!("../src/deploy/tag_converge.rs").replace("\r\n", "\n");
        let start = trait_src.find("pub trait BackupTags").expect("trait");
        let end = start + trait_src[start..].find("\n}\n").expect("end of trait");
        for write in ["post", "put", "delete", "import", "resync", "write"] {
            assert!(
                !trait_src[start..end]
                    .to_lowercase()
                    .contains(&format!("fn {write}")),
                "the backup handle has a write method: {write}"
            );
        }
    }

    // ---- 27-34: WR-01, WR-02, WR-03, WR-10 ----

    /// A git writer that does everything the local one does except remove a
    /// save: the cleanup that fails (a lock on a pack, a read-only entry).
    struct KeepsSaves(LocalFsTransport);

    #[async_trait::async_trait]
    impl GitStateWriter for KeepsSaves {
        async fn save_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.save_git(gateway, project, run_id).await
        }
        async fn restore_git(
            &self,
            gateway: &str,
            project: &str,
            run_id: &str,
        ) -> Result<ignition_core::deploy::transport::projects::GitManifest, CoreError> {
            self.0.restore_git(gateway, project, run_id).await
        }
        async fn write_git_files(&self, p: &str, w: &[GitWrite]) -> Result<(), CoreError> {
            self.0.write_git_files(p, w).await
        }
        async fn remove_git_save(&self, _: &str, _: &str, _: &str) -> Result<(), CoreError> {
            Err(CoreError::TransportFailed {
                operation: "remove".to_string(),
                node: "nodes.master".to_string(),
                detail: "a pack is locked".to_string(),
            })
        }
        async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
            self.0.probe_write(project, run_id).await
        }
    }

    /// Files whose name starts with `journal` below the ledger root of
    /// `data`.
    fn journals_in(data: &std::path::Path) -> Vec<PathBuf> {
        fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("journal"))
                {
                    out.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(&data.join("ign-deploy"), &mut found);
        found
    }

    /// Where the saves of the backup node would go: a regular file there makes
    /// every save on that node fail.
    fn block_saves_on_backup(rig: &Rig) {
        let dir = rig
            .f
            .backup_data_dir()
            .join("ign-deploy/capgw/projects")
            .join(PROJECT);
        std::fs::create_dir_all(&dir).expect("the ledger directory");
        std::fs::write(dir.join("git-save"), b"in the way").expect("the blocker");
    }

    /// WR-01 (D-22): `--rollback-run` proves the deploy account can write the
    /// project directory before its first save, journal write or import. A
    /// read-only project directory refuses `deploy_preflight_failed` with
    /// nothing saved, journaled or imported.
    #[cfg(unix)]
    #[tokio::test]
    async fn rollback_run_preflight_refuses_before_save() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::tagged(true);
        if root_guard(
            "rollback_run_preflight_refuses_before_save",
            &rig.f.project_dir(),
        ) {
            return;
        }
        let applied = rig.apply().await.expect("green");
        let run_id = applied.run_id.clone().expect("a run id");
        let imports = rig.gateway.import_calls().len();
        let runs = rig.run_dirs();
        let journals = journals_in(&rig.f.data_dir);
        std::fs::set_permissions(rig.f.project_dir(), std::fs::Permissions::from_mode(0o555))
            .expect("chmod");
        let _unlock = Unlock(rig.f.project_dir());
        let err = refusal(rig.rollback_run(&run_id, false).await);
        let failures = preflight_details(&err);
        assert_eq!(failures.len(), 1, "{err}");
        assert_eq!(failures[0].0, "git_write_access", "{err}");
        assert!(
            failures[0].1.contains("projects/CapApp"),
            "{}",
            failures[0].1
        );
        assert_eq!(rig.gateway.import_calls().len(), imports, "no import");
        assert!(rig.save_trees().is_empty(), "nothing saved");
        assert_eq!(journals_in(&rig.f.data_dir), journals, "no journal change");
        assert_eq!(rig.run_dirs(), runs, "no run directory");
    }

    /// WR-01 on a pair: the backup's project directory is probed too, and the
    /// refusal names `nodes.backup`. Nothing is saved on either node.
    #[cfg(unix)]
    #[tokio::test]
    async fn rollback_run_preflight_refuses_on_the_backup() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::tagged_pair(true);
        let dir = rig.f.backup_project_dir();
        if root_guard("rollback_run_preflight_refuses_on_the_backup", &dir) {
            return;
        }
        let backup = rig.backup_node(0);
        let applied = rig.apply_pair(&backup, WAIT).await.expect("green");
        let run_id = applied.run_id.clone().expect("a run id");
        let imports = rig.gateway.import_calls().len();
        let journals = journals_in(&rig.f.data_dir);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
        let _unlock = Unlock(dir.clone());
        let outcome = rig
            .rollback_run_with(&run_id, false, Some((&backup, WAIT)))
            .await;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
        let err = refusal(outcome);
        assert_eq!(preflight_checks(&err), ["git_write_access"], "{err}");
        assert!(err.to_string().contains("nodes.backup"), "{err}");
        assert_eq!(rig.gateway.import_calls().len(), imports, "no import");
        assert!(rig.save_trees().is_empty(), "nothing saved on the master");
        assert!(rig.save_trees_in(&rig.f.backup_data_dir()).is_empty());
        assert_eq!(journals_in(&rig.f.data_dir), journals, "no journal change");
    }

    /// WR-01: too little free space refuses `git_free_space` before any save.
    #[tokio::test]
    async fn rollback_run_free_space_refuses_before_save() {
        let rig = Rig::tagged(true);
        let applied = rig.apply().await.expect("green");
        let run_id = applied.run_id.clone().expect("a run id");
        let imports = rig.gateway.import_calls().len();
        let journals = journals_in(&rig.f.data_dir);
        let (host, _calls) = metered(&rig, Some(1_048_576));
        let err = refusal(rig.rollback_run_on(&host, &run_id, false, None, None).await);
        let failures = preflight_details(&err);
        assert_eq!(failures.len(), 1, "{err}");
        assert_eq!(failures[0].0, "git_free_space", "{err}");
        assert!(
            failures[0].1.contains("1048576"),
            "free bytes named: {}",
            failures[0].1
        );
        assert_eq!(rig.gateway.import_calls().len(), imports, "no import");
        assert!(rig.save_trees().is_empty(), "nothing saved");
        assert_eq!(journals_in(&rig.f.data_dir), journals, "no journal change");
        assert_no_probe_left(&rig);
    }

    /// WR-02 (D-06): the gateway refuses the import. The `.git` was never
    /// touched, so the saves taken before it, the master's and the backup's,
    /// are removed and both repositories are as they were.
    #[tokio::test]
    async fn refused_import_removes_the_git_saves() {
        let rig = Rig::with(|g| g.with_project_import_denied());
        rig.add_backup();
        let before_master = rig.git_manifest().await;
        let before_backup = rig.f.backup_node().git_manifest(PROJECT).await.expect("m");
        let backup = rig.backup_node(0);
        let err = refusal(rig.apply_pair(&backup, WAIT).await);
        assert_eq!(err.code(), "import_denied", "{err}");
        assert_eq!(rig.gateway.import_calls().len(), 1, "the import was tried");
        assert!(rig.save_trees().is_empty(), "the master's save is gone");
        assert!(
            rig.save_trees_in(&rig.f.backup_data_dir()).is_empty(),
            "the backup's save is gone"
        );
        assert_eq!(rig.git_manifest().await, before_master);
        assert_eq!(
            rig.f.backup_node().git_manifest(PROJECT).await.expect("m"),
            before_backup
        );
        assert!(
            !err.to_string().contains("git-save"),
            "nothing is left to name: {err}"
        );
    }

    /// WR-02: the same on a master alone.
    #[tokio::test]
    async fn refused_import_removes_the_master_save() {
        let rig = Rig::with(|g| g.with_project_import_denied());
        let before = rig.git_manifest().await;
        let err = refusal(rig.apply().await);
        assert_eq!(err.code(), "import_denied", "{err}");
        assert!(rig.save_trees().is_empty(), "the save is gone");
        assert_eq!(rig.git_manifest().await, before);
    }

    /// WR-02: the backup's save fails after the master's succeeded. The error
    /// names `nodes.backup` and the master's save is removed.
    #[tokio::test]
    async fn failed_backup_save_removes_the_master_save() {
        let rig = pair();
        block_saves_on_backup(&rig);
        let before = rig.git_manifest().await;
        let backup = rig.backup_node(0);
        let err = refusal(rig.apply_pair(&backup, WAIT).await);
        assert!(err.to_string().contains("nodes.backup"), "{err}");
        assert!(rig.gateway.import_calls().is_empty(), "nothing imported");
        assert!(rig.save_trees().is_empty(), "the master's save is gone");
        assert_eq!(rig.git_manifest().await, before);
    }

    /// WR-02: a save the run cannot remove is named in the error, with the
    /// node, and the error keeps its slug and its exit.
    #[tokio::test]
    async fn unremovable_save_is_named_in_the_error() {
        let rig = pair();
        block_saves_on_backup(&rig);
        let host = rig.host();
        let writer = KeepsSaves(rig.f.node());
        let backup = rig.backup_node(0);
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let err = refusal(
            rig.run_prepared_full(
                &host,
                &loaded,
                prepared,
                Some(&writer),
                Default::default(),
                Some((&backup, WAIT)),
            )
            .await,
        );
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save stays: {trees:?}");
        let save = trees[0]
            .strip_prefix(&rig.f.data_dir)
            .expect("below the data dir")
            .to_string_lossy()
            // ign names a host path with `/` on every platform
            .replace('\\', "/");
        let text = err.to_string();
        assert!(text.contains(&save), "the save path is named: {text}");
        assert!(text.contains("nodes.master"), "the node is named: {text}");
        assert_eq!(err.code(), "transport_failed", "the slug stands: {text}");
        assert_eq!(err.exit_code(), 4, "the exit stands: {text}");
        assert!(
            text.contains("nodes.backup"),
            "the first cause stays: {text}"
        );
    }

    /// WR-02: a refused import whose save cannot be removed names the save in
    /// the import's own error, which keeps its slug.
    #[tokio::test]
    async fn unremovable_save_after_a_refused_import_is_named() {
        let rig = Rig::with(|g| g.with_project_import_denied());
        let host = rig.host();
        let writer = KeepsSaves(rig.f.node());
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let err = refusal(
            rig.run_prepared(&host, &loaded, prepared, Some(&writer))
                .await,
        );
        assert_eq!(err.code(), "import_denied", "{err}");
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save stays");
        let save = trees[0]
            .strip_prefix(&rig.f.data_dir)
            .expect("below the data dir")
            .to_string_lossy()
            // ign names a host path with `/` on every platform
            .replace('\\', "/");
        assert!(err.to_string().contains(&save), "{err}");
    }

    /// WR-02: `--rollback-run` takes its saves before the journal write. A
    /// journal the host cannot write stops the undo before any import, and the
    /// saves it took are removed (the run's events log both).
    #[tokio::test]
    async fn rollback_run_journal_failure_removes_the_saves() {
        let rig = Rig::tagged_pair(true);
        let backup = rig.backup_node(0);
        let applied = rig.apply_pair(&backup, WAIT).await.expect("green");
        let run_id = applied.run_id.clone().expect("a run id");
        let imports = rig.gateway.import_calls().len();
        let before = rig.git_manifest().await;
        // A directory where the journal file goes: the write cannot land.
        let journal = rig
            .f
            .data_dir
            .join("ign-deploy/capgw/projects")
            .join(PROJECT)
            .join("journal.json");
        std::fs::create_dir_all(&journal).expect("a directory in the journal's place");
        let err = refusal(
            rig.rollback_run_with(&run_id, false, Some((&backup, WAIT)))
                .await,
        );
        assert_eq!(rig.gateway.import_calls().len(), imports, "no import");
        assert!(
            rig.save_trees().is_empty(),
            "the master's save is gone: {err}"
        );
        assert!(
            rig.save_trees_in(&rig.f.backup_data_dir()).is_empty(),
            "the backup's save is gone: {err}"
        );
        assert_eq!(rig.git_manifest().await, before);
        // The undo's own run directory is the newest; it logged both saves and
        // both removals.
        let mut runs: Vec<PathBuf> = std::fs::read_dir(rig.repo.join(".ign/deploy/capgw"))
            .expect("run dirs")
            .flatten()
            .map(|e| e.path())
            .collect();
        runs.sort();
        let events =
            std::fs::read_to_string(runs.last().expect("the undo's run").join("events.ndjson"))
                .expect("events");
        assert_eq!(events.matches("\"git_saved\"").count(), 2, "{events}");
        assert_eq!(
            events.matches("\"git_save_removed\"").count(),
            2,
            "{events}"
        );
    }

    /// WR-02: the import request failed in a way that may have reached the
    /// gateway (the answer was lost after it applied, and the module recreated
    /// a different `.git`). The save is the only copy of the old repository:
    /// it stays, and the run names it.
    #[tokio::test]
    async fn uncertain_import_keeps_the_save() {
        let rig = Rig::with(|g| {
            g.with_disk_mirror_recreates_git()
                .with_project_import_answer_lost()
        });
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save is kept");
        let node = git_of(&result).master();
        let save = node.save.as_ref().expect("the save is named");
        assert!(rig.f.data_dir.join(&save.path).is_dir());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("may have reached the gateway")),
            "{:?}",
            result.warnings
        );
    }

    /// WR-02: an import that never left the host (no answer, nothing applied)
    /// is still an unknown outcome. The run reads the project back and the
    /// save is not discarded on the strength of the error.
    #[tokio::test]
    async fn lost_import_is_read_back_not_discarded() {
        let rig = Rig::with(|g| g.with_project_import_lost());
        let result = rig.apply().await.expect("the run ends with a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::ImportedUnverified,
            "{:?}",
            result.warnings
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("may have reached the gateway")),
            "{:?}",
            result.warnings
        );
    }

    /// WR-03 (D-08): the rollback put `.git` back equal to the save and only
    /// the removal of the saved copy failed. The rollback stays verified; the
    /// failure is a warning that names the copy, and no `left` line says the
    /// repository differs.
    #[tokio::test]
    async fn rollback_save_cleanup_failure_is_a_warning() {
        let rig = Rig::tagged_with(true, |g| g.with_project_import_denied());
        let host = rig.host();
        let writer = KeepsSaves(rig.f.node());
        let before = rig.git_manifest().await;
        let (loaded, prepared) = rig.plan_with(&host).await.expect("prepare");
        let result = rig
            .run_prepared(&host, &loaded, prepared, Some(&writer))
            .await
            .expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        assert_eq!(rig.git_manifest().await, before, ".git is as saved");
        let restore = git_of(&result)
            .master()
            .restore
            .clone()
            .expect("restore reported");
        assert!(restore.save_kept, "{restore:?}");
        assert!(restore.reason.is_none(), "{restore:?}");
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the copy is still there");
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("could not be removed") && w.contains("git-save")),
            "a warning names the copy: {:?}",
            result.warnings
        );
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.contains("not the one saved")),
            "{:?}",
            result.warnings
        );
    }

    /// WR-03: a `.git` that differs from the save (the module recreated one
    /// during the rollback's own import) is still a `left` line and
    /// `rollback_failed`, with the save kept and named. Unchanged by WR-03.
    #[tokio::test]
    async fn rollback_differing_repo_is_left() {
        let rig = Rig::tagged_with(true, |g| g.with_disk_mirror_recreates_git());
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let err = refusal(rig.apply().await);
        let CoreError::RollbackFailed(failure) = &err else {
            panic!("the rollback must end rollback_failed: {err:?}");
        };
        assert_eq!(err.code(), "rollback_failed");
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save is kept");
        let save = trees[0]
            .strip_prefix(&rig.f.data_dir)
            .expect("below the data dir")
            .to_string_lossy()
            // ign names a host path with `/` on every platform
            .replace('\\', "/");
        assert!(
            failure.cause.contains(&save) && failure.cause.contains("not the one saved"),
            "{}",
            failure.cause
        );
    }

    /// WR-10: a tag-owning pair run whose git step ended `git_restore_failed`
    /// still checks the backup's owned tags and reports them in
    /// `data.tags.backup`, and the git verdict stands.
    #[tokio::test]
    async fn pair_tag_backup_reported_after_git_restore_failed() {
        // The backup's project never follows, so its repository stays unsynced
        // and the git step ends `git_restore_failed`. Its tags do not hold.
        let rig = Rig::tagged_pair(true);
        let mut backup = rig.backup_node(usize::MAX);
        backup.tags_differ = true;
        let result = rig
            .apply_pair(&backup, std::time::Duration::from_millis(80))
            .await
            .expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        assert_eq!(result.slug.as_deref(), Some(GIT_RESTORE_FAILED_SLUG));
        let report = result
            .tags
            .as_ref()
            .and_then(|t| t.backup.as_ref())
            .expect("data.tags.backup is reported");
        assert!(!report.converged, "{report:?}");
        assert!(report.reason.is_some(), "{report:?}");

        // When the tags converge the report says so, and the verdict is the
        // same.
        let rig = Rig::tagged_pair(true);
        let backup = rig.backup_node(usize::MAX);
        let result = rig
            .apply_pair(&backup, std::time::Duration::from_millis(80))
            .await
            .expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        let report = result
            .tags
            .as_ref()
            .and_then(|t| t.backup.as_ref())
            .expect("data.tags.backup is reported");
        assert!(report.converged, "{report:?}");
    }

    // ---- 27-37: the backup's project.json is checked after a pair deploy ----

    /// The detection word of 27-26 (checkpoint option-a).
    const PROJECT_JSON_WORD: &str = "backup_project_json_missing";

    fn backup_project_json(rig: &Rig) -> PathBuf {
        rig.f.backup_project_dir().join("project.json")
    }

    /// What every loud finding says: the consequence, the owner of the defect
    /// and what to do. Never a `.git` restore instruction.
    fn assert_project_json_warning(result: &DeployProjectsResult, what: &str) {
        let line = result
            .warnings
            .iter()
            .find(|w| w.contains("project.json"))
            .unwrap_or_else(|| panic!("a project.json warning: {:?}", result.warnings));
        for needle in [
            what,
            "will lose project CapApp on its next restart",
            "git module and redundancy project-sync defect",
            "ign does not repair",
            "before the backup restarts",
        ] {
            assert!(line.contains(needle), "{needle:?} is missing from {line}");
        }
        assert!(!line.contains("restore"), "no restore instruction: {line}");
    }

    /// The backup node of `result`, which the finding failed.
    fn assert_backup_project_json_failure(rig: &Rig, result: &DeployProjectsResult) {
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok);
        assert_eq!(result.slug.as_deref(), Some(GIT_RESTORE_FAILED_SLUG));
        assert_eq!(result.failure_exit_code(), Some(6));
        let git = git_of(result);
        let backup = node_of(git, "nodes.backup")
            .restore
            .as_ref()
            .expect("the backup restore");
        assert_eq!(backup.reason.as_deref(), Some(PROJECT_JSON_WORD));
        assert!(backup.save_kept, "the save is kept: {backup:?}");
        assert_eq!(
            rig.save_trees_in(&rig.f.backup_data_dir()).len(),
            1,
            "the backup's save is on disk"
        );
        // The master is green and unchanged.
        let master = node_of(git, "nodes.master")
            .restore
            .as_ref()
            .expect("the master restore");
        assert!(master.advanced && master.verified, "{master:?}");
        assert_eq!(master.reason, None);
        assert!(!master.save_kept);
        assert!(rig.save_trees().is_empty());
        // The failure is an event naming the backup, its reason and its save.
        let failed: Vec<serde_json::Value> = rig
            .events(result)
            .into_iter()
            .filter(|e| e["event"] == "git_restore_failed")
            .collect();
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0]["node"], "nodes.backup");
        assert_eq!(failed[0]["reason"], PROJECT_JSON_WORD);
        assert!(failed[0]["path"].is_string(), "{failed:?}");
        // ign repairs nothing.
        assert!(
            std::fs::symlink_metadata(backup_project_json(rig))
                .map(|m| !m.file_type().is_file())
                .unwrap_or(true),
            "project.json on the backup was not put back"
        );
    }

    /// Debug B8: the redundancy sync deletes the backup's `project.json` after
    /// the master's import. The run fails loudly with the decided outcome.
    #[tokio::test]
    async fn pair_backup_project_json_absent_fails_loudly() {
        let rig = pair();
        let backup = rig.backup_node_losing(
            1,
            true,
            Some(common::fake_projects::ProjectJsonFate::Absent),
        );
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_backup_project_json_failure(&rig, &result);
        assert_project_json_warning(&result, "is absent after the deploy");
        assert!(!backup_project_json(&rig).exists());
    }

    /// The 27-37 detection under the 27-40 knob: the backup's import rewrites
    /// only the changed resources and the redundancy sync still deletes its
    /// `project.json`. The run fails loudly with the same decided outcome.
    #[tokio::test]
    async fn backup_project_json_absent_still_fails_loudly() {
        let rig = pair();
        let mut backup = rig.backup_node_losing(
            1,
            true,
            Some(common::fake_projects::ProjectJsonFate::Absent),
        );
        let gw = std::mem::take(&mut backup.gw);
        backup.gw = gw.with_disk_mirror_rewrites_changed_only();
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_backup_project_json_failure(&rig, &result);
        assert_project_json_warning(&result, "is absent after the deploy");
        assert!(!backup_project_json(&rig).exists());
    }

    /// Debug B4: the path is an empty directory on the backup.
    #[tokio::test]
    async fn pair_backup_project_json_directory_fails_loudly() {
        let rig = pair();
        let backup = rig.backup_node_losing(
            1,
            true,
            Some(common::fake_projects::ProjectJsonFate::Directory),
        );
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_backup_project_json_failure(&rig, &result);
        assert_project_json_warning(&result, "is a directory after the deploy");
        assert!(backup_project_json(&rig).is_dir());
    }

    /// Nothing else changed: a backup that keeps its `project.json` is green
    /// and says nothing about it.
    #[tokio::test]
    async fn pair_backup_project_json_present_is_green() {
        let rig = pair();
        let backup = rig.backup_node(1);
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::Imported,
            "{:?}",
            result.warnings
        );
        assert!(result.ok && result.slug.is_none());
        assert!(backup_project_json(&rig).is_file());
        assert!(
            result.warnings.iter().all(|w| !w.contains("project.json")),
            "{:?}",
            result.warnings
        );
        let restore = node_of(git_of(&result), "nodes.backup")
            .restore
            .as_ref()
            .expect("the backup restore");
        assert_eq!(restore.reason, None);
        assert!(!restore.save_kept);
    }

    /// The finding is a read. ign does not hide the deletion: `project.json`
    /// stays tracked in the index it wrote, with no assume-unchanged bit.
    #[tokio::test]
    async fn pair_backup_project_json_keeps_the_index_honest() {
        let rig = pair();
        let backup = rig.backup_node_losing(
            1,
            true,
            Some(common::fake_projects::ProjectJsonFate::Absent),
        );
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_backup_project_json_failure(&rig, &result);
        let listing = rig.f.git_in_backup(&["ls-files", "-v"]);
        let line = listing
            .lines()
            .find(|l| l.ends_with("project.json"))
            .unwrap_or_else(|| panic!("project.json is tracked on the backup: {listing}"));
        assert!(
            line.starts_with("H "),
            "tracked, not assume-unchanged (lowercase) or skip-worktree (S): {line}"
        );
        let status = rig.f.git_in_backup(&["status", "--porcelain"]);
        assert!(
            status.lines().any(|l| l.contains("project.json")),
            "git still sees the deletion: {status}"
        );
    }

    /// The post-advance status of a local backup flags `project.json` and
    /// nothing else: the reason is the detection word, and the status words
    /// stay as measured. When another path differs too, the reason stays the
    /// status mismatch and the finding is a warning.
    #[tokio::test]
    async fn pair_backup_post_status_only_project_json_takes_the_detection_word() {
        let rig = pair();
        let backup = rig.backup_node_losing(
            1,
            true,
            Some(common::fake_projects::ProjectJsonFate::Absent),
        );
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_backup_project_json_failure(&rig, &result);
        let restore = node_of(git_of(&result), "nodes.backup")
            .restore
            .as_ref()
            .expect("the backup restore");
        assert_eq!(restore.post_status, Some("mismatch"));
        let note = restore.post_status_note.as_deref().expect("a note");
        assert!(note.contains("project.json"), "{note}");
        assert!(restore.advanced && restore.verified, "{restore:?}");

        // Another path differs as well: the status mismatch keeps the reason.
        let rig = pair();
        let mut backup = rig.backup_node_losing(
            1,
            true,
            Some(common::fake_projects::ProjectJsonFate::Absent),
        );
        let gw = std::mem::take(&mut backup.gw);
        backup.gw = gw.with_disk_mirror_extra_on_disk("stray.txt");
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::GitRestoreFailed,
            "{:?}",
            result.warnings
        );
        let restore = node_of(git_of(&result), "nodes.backup")
            .restore
            .as_ref()
            .expect("the backup restore");
        assert_eq!(restore.reason.as_deref(), Some("post_status_mismatch"));
        assert_eq!(restore.post_status, Some("mismatch"));
        assert!(restore.save_kept);
        assert_project_json_warning(&result, "is absent after the deploy");
    }

    /// The sync reaches `project.json` after the post-advance status was read
    /// (the late window of WR-08): the settle's second look finds it.
    #[cfg(unix)]
    #[tokio::test]
    async fn pair_backup_project_json_removed_late_fails_after_settle() {
        let mut rig = pair();
        rig.backup_interval = std::time::Duration::from_millis(100);
        let scratch = tempfile::tempdir().expect("scratch");
        let program = late_backup_git(
            scratch.path(),
            &rig,
            &format!("/bin/rm -f '{}'", backup_project_json(&rig).display()),
        );
        let host = rig.host_with_program(&program);
        let backup = rig.backup_node_with(1, false);
        let result = rig
            .apply_pair_on(&host, &backup, WAIT)
            .await
            .expect("a verdict");
        assert!(scratch.path().join("late-ran").exists());
        assert_backup_project_json_failure(&rig, &result);
        let restore = node_of(git_of(&result), "nodes.backup")
            .restore
            .as_ref()
            .expect("the backup restore");
        assert_eq!(restore.post_status, Some("matched"));
        assert_project_json_warning(&result, "is absent after the deploy");
    }

    /// A `record` (no import ran) checks the backup's `project.json` too.
    #[cfg(unix)]
    #[tokio::test]
    async fn pair_record_checks_backup_project_json() {
        let mut rig = pair();
        rig.backup_interval = std::time::Duration::from_millis(100);
        rig.make_live_equal_artifact().await;
        let scratch = tempfile::tempdir().expect("scratch");
        let program = late_backup_git(
            scratch.path(),
            &rig,
            &format!("/bin/rm -f '{}'", backup_project_json(&rig).display()),
        );
        let host = rig.host_with_program(&program);
        let backup = rig.backup_node_with(1, false);
        let result = rig
            .apply_pair_on(&host, &backup, WAIT)
            .await
            .expect("a verdict");
        assert!(scratch.path().join("late-ran").exists());
        assert!(rig.gateway.import_calls().is_empty(), "no import");
        assert_backup_project_json_failure(&rig, &result);
        assert_project_json_warning(&result, "is absent after the deploy");
    }

    /// The automatic rollback keeps its outcome and says the backup lost its
    /// `project.json`.
    #[tokio::test]
    async fn pair_rollback_warns_backup_project_json() {
        let rig = Rig::tagged_pair(true);
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let mut backup = rig.backup_node(0);
        backup.drop_git_at_first_read = true;
        backup.drop_project_json_at_first_read = true;
        let result = rig.apply_pair(&backup, WAIT).await.expect("a verdict");
        assert_eq!(
            result.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            result.warnings
        );
        assert!(!result.ok);
        assert_project_json_warning(&result, "is absent after the deploy");
        let restore = node_of(git_of(&result), "nodes.backup")
            .restore
            .as_ref()
            .expect("the backup restore");
        assert!(restore.restored && !restore.advanced, "{restore:?}");
        assert_eq!(restore.reason, None, "a rollback keeps its reasons");
    }

    /// `--rollback-run` does the same.
    #[tokio::test]
    async fn rollback_run_pair_warns_backup_project_json() {
        let rig = Rig::tagged_pair(true);
        let green = rig.backup_node(0);
        let applied = rig.apply_pair(&green, WAIT).await.expect("green");
        assert_eq!(
            applied.outcome,
            ProjectOutcome::Applied,
            "{:?}",
            applied.warnings
        );
        let run_id = applied.run_id.clone().expect("a run id");
        let losing = rig.backup_node_losing(
            0,
            true,
            Some(common::fake_projects::ProjectJsonFate::Absent),
        );
        let undo = rig
            .rollback_run_with(&run_id, false, Some((&losing, WAIT)))
            .await
            .expect("a verdict");
        assert_eq!(
            undo.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            undo.warnings
        );
        assert_project_json_warning(&undo, "is absent after the deploy");
    }

    /// The probe on its own: a regular file is fine; absent, a directory, a
    /// link and a probe that fails are each a finding, and a failed probe is
    /// never reported as present.
    #[tokio::test]
    async fn backup_project_json_probe_names_each_kind() {
        use ignition_core::actions::deploy_projects::git::check_backup_project_json;
        let rig = pair();
        let node = rig.f.backup_node();
        let file = backup_project_json(&rig);
        assert_eq!(check_backup_project_json(&node, PROJECT).await, None);
        std::fs::remove_file(&file).expect("rm");
        let absent = check_backup_project_json(&node, PROJECT).await;
        assert_eq!(absent.as_deref(), Some("is absent"));
        std::fs::create_dir(&file).expect("dir");
        let dir = check_backup_project_json(&node, PROJECT).await;
        assert_eq!(dir.as_deref(), Some("is a directory"));
        std::fs::remove_dir(&file).expect("rmdir");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hostname", &file).expect("link");
            let link = check_backup_project_json(&node, PROJECT).await;
            assert_eq!(link.as_deref(), Some("is a link"));
            std::fs::remove_file(&file).expect("rm link");
            // `projects` itself is a link: the probe fails, and that is a
            // finding, never "present".
            let projects = rig.f.backup_data_dir().join("projects");
            let moved = rig.f.backup_data_dir().join("projects-real");
            std::fs::rename(&projects, &moved).expect("rename");
            std::os::unix::fs::symlink(&moved, &projects).expect("link");
            let failed = check_backup_project_json(&node, PROJECT)
                .await
                .expect("a failed probe is a finding");
            assert!(failed.starts_with("could not be checked ("), "{failed}");
        }
    }
    // ---- 27-53 (Q2): the import flag of the host copy, with a gateway .git ----

    /// The host `run.json` of the rig's project, as JSON.
    fn host_run_json(rig: &Rig) -> serde_json::Value {
        let path = rig
            .f
            .data_dir
            .join("ign-deploy/capgw/projects")
            .join(PROJECT)
            .join("pre-image/run.json");
        serde_json::from_slice(&std::fs::read(path).expect("run.json")).expect("json")
    }

    /// Q2: a host that refuses the flag rewrite stops the run before the
    /// import: nothing is imported, the tags roll back, the `.git` save goes
    /// and the repository is as it was.
    #[tokio::test]
    async fn import_flag_write_failure_stops_before_the_import() {
        let mut rig = Rig::tagged(true);
        rig.fail_import_flag = true;
        let before = rig.git_manifest().await;

        let result = rig.apply().await.expect("a rolled-back verdict");
        assert!(
            rig.gateway.import_calls().is_empty(),
            "no import: {:?}",
            rig.gateway.call_log()
        );
        assert!(!result.ok, "{:?}", result.warnings);
        assert!(
            format!("{result:?}").contains("the host refused the import flag"),
            "the host's error is the cause: {result:?}"
        );
        assert_eq!(
            rig.git_events(&result),
            ["git_state_read", "git_saved", "git_save_removed"],
            "the save is taken, then discarded"
        );
        assert!(rig.save_trees().is_empty(), "no save is left");
        assert_eq!(rig.git_manifest().await, before, ".git is untouched");
        assert_eq!(host_run_json(&rig)["import_sent"], false);
    }

    /// Q2 (CodeRabbit #57 round 2): a tag-owning run on a git-managed project
    /// is refused before its import (a Designer saved project work at the last
    /// check) and its tag rollback fails. The retry restores only the tags:
    /// it never imports the pre-image over the Designer's save, so it never
    /// removes the gateway `.git`.
    #[tokio::test]
    async fn rollback_run_after_a_pre_import_refusal_never_imports_or_strips_git() {
        let session = ("alice".to_string(), PROJECT.to_string(), "ws-1".to_string());
        let rig = Rig::tagged_with(true, |g| {
            g.with_designers_script(vec![vec![], vec![session], vec![]])
                .with_project_edited_on_designers_read(1, PROJECT)
                .with_tag_import_failure(2)
        });
        let before = rig.git_manifest().await;
        let err = rig.apply().await.expect_err("the tag rollback failed");
        assert_eq!(err.code(), "rollback_failed", "{err}");
        assert!(rig.gateway.import_calls().is_empty(), "no import was sent");
        let run_id = host_run_json(&rig)["run_id"]
            .as_str()
            .expect("a run id")
            .to_string();
        assert_eq!(host_run_json(&rig)["import_sent"], false);
        assert_eq!(rig.git_manifest().await, before, ".git is untouched");

        // CodeRabbit #61: the check makes the apply's project decision. The
        // live project is the Designer's save, not the pre-image, and the run
        // never sent its import, so the undo imports nothing and the check
        // says so.
        let check = rig.rollback_run(&run_id, true).await.expect("a check");
        let rollback = check
            .tags
            .as_ref()
            .and_then(|t| t.rollback.as_ref())
            .expect("data.tags.rollback");
        assert!(
            !rollback.project_reimported,
            "no project reimport is previewed: {:?}",
            check.warnings
        );
        assert!(
            rig.gateway.import_calls().is_empty(),
            "a check imports nothing"
        );

        let undo = rig.rollback_run(&run_id, false).await.expect("a verdict");
        assert!(
            rig.gateway.import_calls().is_empty(),
            "the retry imports no pre-image: {:?}",
            rig.gateway.call_log()
        );
        assert_eq!(
            rig.git_manifest().await,
            before,
            ".git is untouched: {:?}",
            undo.warnings
        );
        assert!(rig.save_trees().is_empty(), "no save is left");
        assert_eq!(
            undo.outcome,
            ProjectOutcome::TagsRolledBack,
            "{:?}",
            undo.warnings
        );
    }
    // ---- 27-54 (P3): a save a rollback could not put back is pending ----

    /// The host journal of the rig's project, as JSON, when there is one.
    fn host_journal_json(rig: &Rig) -> Option<serde_json::Value> {
        let path = rig
            .f
            .data_dir
            .join("ign-deploy/capgw/projects")
            .join(PROJECT)
            .join("journal.json");
        let bytes = std::fs::read(path).ok()?;
        Some(serde_json::from_slice(&bytes).expect("a journal"))
    }

    /// A tag-owning managed deploy that fails after its import (a child
    /// changed behind the run) with a Designer attached throughout: the
    /// automatic rollback re-imports the pre-image, which removes `.git`, and
    /// cannot put the save back. Returns the rig, the manifest of the `.git`
    /// before the run and the run's id.
    async fn rollback_stuck_behind_a_designer() -> (
        Rig,
        ignition_core::deploy::transport::projects::GitManifest,
        String,
    ) {
        let rig = Rig::tagged_with(true, |g| g.with_designers_after_exports(3, 100_000));
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let before = rig.git_manifest().await;
        let err = rig
            .apply_designers(wait_ms(60))
            .await
            .expect_err("the rollback ends rollback_failed");
        assert_eq!(err.code(), "rollback_failed", "{err}");
        assert!(!rig.f.git_dir().exists(), ".git is still absent");
        let journal = host_journal_json(&rig).expect("the journal stays");
        let run_id = journal["run_id"].as_str().expect("a run id").to_string();
        (rig, before, run_id)
    }

    /// P3: the rollback that leaves a save names it in the host journal (state
    /// `rolling_back`): node, the save's run id, path and manifest.
    #[tokio::test]
    async fn rollback_with_a_designer_attached_writes_git_pending() {
        let (rig, before, run_id) = rollback_stuck_behind_a_designer().await;
        let journal = host_journal_json(&rig).expect("the journal");
        assert_eq!(journal["state"], "rolling_back");
        assert_eq!(journal["schema_version"], 2);
        let pending = journal["git_pending"].as_array().expect("git_pending");
        assert_eq!(pending.len(), 1, "{journal}");
        let trees = rig.save_trees();
        assert_eq!(trees.len(), 1, "the save is kept");
        let save = trees[0]
            .strip_prefix(&rig.f.data_dir)
            .expect("below the data dir")
            .to_string_lossy()
            .replace('\\', "/");
        assert_eq!(pending[0]["node"], "nodes.master");
        assert_eq!(pending[0]["run_id"], run_id.as_str());
        assert_eq!(pending[0]["path"], save.as_str());
        assert_eq!(pending[0]["manifest"]["files"], before.files);
        assert_eq!(pending[0]["manifest"]["bytes"], before.bytes);
        assert_eq!(pending[0]["manifest"]["digest"], before.digest.as_str());
    }

    /// CodeRabbit #61: with the project and the tags already restored, the
    /// check of the retry counts the pending save the apply puts back
    /// (`.git` absent, the save as the journal recorded it) in
    /// `would_change`, and writes nothing. A pending save that is gone is not
    /// counted: the apply writes nothing for it.
    #[tokio::test]
    async fn rollback_run_check_counts_only_the_pending_save_the_apply_puts_back() {
        let (rig, _before, run_id) = rollback_stuck_behind_a_designer().await;
        rig.gateway.detach_designers();
        let journal = host_journal_json(&rig).expect("the journal");
        let check = rig.rollback_run(&run_id, true).await.expect("a check");
        let rollback = check
            .tags
            .as_ref()
            .and_then(|t| t.rollback.as_ref())
            .expect("data.tags.rollback");
        assert!(
            !rollback.project_reimported && rollback.reimported_count == 0,
            "the project and the tags are already restored: {rollback:?}"
        );
        assert!(
            check.would_change,
            "the pending save is put back by the apply: {:?}",
            check.warnings
        );
        assert!(!rig.f.git_dir().exists(), "a check writes no .git");
        assert_eq!(host_journal_json(&rig).expect("the journal stays"), journal);

        let (rig, _before, run_id) = rollback_stuck_behind_a_designer().await;
        rig.gateway.detach_designers();
        let tree = rig.save_trees().remove(0);
        std::fs::remove_dir_all(tree.parent().expect("the run's save directory"))
            .expect("the save is deleted");
        let check = rig.rollback_run(&run_id, true).await.expect("a check");
        assert!(
            !check.would_change,
            "a save that is gone is not put back: {:?}",
            check.warnings
        );
    }

    /// P3: `--rollback-run` with the Designer gone finds `.git` absent and a
    /// pending entry, and puts the save back unchanged, removes the entry and
    /// the save, and clears the journal. Before 27-54 the retry saw no git
    /// work and `.git` stayed absent.
    #[tokio::test]
    async fn rollback_run_retry_restores_the_pending_save() {
        let (rig, before, run_id) = rollback_stuck_behind_a_designer().await;
        rig.gateway.detach_designers();

        let undo = rig.rollback_run(&run_id, false).await.expect("a verdict");
        assert_eq!(
            undo.outcome,
            ProjectOutcome::RolledBack,
            "{:?}",
            undo.warnings
        );
        assert_eq!(rig.git_manifest().await, before, ".git is as it was saved");
        let restore = git_of(&undo).master().restore.clone().expect("restore");
        assert!(restore.restored && !restore.advanced, "{restore:?}");
        assert_eq!(restore.reason, None);
        assert!(!restore.save_kept);
        assert!(rig.save_trees().is_empty(), "the save is removed");
        assert!(host_journal_json(&rig).is_none(), "the journal is cleared");
        assert_eq!(
            rig.git_events(&undo),
            ["git_restored", "git_save_removed"],
            "the retry puts the save back and removes it"
        );
    }

    /// P3: the same retry with the save deleted: the node ends
    /// `git_save_missing` naming the path, nothing else is changed on `.git`,
    /// and the journal stays with its entry.
    #[tokio::test]
    async fn rollback_run_retry_with_the_save_gone_ends_rollback_failed() {
        let (rig, _before, run_id) = rollback_stuck_behind_a_designer().await;
        rig.gateway.detach_designers();
        let journal = host_journal_json(&rig).expect("the journal");
        let path = journal["git_pending"][0]["path"]
            .as_str()
            .expect("a path")
            .to_string();
        let tree = rig.save_trees().remove(0);
        std::fs::remove_dir_all(tree.parent().expect("the run's save directory"))
            .expect("the save is deleted");

        let err = rig
            .rollback_run(&run_id, false)
            .await
            .expect_err("the save is gone");
        assert_eq!(err.code(), "rollback_failed", "{err}");
        let text = err.to_string();
        assert!(text.contains("git_save_missing"), "{text}");
        assert!(text.contains(&path), "the line names the path: {text}");
        assert!(!rig.f.git_dir().exists(), "nothing was written to .git");
        let kept = host_journal_json(&rig).expect("the journal stays");
        assert_eq!(kept["git_pending"][0]["path"], path.as_str(), "{kept}");
        // the retry's own run directory records the failed restore
        let mut runs: Vec<PathBuf> = std::fs::read_dir(rig.repo.join(".ign/deploy/capgw"))
            .expect("run dirs")
            .map(|e| e.expect("entry").path())
            .collect();
        runs.sort();
        let events: Vec<String> = runs
            .iter()
            .filter_map(|dir| std::fs::read_to_string(dir.join("events.ndjson")).ok())
            .collect();
        assert!(
            events
                .iter()
                .any(|log| log.contains("git_restore_failed") && log.contains("git_save_missing")),
            "{events:?}"
        );
    }

    /// P3: a journal without pending saves, a version 1 one and a version 2 one
    /// with none listed, drives today's retry (the tags only, the project
    /// never imported, `.git` untouched).
    #[tokio::test]
    async fn rollback_run_of_a_journal_without_git_pending_behaves_as_today() {
        for version in [2, 1] {
            let session = ("alice".to_string(), PROJECT.to_string(), "ws-1".to_string());
            let rig = Rig::tagged_with(true, |g| {
                g.with_designers_script(vec![vec![], vec![session], vec![]])
                    .with_project_edited_on_designers_read(1, PROJECT)
                    .with_tag_import_failure(2)
            });
            let before = rig.git_manifest().await;
            let err = rig.apply().await.expect_err("the tag rollback failed");
            assert_eq!(err.code(), "rollback_failed", "{err}");
            let mut journal = host_journal_json(&rig).expect("the journal stays");
            assert!(journal.get("git_pending").is_none(), "{journal}");
            let run_id = journal["run_id"].as_str().expect("a run id").to_string();
            if version == 1 {
                journal["schema_version"] = serde_json::json!(1);
                std::fs::write(
                    rig.f
                        .data_dir
                        .join("ign-deploy/capgw/projects")
                        .join(PROJECT)
                        .join("journal.json"),
                    serde_json::to_vec_pretty(&journal).expect("json"),
                )
                .expect("the journal is rewritten");
            }
            let undo = rig.rollback_run(&run_id, false).await.expect("a verdict");
            assert_eq!(
                undo.outcome,
                ProjectOutcome::TagsRolledBack,
                "version {version}: {:?}",
                undo.warnings
            );
            assert!(rig.gateway.import_calls().is_empty(), "version {version}");
            assert_eq!(rig.git_manifest().await, before, "version {version}");
            assert!(host_journal_json(&rig).is_none(), "version {version}");
        }
    }
    /// P3 on a pair: both nodes end the automatic rollback behind an attached
    /// Designer, so the journal names both saves. The retry, with the session
    /// attached again only once the master is done, puts the master's save
    /// back and removes its entry, keeps the backup's entry, ends
    /// `rollback_failed` with a line for the backup, and leaves the journal.
    #[tokio::test]
    async fn rollback_run_retry_keeps_the_journal_while_one_entry_is_pending() {
        let mut rig = Rig::tagged_with(true, |g| g.with_designers_after_exports(3, 100_000));
        rig.tree = Some(rig.f.node());
        rig.rollback_designers = Some(wait_ms(60));
        rig.add_backup();
        rig.gateway.mutate_child_after_import(PROJECT, 1);
        let before_master = rig.git_manifest().await;
        let mut backup = rig.backup_node(0);
        // the sync already removed the backup's `.git` when the content changed
        backup.drop_git_at_first_read = true;
        let host = rig.host();
        let (loaded, prepared) = rig.plan_with(&host).await.expect("the plan");
        let err = rig
            .run_prepared_full(
                &host,
                &loaded,
                prepared,
                None,
                wait_ms(60),
                Some((&backup, WAIT)),
            )
            .await
            .expect_err("both repositories stay behind the Designer");
        assert_eq!(err.code(), "rollback_failed", "{err}");
        let journal = host_journal_json(&rig).expect("the journal stays");
        let run_id = journal["run_id"].as_str().expect("a run id").to_string();
        let nodes: Vec<&str> = journal["git_pending"]
            .as_array()
            .expect("git_pending")
            .iter()
            .map(|e| e["node"].as_str().expect("a node"))
            .collect();
        assert_eq!(nodes.len(), 2, "{journal}");
        assert!(nodes.contains(&"nodes.master") && nodes.contains(&"nodes.backup"));

        // The session goes for the project check and the master's restore (two
        // reads), then is attached again for the backup's.
        rig.gateway.detach_designers();
        let session = ("alice".to_string(), PROJECT.to_string(), "ws-1".to_string());
        let mut script = vec![Vec::new(); rig.gateway.designers_reads() + 2];
        script.push(vec![session]);
        rig.gateway.set_designers_script(script);
        let err = rig
            .rollback_run_with(&run_id, false, Some((&backup, WAIT)))
            .await
            .expect_err("the backup's save is still pending");
        assert_eq!(err.code(), "rollback_failed", "{err}");
        let text = err.to_string();
        assert!(
            text.contains("1 item(s) are left") && text.contains("designer_attached"),
            "one line, for the backup: {text}"
        );
        assert_eq!(
            rig.git_manifest().await,
            before_master,
            "the master is back"
        );
        let kept = host_journal_json(&rig).expect("the journal stays");
        let left: Vec<&str> = kept["git_pending"]
            .as_array()
            .expect("the backup's entry stays")
            .iter()
            .map(|e| e["node"].as_str().expect("a node"))
            .collect();
        assert_eq!(left, ["nodes.backup"], "{kept}");
    }
}

/// Write the executable script `path`. The file is written by `cp`, not by
/// this process: a write handle held here can be inherited by a child another
/// test thread forks at that moment, and Linux then refuses to run the script
/// (`ETXTBSY`, "executable file busy").
#[cfg(unix)]
fn write_exec(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    let name = path.file_name().expect("a file name").to_string_lossy();
    let src = path.with_file_name(format!(".{name}.src"));
    std::fs::write(&src, text).expect("write script");
    let copied = std::process::Command::new("cp")
        .arg(&src)
        .arg(path)
        .status()
        .expect("cp runs");
    assert!(copied.success(), "cp the script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// 27-41: an `ssh` node predicts `expected_drift` from its own disk, as a local
/// node does since 27-40.
#[cfg(unix)]
mod ssh_nodes {
    use super::common;
    use super::common::git_fixture::{GATEWAY, GitFixture, PROJECT, UTIL_RES};
    use super::*;
    use ignition_core::deploy::transport::process::{
        ProcessOutput, ProcessRunner, TokioProcessRunner,
    };
    use ignition_core::deploy::transport::remote::RemoteShell;
    use ignition_core::deploy::transport::sftp::{SftpTransport, posix_root};
    use std::io::{Read, Write};
    use std::sync::Arc;

    const VIEW_RES: &str = "com.inductiveautomation.perspective/views/V/resource.json";

    /// Runs the real processes, with the `stat` shim for a BSD host.
    struct ShimRunner;

    #[async_trait::async_trait]
    impl ProcessRunner for ShimRunner {
        async fn run(
            &self,
            program: &std::ffi::OsStr,
            args: &[std::ffi::OsString],
            stdin: Option<Vec<u8>>,
            timeout: std::time::Duration,
        ) -> ProcessOutput {
            let (program, args) = common::posix_exec::shimmed(program, args);
            TokioProcessRunner
                .run(&program, &args, stdin, timeout)
                .await
        }
    }

    /// `zip` with the member `name` replaced by `bytes`.
    fn with_member(zip: &[u8], name: &str, bytes: &[u8]) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("zip");
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let mut replaced = false;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("entry");
            if entry.is_dir() {
                continue;
            }
            let mut body = Vec::new();
            entry.read_to_end(&mut body).expect("member");
            if entry.name() == name {
                body = bytes.to_vec();
                replaced = true;
            }
            writer
                .start_file(entry.name(), zip::write::SimpleFileOptions::default())
                .expect("start");
            writer.write_all(&body).expect("write");
        }
        assert!(replaced, "{name} is an export member");
        writer.finish().expect("finish").into_inner()
    }

    /// The gateway's rendering of a descriptor (pretty-printed).
    fn pretty(bytes: &[u8]) -> Vec<u8> {
        let value: serde_json::Value = serde_json::from_slice(bytes).expect("json");
        let mut out = serde_json::to_vec_pretty(&value).expect("pretty");
        out.push(b'\n');
        out
    }

    /// The over-count of 2026-11-18, on a node reached over ssh. The import
    /// rewrote one resource (`UTIL_RES`, written in the gateway's form) and left
    /// the other resource (`V`) alone: its disk bytes are the release's, while
    /// the export renders it in the gateway's form. The prediction must list
    /// only the rewritten path, which is what JGit lists on that disk.
    async fn predicts_from_disk(windows: bool) {
        let f = GitFixture::new();
        let host = HostGit::new();
        let home = tempfile::tempdir().expect("home");
        let data = posix_root(&f.data_dir.display().to_string()).expect("posix root");
        let shell = |runner: Arc<dyn ProcessRunner>| {
            RemoteShell::local_sftp_server(
                &common::sftp_server::sftp_server_path(),
                runner,
                std::time::Duration::from_secs(120),
            )
            .local_exec()
        };
        let node = if windows {
            let runner = common::pwsh_exec::PwshHomeRunner {
                home: home.path().to_path_buf(),
            };
            SftpTransport::new_windows(shell(Arc::new(runner)), data, "nodes.pwsh")
                .with_home(common::pwsh_exec::SEAM_HOME)
        } else {
            let home_root = posix_root(&home.path().display().to_string()).expect("home root");
            SftpTransport::new(shell(Arc::new(ShimRunner)), data, "nodes.sh").with_home(&home_root)
        };

        let (state, prep) = f.prepare(&host).await;
        let saved = save(&node, &node, LedgerRoot::AccountHome, GATEWAY, PROJECT, RUN)
            .await
            .expect("save");
        // the import: the release's bytes everywhere, `UTIL_RES` rewritten
        f.module_import(true, &[UTIL_RES], &[], &[]);
        // the export renders `V`'s descriptor in the gateway's form as well
        let view = std::fs::read(f.project_dir().join(VIEW_RES)).expect("V descriptor");
        let export = with_member(&f.export_zip(), VIEW_RES, &pretty(&view));
        let report = restore_and_advance(
            &node,
            &node,
            &host,
            &f.checkout,
            &state,
            &saved,
            &prep,
            &export,
            None,
        )
        .await
        .unwrap_or_else(|failure| panic!("the handover: {failure:?}"));
        assert!(report.advanced && report.verified, "{report:?}");
        assert_eq!(
            report.expected_drift,
            vec![UTIL_RES.to_string()],
            "only the rewritten resource is drift"
        );
        assert_eq!(f.status_paths(), report.expected_drift);
    }

    #[tokio::test]
    async fn ssh_node_predicts_from_its_disk() {
        predicts_from_disk(false).await;
        if common::pwsh_exec::gate("ssh_node_predicts_from_its_disk (windows shape)") {
            predicts_from_disk(true).await;
        }
    }
}
