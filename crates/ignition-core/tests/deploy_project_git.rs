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
use ignition_core::deploy::git_state::{GitState, url_key};
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
    let saved = save(&node, &node, GATEWAY, PROJECT, RUN)
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
    let saved = save(&node, &node, GATEWAY, PROJECT, RUN)
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

#[tokio::test]
async fn restore_never_overwrites() {
    let f = GitFixture::new();
    let node = f.node();
    save(&node, &node, GATEWAY, PROJECT, RUN)
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
    let saved = save(&node, &node, GATEWAY, PROJECT, RUN)
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
    let saved = save(&node, &node, GATEWAY, PROJECT, RUN)
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
    let mut saved = save(&node, &node, GATEWAY, PROJECT, RUN)
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

    let err = save(&node, &node, GATEWAY, PROJECT, RUN)
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
            name: GitWriteName::RemoteRef {
                remote: REMOTE.to_string(),
                branch: BRANCH.to_string(),
            },
            bytes: new.clone(),
        },
        GitWrite {
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
    let saved = save(&node, &node, GATEWAY, PROJECT, RUN)
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
    use std::os::unix::fs::PermissionsExt;
    let f = GitFixture::new();
    let url = "https://user:secret@127.0.0.1:9/x.git";
    assert_eq!(f.checkout_with_userinfo_remote(url), "origin");

    // A recorder in front of git: nothing of the URL may reach an argument.
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("argv.log");
    let recorder = dir.path().join("git-recorder");
    std::fs::write(
        &recorder,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\nexec git \"$@\"\n",
            log.display()
        ),
    )
    .expect("write recorder");
    let mut perms = std::fs::metadata(&recorder).expect("stat").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&recorder, perms).expect("chmod");
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
    use std::os::unix::fs::PermissionsExt;
    let f = GitFixture::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let wrapper = dir.path().join("git-moved");
    std::fs::write(
        &wrapper,
        "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = ls-remote ]; then\n    printf 'ffffffffffffffffffffffffffffffffffffffff\\trefs/heads/production\\n'\n    exit 0\n  fi\ndone\nexec git \"$@\"\n",
    )
    .expect("write wrapper");
    let mut perms = std::fs::metadata(&wrapper).expect("stat").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper, perms).expect("chmod");

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
    use ignition_core::deploy::transport::projects::{GitEntry, ProjectHost};
    use ignition_core::error::CoreError;

    /// A host that answers like the local transport and runs `program` as
    /// host git (a stub, to prove git is never run or to make one step fail).
    pub struct StubHost {
        pub inner: LocalFsTransport,
        pub program: Option<OsString>,
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
                Some(program) => Git::with_program(program),
                None => Git::new(),
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
            let f = GitFixture::new();
            put_file(
                &f.checkout,
                "deploy/project.yaml",
                &format!("schema_version: 1\nproject: {PROJECT}\nenvironments: [dev]\n"),
            );
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
            let gateway = shape(
                FakeProjects::new()
                    .with_project(PROJECT, None, false, &borrowed)
                    .with_disk_mirror(&f.data_dir, true),
            );
            let repo = f.checkout.clone();
            Rig {
                f,
                targets,
                gateway,
                repo,
                release: "v1.1.0".to_string(),
            }
        }

        pub fn input(&self) -> ProjectsInput {
            ProjectsInput {
                repo: self.repo.clone(),
                targets: self.targets.path().to_path_buf(),
                env: "dev".to_string(),
                release: self.release.clone(),
                allow_downgrade: false,
                allow_first_change: Some("first deploy of the rig".to_string()),
                discard_gateway_changes: None,
                allow_git_sync_hazards: None,
                allow_destructive: false,
                allow_manual_undo: false,
                artifact_sha256: None,
                tags_sha256: None,
            }
        }

        pub fn host(&self) -> StubHost {
            StubHost {
                inner: self.f.node(),
                program: None,
                read: None,
            }
        }

        /// A host whose host git is `program`.
        pub fn host_with_program(&self, program: &std::path::Path) -> StubHost {
            StubHost {
                inner: self.f.node(),
                program: Some(program.as_os_str().to_os_string()),
                read: None,
            }
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
                    tree: None,
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
            let input = self.input();
            deploy_projects_run(
                prepared,
                loaded,
                &input,
                &ProjectsSurfaces {
                    gateway: &self.gateway,
                    host,
                    tree: None,
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
                    designers,
                    children_settle: std::time::Duration::from_secs(1),
                    children_poll: std::time::Duration::from_millis(5),
                    tag_verify_settle: std::time::Duration::from_secs(1),
                    tag_verify_poll: std::time::Duration::from_millis(5),
                    backup: None,
                    backup_policy: ignition_core::deploy::pair::BackupPolicy::default(),
                    tag_delete_ready: std::time::Duration::from_millis(50),
                    tag_delete_poll: std::time::Duration::from_millis(5),
                },
                &NoProjectsObserver,
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
            let root = self
                .f
                .data_dir
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
    struct Unlock(PathBuf);
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
    async fn designer_attached_at_restore() {
        let rig = Rig::with(|g| g.with_designers_after_verify(100_000));
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
        let rig = Rig::with(|g| g.with_designers_after_verify(2));
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
        let rig = Rig::with(|g| g.with_designers_after_verify(100_000));
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
}
