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
    UNPUSHED, UTIL_RES, in_scope,
};
use ignition_core::deploy::git_advance::{GitWrite, GitWriteName};
use ignition_core::deploy::git_handover::{
    GitRestoreFailure, GitRestoreReport, RESTORE_REASONS, restore_and_advance, save,
};
use ignition_core::deploy::git_remote::HostGit;
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
    let d = f.designer_commit_unpushed();
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
