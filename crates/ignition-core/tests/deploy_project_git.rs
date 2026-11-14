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
