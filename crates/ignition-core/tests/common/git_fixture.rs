//! A gateway project with a git-module-shaped clone, built with host git in a
//! temp dir (Phase 27, 27-02).
//!
//! Neutral names throughout: project `CapApp`, remote repo `capapp.git`. The
//! shape mirrors what the git module leaves on a gateway:
//!
//! - a bare remote with branches `main` and `production` (no `master`) and
//!   tags `v1.0.0` (C1) and `v1.1.0` (C2), C2 a child of C1 on `production`;
//! - the release checkout (the "seed"), at C2;
//! - a gateway data dir whose `projects/CapApp` is a module-style clone of C1:
//!   the remote is named for the repo (`capapp`), single-branch fetch refspec,
//!   `[branch "production"]` tracking it, `[http] sslVerify = false`;
//! - [`GitFixture::module_import`], the R-3 simulation: an import that deletes
//!   `.git`, `tags/`, `images/` and the README and writes the artifact's files;
//! - [`GitFixture::partial_setup_local_repo`], the shape the module's
//!   `setupLocalRepo` leaves against a remote with no `master` (G6, 27-17): an
//!   unborn `master` HEAD, remote `origin`, the remote's branches and tags in
//!   `packed-refs`, no index, no `[branch]` section.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use ignition_core::deploy::git_advance::{
    AdvanceMode, AdvancePrep, prepare_advance, prepare_advance_with,
};
use ignition_core::deploy::git_remote::HostGit;
use ignition_core::deploy::git_state::{GitRefs, GitState, GitStateKind, classify};
use ignition_core::deploy::project_artifact::{Artifact, build_artifact};
use ignition_core::deploy::transport::local::LocalFsTransport;
use ignition_core::deploy::transport::projects::GitHostRead;

/// The project's name.
pub const PROJECT: &str = "CapApp";
/// The gateway's name in ledger paths.
pub const GATEWAY: &str = "gw-a";
/// The remote's name in the gateway config (named for the repo).
pub const REMOTE: &str = "capapp";
/// The tracked branch.
pub const BRANCH: &str = "production";

/// A resource folder file C1 and C2 both hold.
pub const CODE: &str = "ignition/script-python/cap/util/code.py";
/// Its `resource.json` (the one the gateway re-serialises).
pub const UTIL_RES: &str = "ignition/script-python/cap/util/resource.json";
/// A file in that folder that `resource.json` lists, so the artifact carries
/// it, and that the gateway does not keep on import (`module_import` `drop`).
pub const NOTES: &str = "ignition/script-python/cap/util/NOTES.txt";
/// A script C2 adds.
pub const NEW_CODE: &str = "ignition/script-python/cap/util2/code.py";
/// An in-scope script only a checkout holds (never committed); it sits in its
/// own resource folder, so the artifact carries it with [`LOCAL_ONLY_RES`].
pub const LOCAL_ONLY: &str = "ignition/script-python/cap/local/code.py";
/// The `resource.json` of that folder.
pub const LOCAL_ONLY_RES: &str = "ignition/script-python/cap/local/resource.json";
/// A member the artifact lacks that an import writes anyway.
pub const EXTRA: &str = "ignition/script-python/cap/extra.txt";
/// A file a designer commits on the gateway and never pushes.
pub const UNPUSHED: &str = "ignition/script-python/cap/util/unpushed.py";
/// The dummy credential [`GitFixture::with_git_yaml`] writes into `git.yaml`
/// (a `user_password` and the userinfo of `repo_uri`). It must reach no
/// envelope field, event line, run-directory file or error text.
pub const GIT_YAML_SECRET: &str = "gy-dummy-credential-7f3a91c2";

/// Whether `path` is one an artifact carries (the project root allowlist).
pub fn in_scope(path: &str) -> bool {
    path == "project.json"
        || path.starts_with("ignition/")
        || path.starts_with("com.inductiveautomation.")
}

/// Repository-selection variables a test process may inherit (a git hook,
/// an IDE, `git rebase --exec`). Any of them makes git ignore the working
/// directory and act on the caller's repository instead of the fixture's.
const GIT_LOCATION_VARS: [&str; 8] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// Host git in `dir` on the fixture's repository only: the inherited
/// repository-selection variables cleared, no user or system config, no
/// background maintenance or auto gc, and a fixed identity.
fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for var in GIT_LOCATION_VARS {
        command.env_remove(var);
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        // no background maintenance or auto gc: git 2.5x repacks a many-pack
        // repo after a commit, under a test that is reading its packs
        .env("GIT_CONFIG_COUNT", "2")
        .env("GIT_CONFIG_KEY_0", "maintenance.auto")
        .env("GIT_CONFIG_VALUE_0", "false")
        .env("GIT_CONFIG_KEY_1", "gc.auto")
        .env("GIT_CONFIG_VALUE_1", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid");
    command
}

/// Run host git in `dir` ([`git_command`]) and return its trimmed stdout.
/// Panics on a non-zero exit.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_command(dir).args(args).output().expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Write `body` to `root/rel`, creating folders.
pub fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

/// The fixture.
pub struct GitFixture {
    _dir: tempfile::TempDir,
    /// The bare remote.
    pub remote: PathBuf,
    /// The release checkout, at C2.
    pub checkout: PathBuf,
    /// The gateway data dir.
    pub data_dir: PathBuf,
    /// C1, the commit the gateway clone starts at.
    pub c1: String,
    /// C2, the release commit.
    pub c2: String,
}

impl GitFixture {
    /// Build the remote, the release checkout and the gateway clone.
    pub fn new() -> GitFixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let remote = root.join("capapp.git");
        let checkout = root.join("release");
        let data_dir = root.join("data");
        std::fs::create_dir(&remote).expect("mkdir");
        std::fs::create_dir(&checkout).expect("mkdir");
        std::fs::create_dir_all(data_dir.join("projects")).expect("mkdir");

        git(&remote, &["init", "-q", "--bare", "-b", "main"]);
        git(&checkout, &["init", "-q", "-b", "main"]);
        git(
            &checkout,
            &["remote", "add", "origin", remote.to_str().expect("utf8")],
        );
        put(&checkout, "README.md", "readme\n");
        git(&checkout, &["add", "-A"]);
        git(&checkout, &["commit", "-q", "-m", "init"]);
        git(&checkout, &["push", "-q", "origin", "main"]);

        git(&checkout, &["checkout", "-q", "-b", BRANCH]);
        put(&checkout, "project.json", "{\"title\":\"CapApp\"}\n");
        put(&checkout, CODE, "def f():\n    return 1\n");
        put(
            &checkout,
            UTIL_RES,
            "{\"scope\":\"A\",\"files\":[\"code.py\",\"NOTES.txt\"]}\n",
        );
        put(&checkout, NOTES, "notes\n");
        put(
            &checkout,
            "com.inductiveautomation.perspective/views/V/view.json",
            "{\"root\":{}}\n",
        );
        put(
            &checkout,
            "com.inductiveautomation.perspective/views/V/resource.json",
            "{\"scope\":\"G\",\"files\":[\"view.json\"]}\n",
        );
        put(&checkout, "tags/tags.json", "{\"tags\":[]}\n");
        put(&checkout, "images/logo.txt", "logo\n");
        git(&checkout, &["add", "-A"]);
        git(&checkout, &["commit", "-q", "-m", "one"]);
        let c1 = git(&checkout, &["rev-parse", "HEAD"]);
        git(&checkout, &["tag", "v1.0.0"]);
        git(&checkout, &["push", "-q", "origin", BRANCH]);

        // The git module's clone: remote named for the repo, single branch.
        let project = data_dir.join("projects").join(PROJECT);
        git(
            &root,
            &[
                "clone",
                "-q",
                "-o",
                REMOTE,
                "-b",
                BRANCH,
                "--single-branch",
                remote.to_str().expect("utf8"),
                project.to_str().expect("utf8"),
            ],
        );
        git(&project, &["config", "http.sslVerify", "false"]);

        put(&checkout, CODE, "def f():\n    return 2\n");
        put(&checkout, NEW_CODE, "def g():\n    return 3\n");
        put(
            &checkout,
            "ignition/script-python/cap/util2/resource.json",
            "{\"scope\":\"A\",\"files\":[\"code.py\"]}\n",
        );
        git(&checkout, &["add", "-A"]);
        git(&checkout, &["commit", "-q", "-m", "two"]);
        let c2 = git(&checkout, &["rev-parse", "HEAD"]);
        git(&checkout, &["tag", "v1.1.0"]);
        git(&checkout, &["push", "-q", "origin", BRANCH, "--tags"]);

        GitFixture {
            _dir: dir,
            remote,
            checkout,
            data_dir,
            c1,
            c2,
        }
    }

    /// Replace the gateway clone's `.git` with the partial repo the module's
    /// `setupLocalRepo` leaves when the remote has no `master` (G6): an unborn
    /// `master` HEAD, one remote `origin` with the default refspec, every
    /// remote branch and tag fetched into `packed-refs`, `[http] sslVerify =
    /// false`, no local branch, no index and no `[branch]` section. The work
    /// tree is left as it is. Never deletes anything but the clone's own
    /// `.git`, which the fixture created.
    pub fn partial_setup_local_repo(&self) {
        let project = self.project_dir();
        std::fs::remove_dir_all(self.git_dir()).expect("the fixture's clone .git goes");
        git(&project, &["init", "-q", "-b", "master"]);
        git(
            &project,
            &[
                "remote",
                "add",
                "origin",
                self.remote.to_str().expect("utf8"),
            ],
        );
        git(&project, &["config", "http.sslVerify", "false"]);
        git(&project, &["fetch", "-q", "origin", "--tags"]);
        git(&project, &["pack-refs", "--all"]);
        self.assert_partial_shape();
    }

    /// The same partial shape built from the module's clone itself, so the
    /// repo holds objects only up to C1 (the release C2 is missing and an
    /// adoption must send it as a pack): the clone's local branch, index, logs
    /// and `[branch]` section go, `HEAD` points at an unborn `master`, the
    /// remote is renamed `origin`.
    pub fn partial_behind(&self) {
        let project = self.project_dir();
        git(&project, &["symbolic-ref", "HEAD", "refs/heads/master"]);
        git(
            &project,
            &["update-ref", "-d", &format!("refs/heads/{BRANCH}")],
        );
        git(
            &project,
            &["config", "--remove-section", &format!("branch.{BRANCH}")],
        );
        git(&project, &["remote", "rename", REMOTE, "origin"]);
        git(&project, &["pack-refs", "--all"]);
        let dot_git = self.git_dir();
        let _ = std::fs::remove_file(dot_git.join("index"));
        let _ = std::fs::remove_dir_all(dot_git.join("logs"));
        self.assert_partial_shape();
    }

    /// The G6 recording, as asserts: nothing the partial shape lacks is there.
    fn assert_partial_shape(&self) {
        let project = self.project_dir();
        assert_eq!(
            git(&project, &["symbolic-ref", "HEAD"]),
            "refs/heads/master"
        );
        assert_eq!(
            git(&project, &["for-each-ref", "refs/heads"]),
            "",
            "no local branch"
        );
        assert!(!self.git_dir().join("index").exists(), "no index");
        assert_eq!(git(&project, &["remote"]), "origin");
        let config = std::fs::read_to_string(self.git_dir().join("config")).expect("config");
        assert!(!config.contains("[branch"), "no [branch] section: {config}");
    }

    /// Write `<data_dir>/git.yaml`: one entry per `(project, repo_branch)`,
    /// each with a `user_password`, a `user_name`, a `repo_uri` that carries
    /// [`GIT_YAML_SECRET`] as userinfo and a key this build does not know.
    /// With `repo_branch` `None` the entry names no branch.
    pub fn with_git_yaml(&self, entries: &[(&str, Option<&str>)]) {
        let mut text = String::new();
        for (project, branch) in entries {
            text.push_str(&format!("- ignition_projectName: {project}\n"));
            text.push_str(&format!(
                "  repo_uri: https://dummy-user:{GIT_YAML_SECRET}@git.example/org/capapp.git\n"
            ));
            if let Some(branch) = branch {
                text.push_str(&format!("  repo_branch: {branch}\n"));
            }
            text.push_str(&format!(
                "  user_name: dummy-user\n  user_password: {GIT_YAML_SECRET}\n  \
                 commissioning_importTags: \"false\"\n  unknown_future_key: [1, 2]\n"
            ));
        }
        put(&self.data_dir, "git.yaml", &text);
    }

    /// Write `<data_dir>/git.yaml` with one entry per `(project, flags)`:
    /// each `(key, value)` of `flags` is written as `key: value`, beside the
    /// same credential lines [`with_git_yaml`](Self::with_git_yaml) writes
    /// (27-18: the module's start-sync flags).
    pub fn with_git_yaml_flags(&self, entries: &[(&str, &[(&str, &str)])]) {
        let mut text = String::new();
        for (project, flags) in entries {
            text.push_str(&format!("- ignition_projectName: {project}\n"));
            text.push_str(&format!(
                "  repo_uri: https://dummy-user:{GIT_YAML_SECRET}@git.example/org/capapp.git\n  \
                 user_name: dummy-user\n  user_password: {GIT_YAML_SECRET}\n"
            ));
            for (key, value) in *flags {
                text.push_str(&format!("  {key}: {value}\n"));
            }
        }
        put(&self.data_dir, "git.yaml", &text);
    }

    /// Remove `tags/` from the project directory, as a gateway import leaves
    /// it (G3: the overwrite import removes `tags/` and `images/`).
    pub fn without_tags_dir(&self) {
        let _ = std::fs::remove_dir_all(self.project_dir().join("tags"));
    }

    /// Point the partial repo's `origin` at `url`.
    pub fn set_gateway_remote_url(&self, url: &str) {
        git(&self.project_dir(), &["remote", "set-url", "origin", url]);
    }

    /// A branch the release is not on: created at `main`'s tip and pushed.
    pub fn push_branch_without_release(&self, branch: &str) {
        let root = self._dir.path();
        let scratch = root.join(format!("scratch-branch-{branch}"));
        git(
            root,
            &[
                "clone",
                "-q",
                "-b",
                "main",
                self.remote.to_str().expect("utf8"),
                scratch.to_str().expect("utf8"),
            ],
        );
        git(
            &scratch,
            &["push", "-q", "origin", &format!("main:refs/heads/{branch}")],
        );
    }

    /// `data/projects/CapApp`.
    pub fn project_dir(&self) -> PathBuf {
        self.data_dir.join("projects").join(PROJECT)
    }

    /// `data/projects/CapApp/.git`.
    pub fn git_dir(&self) -> PathBuf {
        self.project_dir().join(".git")
    }

    /// The backup node's data dir (27-22): a second gateway data dir whose
    /// `projects/CapApp` is a byte copy of the master's module-style clone as
    /// it is NOW (so the two nodes agree on origin, branch and HEAD), taken the
    /// first time this is called. Change either clone afterwards to make the
    /// nodes differ.
    pub fn backup_data_dir(&self) -> PathBuf {
        let dir = self
            .data_dir
            .parent()
            .expect("the fixture root")
            .join("data-b");
        if !dir.exists() {
            std::fs::create_dir_all(dir.join("projects")).expect("mkdir");
            let status = Command::new("cp")
                .arg("-R")
                .arg(self.project_dir())
                .arg(dir.join("projects").join(PROJECT))
                .status()
                .expect("cp runs");
            assert!(status.success(), "the backup clone is copied");
        }
        dir
    }

    /// `data-b/projects/CapApp`.
    pub fn backup_project_dir(&self) -> PathBuf {
        self.backup_data_dir().join("projects").join(PROJECT)
    }

    /// `data-b/projects/CapApp/.git`.
    pub fn backup_git_dir(&self) -> PathBuf {
        self.backup_project_dir().join(".git")
    }

    /// Run host git in the backup's project dir.
    pub fn git_in_backup(&self, args: &[&str]) -> String {
        git(&self.backup_project_dir(), args)
    }

    /// The local-filesystem transport over the backup's data dir.
    pub fn backup_node(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.backup_data_dir(), "nodes.backup")
    }

    /// The local-filesystem transport over the gateway data dir.
    pub fn node(&self) -> LocalFsTransport {
        LocalFsTransport::new(self.data_dir.clone(), "nodes.master")
    }

    /// Run host git in the gateway project dir.
    pub fn git_in_project(&self, args: &[&str]) -> String {
        git(&self.project_dir(), args)
    }

    /// A designer commit on the gateway clone. With `push` false it is never
    /// pushed; with `push` true the clone first fast-forwards to the remote
    /// and the commit is pushed to the tracked branch (a break/fix other
    /// people can see). Returns its id.
    pub fn designer_commit(&self, push: bool) -> String {
        let project = self.project_dir();
        if push {
            git(&project, &["pull", "-q", "--ff-only"]);
        }
        let head = git(&project, &["rev-parse", "HEAD"]);
        put(&project, UNPUSHED, &format!("u = '{head}'\n"));
        // The folder's descriptor lists the new file, so the artifact (and a
        // digest of this commit's tree) carries it like the gateway's export.
        put(
            &project,
            UTIL_RES,
            "{\"scope\":\"A\",\"files\":[\"code.py\",\"NOTES.txt\",\"unpushed.py\"]}\n",
        );
        git(&project, &["add", "-A"]);
        git(&project, &["commit", "-q", "-m", "designer"]);
        if push {
            git(&project, &["push", "-q", REMOTE, BRANCH]);
        }
        git(&project, &["rev-parse", "HEAD"])
    }

    /// A designer commit on the gateway clone that changes exactly the file
    /// `rel` to `body` (27-10). With `push` the clone first fast-forwards and
    /// the commit is pushed to the tracked branch. Returns its id. `rel` must
    /// be a file the tree's `resource.json` files already list (or a
    /// non-artifact path), so the commit's tree and the live export agree.
    pub fn designer_commit_changing(&self, rel: &str, body: &str, push: bool) -> String {
        let project = self.project_dir();
        if push {
            git(&project, &["pull", "-q", "--ff-only"]);
        }
        put(&project, rel, body);
        git(&project, &["add", "-A"]);
        git(&project, &["commit", "-q", "-m", "designer"]);
        if push {
            git(&project, &["push", "-q", REMOTE, BRANCH]);
        }
        git(&project, &["rev-parse", "HEAD"])
    }

    /// A change to the live project that is never committed (what the gateway
    /// holds beyond its own HEAD).
    pub fn live_change(&self, rel: &str, body: &str) {
        put(&self.project_dir(), rel, body);
    }

    /// `git gc` in the gateway clone: every object ends up in a pack.
    pub fn gateway_gc(&self) {
        git(&self.project_dir(), &["gc", "-q", "--prune=now"]);
    }

    /// Two packs in the gateway clone, loose objects pruned: one with the
    /// history up to C1 (what the clone started with), one with every object
    /// the gateway added since (the unpushed commits). Returns the two pack
    /// names, history first.
    pub fn gateway_split_packs(&self) -> (String, String) {
        let project = self.project_dir();
        let pack_dir = self.git_dir().join("objects").join("pack");
        let old: Vec<PathBuf> = std::fs::read_dir(&pack_dir)
            .expect("pack dir")
            .map(|e| e.expect("entry").path())
            .collect();
        let head = git(&project, &["rev-parse", "HEAD"]);
        let history = self.pack_objects(&project, &["--objects", &self.c1]);
        let unpushed = self.pack_objects(&project, &["--objects", &head, "--not", &self.c1]);
        let keep = [format!("pack-{history}."), format!("pack-{unpushed}.")];
        for path in old {
            let name = path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            if !keep.iter().any(|k| name.starts_with(k)) {
                std::fs::remove_file(&path).expect("remove old pack file");
            }
        }
        git(&project, &["prune-packed", "-q"]);
        (history, unpushed)
    }

    /// Delete one loose object of the gateway clone (a gateway repo that
    /// cannot supply its HEAD).
    pub fn gateway_drop_object(&self, id: &str) {
        let path = self.git_dir().join("objects").join(&id[..2]).join(&id[2..]);
        std::fs::remove_file(&path).expect("the object is loose in the gateway clone");
    }

    /// `git pack-objects` of `rev-list <args>` into the gateway clone's pack
    /// dir; returns the pack's name.
    fn pack_objects(&self, project: &Path, rev_list: &[&str]) -> String {
        let mut args = vec!["rev-list"];
        args.extend_from_slice(rev_list);
        let listing = git(project, &args);
        let mut child = git_command(project)
            .args(["pack-objects", "-q", ".git/objects/pack/pack"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("git pack-objects starts");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(format!("{listing}\n").as_bytes())
            .expect("feed pack-objects");
        let out = child.wait_with_output().expect("pack-objects finishes");
        assert!(
            out.status.success(),
            "pack-objects failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A pushed break/fix: the gateway clone fast-forwards, commits and
    /// pushes to `production`, so the remote tip is a child of the release
    /// commit that the release lacks. Returns its id.
    pub fn breakfix_on_production(&self) -> String {
        self.designer_commit(true)
    }

    /// A commit pushed to `production` from a scratch clone (the gateway
    /// clone and the release checkout are untouched). Returns its id.
    pub fn push_to_production(&self, rel: &str, body: &str) -> String {
        let root = self._dir.path();
        let mut n = 0;
        let scratch = loop {
            let candidate = root.join(format!("scratch-{n}"));
            if !candidate.exists() {
                break candidate;
            }
            n += 1;
        };
        git(
            root,
            &[
                "clone",
                "-q",
                "-b",
                BRANCH,
                self.remote.to_str().expect("utf8"),
                scratch.to_str().expect("utf8"),
            ],
        );
        put(&scratch, rel, body);
        git(&scratch, &["add", "-A"]);
        git(&scratch, &["commit", "-q", "-m", "pushed"]);
        git(&scratch, &["push", "-q", "origin", BRANCH]);
        git(&scratch, &["rev-parse", "HEAD"])
    }

    /// Tag a new release commit on `branch` (created from the current
    /// commit when it does not exist), push the branch and the tag, and leave
    /// the release checkout at it. Returns the commit id.
    pub fn tag_release_on(&self, branch: &str, tag: &str) -> String {
        let checkout = &self.checkout;
        let exists = git_command(checkout)
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .output()
            .expect("git runs")
            .status
            .success();
        if exists {
            git(checkout, &["checkout", "-q", branch]);
        } else {
            git(checkout, &["checkout", "-q", "-b", branch]);
        }
        put(checkout, "RELEASE.txt", &format!("{tag}\n"));
        git(checkout, &["add", "-A"]);
        git(checkout, &["commit", "-q", "-m", &format!("release {tag}")]);
        git(checkout, &["tag", tag]);
        git(checkout, &["push", "-q", "origin", branch, tag]);
        git(checkout, &["rev-parse", "HEAD"])
    }

    /// A `--depth 1` clone of the remote at `v1.1.0` (a shallow release
    /// checkout). Its path is inside the fixture's temp dir.
    pub fn shallow_checkout(&self) -> PathBuf {
        let root = self._dir.path();
        let path = root.join("shallow");
        git(
            root,
            &[
                "clone",
                "-q",
                "--depth",
                "1",
                "--branch",
                "v1.1.0",
                &format!("file://{}", self.remote.display()),
                path.to_str().expect("utf8"),
            ],
        );
        path
    }

    /// Point the release checkout's `origin` at `url` (which may carry
    /// userinfo). Returns the remote's name.
    pub fn checkout_with_userinfo_remote(&self, url: &str) -> String {
        git(&self.checkout, &["remote", "set-url", "origin", url]);
        "origin".to_string()
    }

    /// The tail of the gateway repo's `logs/HEAD`, as a host read returns it.
    pub async fn head_log_tail(&self) -> Option<Vec<u8>> {
        self.node()
            .read_git_state(PROJECT)
            .await
            .expect("host read")
            .head_log_tail
    }

    /// An in-scope resource folder in the release checkout that no commit
    /// holds ([`LOCAL_ONLY`] and [`LOCAL_ONLY_RES`]): the artifact carries
    /// both files, C2's tree neither.
    pub fn untracked_in_checkout(&self) {
        put(&self.checkout, LOCAL_ONLY, "x = 1\n");
        put(
            &self.checkout,
            LOCAL_ONLY_RES,
            "{\"scope\":\"A\",\"files\":[\"code.py\"]}\n",
        );
    }

    /// The artifact built from the release checkout.
    pub fn artifact(&self) -> Artifact {
        build_artifact(&self.checkout).expect("artifact builds")
    }

    /// The gateway repo as a host read shows it, classified, with its refs.
    pub async fn gateway_state(&self, host: &HostGit) -> (GitState, GitRefs) {
        let files = self
            .node()
            .read_git_state("CapApp")
            .await
            .expect("host read");
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

    /// Everything the handover needs before the import: the managed state
    /// and the prepared advance to C2.
    pub async fn prepare(&self, host: &HostGit) -> (GitState, AdvancePrep) {
        let (state, refs) = self.gateway_state(host).await;
        assert_eq!(state.kind, GitStateKind::Managed, "{state:?}");
        let members = self.artifact().digest.members.keys().cloned().collect();
        let prep = prepare_advance(host, &self.checkout, &state, &refs, &self.c2, &members)
            .await
            .expect("prepare");
        (state, prep)
    }

    /// Everything the handover needs before the import for a partial repo: the
    /// adoptable state and the prepared adoption of `branch` at C2.
    pub async fn prepare_adopt(&self, host: &HostGit, branch: &str) -> (GitState, AdvancePrep) {
        let (state, refs) = self.gateway_state(host).await;
        assert_eq!(state.kind, GitStateKind::Adoptable, "{state:?}");
        let config = std::fs::read(self.git_dir().join("config")).expect("config");
        let members = self.artifact().digest.members.keys().cloned().collect();
        let prep = prepare_advance_with(
            host,
            &self.checkout,
            &state,
            &refs,
            &self.c2,
            &members,
            AdvanceMode::Adopt {
                branch: branch.to_string(),
                remote: "origin".to_string(),
                config,
            },
        )
        .await
        .expect("prepare the adoption");
        (state, prep)
    }

    /// Simulate the module's import (R-3 when `keep_git` is false, R-6 when
    /// true): the project directory is emptied (every name but `.git` when it
    /// is kept), then the artifact's files are written.
    ///
    /// - `gateway_form`: JSON members written re-serialised the way the
    ///   gateway does (pretty-printed).
    /// - `extra`: members the artifact lacks that are written anyway.
    /// - `drop`: artifact members left off disk, as the gateway does with a
    ///   file its `resource.json` does not list.
    pub fn module_import(
        &self,
        keep_git: bool,
        gateway_form: &[&str],
        extra: &[&str],
        drop: &[&str],
    ) {
        let project = self.project_dir();
        for entry in std::fs::read_dir(&project).expect("read_dir") {
            let path = entry.expect("entry").path();
            if keep_git && path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                std::fs::remove_dir_all(&path).expect("rm dir");
            } else {
                std::fs::remove_file(&path).expect("rm file");
            }
        }
        let artifact = self.artifact();
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(artifact.zip)).expect("artifact zip");
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("entry");
            if entry.is_dir() {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read member");
            files.insert(entry.name().to_string(), bytes);
        }
        for name in drop {
            assert!(
                files.remove(*name).is_some(),
                "{name} is not an artifact member"
            );
        }
        for name in gateway_form {
            let bytes = files
                .get(*name)
                .unwrap_or_else(|| panic!("{name} is not written"));
            let value: serde_json::Value = serde_json::from_slice(bytes).expect("json member");
            let mut pretty = serde_json::to_vec_pretty(&value).expect("pretty");
            pretty.push(b'\n');
            assert_ne!(&pretty, bytes, "{name} must change when re-serialised");
            files.insert((*name).to_string(), pretty);
        }
        for name in extra {
            files.insert((*name).to_string(), b"x\n".to_vec());
        }
        for (name, bytes) in files {
            put_bytes(&project, &name, &bytes);
        }
    }

    /// The export the gateway would give: the in-scope files of the project
    /// directory as a zip.
    pub fn export_zip(&self) -> Vec<u8> {
        live_export_from(&self.project_dir())
    }

    /// `git status --porcelain` paths of the gateway repo, byte-sorted.
    pub fn status_paths(&self) -> Vec<String> {
        let text = self.git_in_project(&["status", "--porcelain", "--untracked-files=all"]);
        let mut paths: Vec<String> = text
            .lines()
            .filter_map(|l| l.trim_start().split_once(' '))
            .map(|(_, path)| path.to_string())
            .collect();
        paths.sort();
        paths
    }

    /// Paths `git ls-files -v` marks `h` (assume-unchanged), byte-sorted.
    pub fn assume_unchanged_paths(&self) -> Vec<String> {
        self.ls_files_with("h ")
    }

    /// Paths `git ls-files -v` marks `H` (cached, not assume-unchanged).
    pub fn tracked_paths(&self) -> Vec<String> {
        self.ls_files_with("H ")
    }

    fn ls_files_with(&self, prefix: &str) -> Vec<String> {
        let text = self.git_in_project(&["ls-files", "-v"]);
        let mut paths: Vec<String> = text
            .lines()
            .filter_map(|l| l.strip_prefix(prefix))
            .map(str::to_string)
            .collect();
        paths.sort();
        paths
    }
}

/// The in-scope files of `dir` (skipping `.git`) as `(path, bytes)`, byte
/// sorted: what a gateway export of that project directory holds.
pub fn members_of(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    collect(dir, "", &mut files);
    files.retain(|(rel, _)| in_scope(rel));
    files.sort();
    files
}

/// A project zip of the in-scope files below `dir` (skipping `.git`), the
/// way the fake gateway answers an export.
pub fn live_export_from(dir: &Path) -> Vec<u8> {
    let mut files = Vec::new();
    collect(dir, "", &mut files);
    files.retain(|(rel, _)| in_scope(rel));
    files.sort();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        writer
            .start_file(&name, zip::write::SimpleFileOptions::default())
            .expect("start");
        writer.write_all(&bytes).expect("zip write");
    }
    writer.finish().expect("finish").into_inner()
}

/// A `resource.json` the way the gateway re-serialises it: pretty-printed,
/// `files` reversed and `attributes.lastModification` stamped. The content is
/// otherwise equal, so it must not read as a change.
pub fn gateway_form(resource_json: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(resource_json).expect("json");
    let object = value.as_object_mut().expect("object");
    if let Some(files) = object.get_mut("files").and_then(|f| f.as_array_mut()) {
        files.reverse();
    }
    object.insert(
        "attributes".to_string(),
        serde_json::json!({
            "lastModification": {"actor": "designer", "timestamp": "2026-11-13T10:00:00Z"},
            "lastModificationSignature": "sig"
        }),
    );
    let mut text = serde_json::to_string_pretty(&value).expect("pretty");
    text.push('\n');
    text
}

/// Write raw `bytes` to `root/rel`, creating folders.
fn put_bytes(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, bytes).expect("write");
}

/// Every file below `dir` (skipping `.git`) as `(relative path, bytes)`.
fn collect(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.path().is_dir() {
            collect(&entry.path(), &rel, out);
        } else {
            out.push((rel, std::fs::read(entry.path()).expect("read")));
        }
    }
}
