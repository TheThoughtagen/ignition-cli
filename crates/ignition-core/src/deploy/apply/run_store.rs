//! The run store: run ids, the private run directory, its git-ignore guard
//! and the events log (Phase 23, D-05, D-06).
//!
//! Every apply and rollback run gets one directory,
//! `<repo>/.ign/deploy/<gateway>/<run-id>/` ([`run_dir_rel`]). It holds
//! secrets: the pre-apply snapshot of the gateway's files and the `.gwbk`
//! backup. So the directory is created exclusively at mode 0700 (files 0600;
//! an owner-only ACL on Windows), and only after [`ignore_status`] has shown
//! that git would not commit it: an answer of [`IgnoreStatus::NotIgnored`] or
//! [`IgnoreStatus::GitUnavailable`] refuses with nothing written.
//! [`IgnoreStatus::NotARepo`] is allowed and recorded
//! ([`RunDir::git_status`]).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::CoreError;

/// Where run directories live below the repo (D-05). Users' `.gitignore`
/// files depend on this path.
pub const RUN_DIR_ROOT: &str = ".ign/deploy";

/// One run's identifier: `YYYYMMDDTHHMMSSZ-xxxxxx`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RunId(String);

impl RunId {
    /// A fresh id for now.
    pub fn generate() -> RunId {
        RunId(String::new())
    }

    /// A fresh id for the instant `ms` (epoch milliseconds).
    #[doc(hidden)]
    pub fn generate_at(ms: i64) -> RunId {
        let _ = ms;
        RunId(String::new())
    }

    /// Validate `text` against the staging rule.
    pub fn parse(text: &str) -> Result<RunId, CoreError> {
        let _ = text;
        Err(CoreError::Internal("run store not built".to_string()))
    }

    /// The id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `.ign/deploy/<gateway>/<run-id>` below the repo.
pub fn run_dir_rel(gateway: &str, run_id: &RunId) -> String {
    let _ = (gateway, run_id);
    String::new()
}

/// What `git check-ignore` said about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreStatus {
    /// Git would not commit it.
    Ignored,
    /// Git would commit it: refuse.
    NotIgnored,
    /// The repo is not a git work tree: allowed and recorded.
    NotARepo,
    /// git could not be run: refuse (fail closed).
    GitUnavailable,
}

/// Whether git ignores `rel` below `repo`.
pub fn ignore_status(repo: &Path, rel: &str) -> IgnoreStatus {
    ignore_status_with(OsStr::new("git"), repo, rel)
}

/// [`ignore_status`] with the git program named (the test seam).
#[doc(hidden)]
pub fn ignore_status_with(git: &OsStr, repo: &Path, rel: &str) -> IgnoreStatus {
    let _ = (git, repo, rel);
    IgnoreStatus::GitUnavailable
}

/// A created run directory.
#[derive(Debug)]
pub struct RunDir {
    path: PathBuf,
    git: IgnoreStatus,
}

impl RunDir {
    /// Create the run directory for `run_id`.
    pub fn create(repo: &Path, gateway: &str, run_id: &RunId) -> Result<RunDir, CoreError> {
        Self::create_with_git(OsStr::new("git"), repo, gateway, run_id)
    }

    /// [`RunDir::create`] with the git program named (the test seam).
    #[doc(hidden)]
    pub fn create_with_git(
        git: &OsStr,
        repo: &Path,
        gateway: &str,
        run_id: &RunId,
    ) -> Result<RunDir, CoreError> {
        let _ = (git, repo, gateway, run_id);
        Err(CoreError::Internal("run store not built".to_string()))
    }

    /// Create a directory under a fresh id, drawing a new id when the path
    /// exists (at most three attempts).
    pub fn create_fresh(repo: &Path, gateway: &str) -> Result<(RunId, RunDir), CoreError> {
        Self::create_fresh_with(OsStr::new("git"), repo, gateway, RunId::generate)
    }

    /// [`RunDir::create_fresh`] with the git program and the id source named
    /// (the test seam).
    #[doc(hidden)]
    pub fn create_fresh_with(
        git: &OsStr,
        repo: &Path,
        gateway: &str,
        mut next_id: impl FnMut() -> RunId,
    ) -> Result<(RunId, RunDir), CoreError> {
        let _ = (git, repo, gateway, next_id());
        Err(CoreError::Internal("run store not built".to_string()))
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What git said about this directory before it was created.
    pub fn git_status(&self) -> IgnoreStatus {
        self.git
    }

    /// Create `name` in the directory exclusively at mode 0600.
    pub fn create_private_file(&self, name: &str) -> Result<std::fs::File, CoreError> {
        let _ = name;
        Err(CoreError::Internal("run store not built".to_string()))
    }

    /// Write `value` as pretty JSON plus LF to `name`, once.
    pub fn write_json(&self, name: &str, value: &impl Serialize) -> Result<(), CoreError> {
        let _ = (name, value);
        Err(CoreError::Internal("run store not built".to_string()))
    }
}

/// The append-only `events.ndjson` of one run.
#[derive(Debug)]
pub struct EventLog {
    _file: Option<std::fs::File>,
}

impl EventLog {
    /// Create `events.ndjson` in `dir`.
    pub fn open(dir: &RunDir) -> Result<EventLog, CoreError> {
        let _ = dir;
        Err(CoreError::Internal("run store not built".to_string()))
    }

    /// Append one line `{"seq":N,"t":...,"event":...,<fields>}`.
    pub fn append(&mut self, event: &str, fields: serde_json::Value) -> Result<(), CoreError> {
        let _ = (event, fields);
        Err(CoreError::Internal("run store not built".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Command;

    fn git_repo(gitignore: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = Command::new("git")
            .args(["init", "-q"])
            .arg(dir.path())
            .status()
            .expect("git runs");
        assert!(status.success());
        if let Some(body) = gitignore {
            std::fs::write(dir.path().join(".gitignore"), body).expect("write .gitignore");
        }
        dir
    }

    fn id(text: &str) -> RunId {
        RunId::parse(text).expect("valid id")
    }

    #[test]
    fn run_id_shape() {
        let generated = RunId::generate();
        let s = generated.as_str();
        let bytes = s.as_bytes();
        assert_eq!(s.len(), 23, "{s}");
        assert!(bytes[..8].iter().all(u8::is_ascii_digit), "{s}");
        assert_eq!(bytes[8], b'T', "{s}");
        assert!(bytes[9..15].iter().all(u8::is_ascii_digit), "{s}");
        assert_eq!(bytes[15], b'Z', "{s}");
        assert_eq!(bytes[16], b'-', "{s}");
        assert!(
            bytes[17..]
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)),
            "{s}"
        );
        assert_eq!(RunId::parse(s).expect("generated id parses"), generated);
        for ok in ["20261015T142233Z-a1b2c3", "run_1.x-2", "a", &"a".repeat(128)] {
            RunId::parse(ok).unwrap_or_else(|e| panic!("{ok}: {e}"));
        }
        for bad in [
            "",
            &"a".repeat(129),
            ".hidden",
            "a/b",
            "..",
            "a b",
            "a\\b",
            "a\0b",
        ] {
            let err = RunId::parse(bad).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
        }
    }

    #[test]
    fn run_ids_sort_by_time() {
        // 2026-10-15T14:22:33Z and one second later, then a later day.
        let a = RunId::generate_at(1_792_074_153_000);
        let b = RunId::generate_at(1_792_074_154_000);
        let c = RunId::generate_at(1_792_160_553_000);
        assert!(a.as_str().starts_with("20261015T142233Z-"), "{}", a.as_str());
        assert!(b.as_str().starts_with("20261015T142234Z-"), "{}", b.as_str());
        assert!(c.as_str().starts_with("20261016T142233Z-"), "{}", c.as_str());
        let mut ids = vec![c.clone(), a.clone(), b.clone()];
        ids.sort();
        assert_eq!(ids, vec![a, b, c]);
    }

    #[test]
    fn run_ids_in_one_second_differ_by_suffix() {
        let ids: std::collections::BTreeSet<String> = (0..64)
            .map(|_| RunId::generate_at(1_792_074_153_000).as_str().to_string())
            .collect();
        assert!(ids.len() > 60, "six random hex digits collided: {}", ids.len());
    }

    #[test]
    fn run_dir_rel_is_under_the_root() {
        let rel = run_dir_rel("gw", &id("20261015T142233Z-a1b2c3"));
        assert_eq!(rel, ".ign/deploy/gw/20261015T142233Z-a1b2c3");
        assert!(rel.starts_with(RUN_DIR_ROOT));
    }

    #[cfg(unix)]
    #[test]
    fn run_dir_created_private() {
        use std::os::unix::fs::PermissionsExt;
        let repo = git_repo(Some(".ign/\n"));
        let run = id("20261015T142233Z-a1b2c3");
        let dir = RunDir::create(repo.path(), "gw", &run).expect("create");
        assert_eq!(dir.git_status(), IgnoreStatus::Ignored);
        assert_eq!(
            dir.path(),
            repo.path().join(".ign/deploy/gw/20261015T142233Z-a1b2c3")
        );
        let mode = |p: &Path| std::fs::metadata(p).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode(dir.path()), 0o700);
        for parent in [".ign", ".ign/deploy", ".ign/deploy/gw"] {
            assert_eq!(mode(&repo.path().join(parent)), 0o700, "{parent}");
        }

        dir.write_json("run.json", &json!({"run_id": run.as_str()}))
            .expect("first write");
        let file = dir.path().join("run.json");
        assert_eq!(mode(&file), 0o600);
        let text = std::fs::read_to_string(&file).expect("read");
        assert!(text.ends_with("}\n"), "{text:?}");
        let again = dir
            .write_json("run.json", &json!({"run_id": "other"}))
            .expect_err("second write must fail");
        assert_eq!(again.code(), "deploy_run_invalid");
        assert_eq!(std::fs::read_to_string(&file).expect("read"), text);

        let mut log = EventLog::open(&dir).expect("open log");
        log.append("started", json!({"count": 2})).expect("append 1");
        log.append("finished", json!({})).expect("append 2");
        let log_path = dir.path().join("events.ndjson");
        assert_eq!(mode(&log_path), 0o600);
        let lines: Vec<serde_json::Value> = std::fs::read_to_string(&log_path)
            .expect("read log")
            .lines()
            .map(|l| serde_json::from_str(l).expect("each line is JSON"))
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["seq"], 1);
        assert_eq!(lines[1]["seq"], 2);
        assert_eq!(lines[0]["event"], "started");
        assert_eq!(lines[0]["count"], 2);
        assert!(lines[0]["t"].as_str().is_some_and(|t| t.ends_with('Z')));
    }

    #[test]
    fn event_log_refuses_reserved_keys_and_a_second_open() {
        let repo = git_repo(Some(".ign/\n"));
        let dir = RunDir::create(repo.path(), "gw", &id("r1")).expect("create");
        let mut log = EventLog::open(&dir).expect("open");
        for key in ["seq", "t", "event"] {
            let err = log
                .append("x", json!({ key: 1 }))
                .expect_err("reserved key must be refused");
            assert_eq!(err.code(), "internal", "{key}");
        }
        log.append("ok", json!({"n": 1})).expect("append");
        EventLog::open(&dir).expect_err("the log is created once");
        let text = std::fs::read_to_string(dir.path().join("events.ndjson")).expect("read");
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn write_json_refuses_a_name_with_a_separator() {
        let repo = git_repo(Some(".ign/\n"));
        let dir = RunDir::create(repo.path(), "gw", &id("r1")).expect("create");
        for bad in ["", "a/b", "../x", "a\\b", ".hidden"] {
            let err = dir.write_json(bad, &json!({})).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
        }
    }

    #[test]
    fn ignore_guard_matrix() {
        let rel = ".ign/deploy/gw/r1";
        let ignored = git_repo(Some(".ign/\n"));
        assert_eq!(ignore_status(ignored.path(), rel), IgnoreStatus::Ignored);

        let open = git_repo(None);
        assert_eq!(ignore_status(open.path(), rel), IgnoreStatus::NotIgnored);

        let plain = tempfile::tempdir().expect("tempdir");
        assert_eq!(ignore_status(plain.path(), rel), IgnoreStatus::NotARepo);

        let missing = Path::new("/definitely/not/a/git/program");
        assert_eq!(
            ignore_status_with(missing.as_os_str(), ignored.path(), rel),
            IgnoreStatus::GitUnavailable
        );
    }

    #[test]
    fn a_run_dir_is_refused_with_nothing_written_unless_git_ignores_it() {
        let open = git_repo(None);
        let err = RunDir::create(open.path(), "gw", &id("r1")).expect_err("not ignored");
        assert_eq!(err.code(), "deploy_preflight_failed");
        assert!(err.to_string().contains("run_dir_not_ignored"), "{err}");
        assert!(!open.path().join(".ign").exists(), "nothing may be written");

        let ignored = git_repo(Some(".ign/\n"));
        let missing = Path::new("/definitely/not/a/git/program");
        let err = RunDir::create_with_git(missing.as_os_str(), ignored.path(), "gw", &id("r1"))
            .expect_err("git unavailable");
        assert_eq!(err.code(), "deploy_preflight_failed");
        assert!(!ignored.path().join(".ign").exists(), "nothing may be written");
    }

    #[test]
    fn not_a_repo_is_allowed_and_recorded() {
        let plain = tempfile::tempdir().expect("tempdir");
        let dir = RunDir::create(plain.path(), "gw", &id("r1")).expect("create");
        assert_eq!(dir.git_status(), IgnoreStatus::NotARepo);
        assert!(dir.path().is_dir());
    }

    #[test]
    fn a_traversing_gateway_is_refused_before_any_io() {
        let repo = git_repo(Some(".ign/\n"));
        for bad in ["", "..", "a/b", ".hidden"] {
            let err = RunDir::create(repo.path(), bad, &id("r1")).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
        }
        assert!(!repo.path().join(".ign").exists());
    }

    #[test]
    fn run_dir_exclusive() {
        let repo = git_repo(Some(".ign/\n"));
        let taken = id("20261015T142233Z-aaaaaa");
        let first = RunDir::create(repo.path(), "gw", &taken).expect("create");
        first
            .write_json("run.json", &json!({"keep": true}))
            .expect("write");

        let err = RunDir::create(repo.path(), "gw", &taken).expect_err("exists");
        assert_eq!(err.code(), "deploy_run_invalid");

        let mut ids = vec![id("20261015T142233Z-bbbbbb"), taken.clone()];
        let (drawn, second) = RunDir::create_fresh_with(
            OsStr::new("git"),
            repo.path(),
            "gw",
            move || ids.pop().expect("an id"),
        )
        .expect("a new id is drawn");
        assert_eq!(drawn.as_str(), "20261015T142233Z-bbbbbb");
        assert_ne!(second.path(), first.path());
        assert_eq!(
            std::fs::read_to_string(first.path().join("run.json")).expect("read"),
            "{\n  \"keep\": true\n}\n",
            "the existing run directory is never written into"
        );
        assert_eq!(std::fs::read_dir(first.path()).expect("ls").count(), 1);
    }

    #[test]
    fn run_dir_exclusive_gives_up_after_three_attempts() {
        let repo = git_repo(Some(".ign/\n"));
        let taken = id("r-taken");
        RunDir::create(repo.path(), "gw", &taken).expect("create");
        let mut draws = 0;
        let err = RunDir::create_fresh_with(OsStr::new("git"), repo.path(), "gw", || {
            draws += 1;
            taken.clone()
        })
        .expect_err("always taken");
        assert_eq!(err.code(), "deploy_run_invalid");
        assert_eq!(draws, 3);
    }

    #[test]
    fn two_runs_started_at_once_get_two_directories() {
        let repo = git_repo(Some(".ign/\n"));
        let path = repo.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || RunDir::create_fresh(&path, "gw").expect("create"))
            })
            .collect();
        let made: Vec<(RunId, RunDir)> = handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect();
        let dirs: std::collections::BTreeSet<PathBuf> =
            made.iter().map(|(_, d)| d.path().to_path_buf()).collect();
        assert_eq!(dirs.len(), 8, "every run has its own directory");
    }
}
