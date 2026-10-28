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
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::actions::workspace::{now_unix_ms, rfc3339_now_utc, utc_parts};
use crate::deploy::component_problem;
use crate::error::{CoreError, PreflightFailure};

/// Where run directories live below the repo (D-05). Users' `.gitignore`
/// files depend on this path.
pub const RUN_DIR_ROOT: &str = ".ign/deploy";

/// The check id a refused run directory is reported under (23-02, item 8).
const RUN_DIR_CHECK: &str = "run_dir_not_ignored";

/// How many ids are drawn before [`RunDir::create_fresh`] gives up.
const CREATE_ATTEMPTS: usize = 3;

/// The longest run id accepted (the staging rule).
const RUN_ID_MAX: usize = 128;

/// One run's identifier: a UTC timestamp `YYYYMMDDTHHMMSSZ`, `-` and six
/// lowercase hex characters (`20261015T142233Z-a1b2c3`).
///
/// The fixed-width timestamp prefix makes ids sort by start time as strings;
/// the random suffix separates two runs started in the same second. Any text
/// matching the staging rule `[A-Za-z0-9._-]{1,128}` without a leading dot
/// also parses, so an id read back from a record or a command line is held to
/// the rule before it becomes a path component (T-23-10).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RunId(String);

impl RunId {
    /// A fresh id for now.
    pub fn generate() -> RunId {
        Self::generate_at(now_unix_ms())
    }

    /// A fresh id for the instant `ms` (epoch milliseconds).
    #[doc(hidden)]
    pub fn generate_at(ms: i64) -> RunId {
        let (y, mo, d, h, mi, s) = utc_parts(ms);
        let mut random = [0u8; 3];
        getrandom::getrandom(&mut random).expect("the OS random source is available");
        RunId(format!(
            "{y:04}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z-{:02x}{:02x}{:02x}",
            random[0], random[1], random[2]
        ))
    }

    /// Validate `text` against the staging rule.
    pub fn parse(text: &str) -> Result<RunId, CoreError> {
        let ok = (1..=RUN_ID_MAX).contains(&text.len())
            && !text.starts_with('.')
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if ok {
            Ok(RunId(text.to_string()))
        } else {
            Err(CoreError::InvalidInput {
                reason: format!(
                    "a run id is 1 to {RUN_ID_MAX} characters of [A-Za-z0-9._-] and does not start with a dot"
                ),
            })
        }
    }

    /// The id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `.ign/deploy/<gateway>/<run-id>` below the repo. `gateway` must already
/// be a single path component ([`RunDir::create`] checks it).
pub fn run_dir_rel(gateway: &str, run_id: &RunId) -> String {
    format!("{RUN_DIR_ROOT}/{gateway}/{}", run_id.as_str())
}

/// What `git check-ignore` said about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreStatus {
    /// Git would not commit it (exit 0).
    Ignored,
    /// Git would commit it (exit 1): refuse, nothing written.
    NotIgnored,
    /// The repo is not a git work tree (exit 128, and no `.git` at or above
    /// it): allowed and recorded.
    NotARepo,
    /// git could not be run, or answered something else (including exit 128
    /// under a `.git` entry): refuse (fail closed).
    GitUnavailable,
}

impl IgnoreStatus {
    /// The value recorded as `run_dir_git` in `run.json` (23-02, item 6).
    pub fn as_record(self) -> &'static str {
        match self {
            IgnoreStatus::Ignored => "ignored",
            IgnoreStatus::NotIgnored => "not_ignored",
            IgnoreStatus::NotARepo => "not_a_repo",
            IgnoreStatus::GitUnavailable => "git_unavailable",
        }
    }
}

/// Whether git ignores `rel` (a path below `repo`), by
/// `git -C <repo> check-ignore -q -- <rel>`.
pub fn ignore_status(repo: &Path, rel: &str) -> IgnoreStatus {
    ignore_status_with(OsStr::new("git"), repo, rel)
}

/// [`ignore_status`] with the git program named (the test seam).
#[doc(hidden)]
pub fn ignore_status_with(git: &OsStr, repo: &Path, rel: &str) -> IgnoreStatus {
    let status = std::process::Command::new(git)
        .arg("-C")
        .arg(repo)
        .args(["check-ignore", "-q", "--"])
        .arg(rel)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status.map(|s| s.code()) {
        Ok(Some(0)) => IgnoreStatus::Ignored,
        Ok(Some(1)) => IgnoreStatus::NotIgnored,
        // 128 is "not a repo", but also "dubious ownership", a corrupt repo
        // and other fatal errors. Only a directory with no `.git` at or above
        // it is a genuine non-repo; under a `.git`, refuse.
        Ok(Some(128)) if !has_git_entry(repo) => IgnoreStatus::NotARepo,
        _ => IgnoreStatus::GitUnavailable,
    }
}

/// Whether `start` or any ancestor holds a `.git` entry (file or directory,
/// not followed). An entry that cannot be inspected counts as present.
fn has_git_entry(start: &Path) -> bool {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_path_buf());
    start.ancestors().any(|dir| {
        !matches!(
            std::fs::symlink_metadata(dir.join(".git")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        )
    })
}

/// A created run directory.
#[derive(Debug)]
pub struct RunDir {
    path: PathBuf,
    git: IgnoreStatus,
}

/// The result of one exclusive create: the directory, or `None` when the path
/// was already taken.
type Created = Result<Option<RunDir>, CoreError>;

impl RunDir {
    /// Create the run directory for `run_id`: the git-ignore guard first,
    /// then the parents (0700 when new) and the directory itself
    /// (exclusively, 0700). A path that already exists is
    /// `deploy_run_invalid`; an existing run directory is never reused.
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
        Self::try_create(git, repo, gateway, run_id)?.ok_or_else(|| CoreError::DeployRunInvalid {
            run_id: run_id.as_str().to_string(),
            problem: "the run directory already exists".to_string(),
        })
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
        let mut last = String::new();
        for _ in 0..CREATE_ATTEMPTS {
            let id = next_id();
            if let Some(dir) = Self::try_create(git, repo, gateway, &id)? {
                return Ok((id, dir));
            }
            last = id.as_str().to_string();
        }
        Err(CoreError::DeployRunInvalid {
            run_id: last,
            problem: format!(
                "every one of {CREATE_ATTEMPTS} run ids drawn already had a run directory"
            ),
        })
    }

    fn try_create(git: &OsStr, repo: &Path, gateway: &str, run_id: &RunId) -> Created {
        if let Some(why) = component_problem(gateway) {
            return Err(CoreError::InvalidInput {
                reason: format!("the gateway directory name {why}"),
            });
        }
        // `component_problem` allows `:`, but on Windows `C:x` replaces the
        // base path when pushed and `gw:stream` names an alternate data stream.
        if gateway.contains(':') {
            return Err(CoreError::InvalidInput {
                reason: "the gateway directory name may not contain a colon".to_string(),
            });
        }
        let rel = run_dir_rel(gateway, run_id);
        let status = ignore_status_with(git, repo, &rel);
        let detail = match status {
            IgnoreStatus::NotIgnored => Some(format!(
                "git would commit {rel}, which holds the pre-apply snapshot and the backup; add `.ign/` to .gitignore"
            )),
            IgnoreStatus::GitUnavailable => Some(
                "git could not be run to check that the run directory is ignored, so none was created"
                    .to_string(),
            ),
            IgnoreStatus::Ignored | IgnoreStatus::NotARepo => None,
        };
        if let Some(detail) = detail {
            return Err(CoreError::DeployPreflightFailed {
                failures: vec![PreflightFailure {
                    check: RUN_DIR_CHECK.to_string(),
                    detail,
                }]
                .into(),
            });
        }

        let fail = |what: &str, e: &std::io::Error| CoreError::DeployRunInvalid {
            run_id: run_id.as_str().to_string(),
            problem: format!("could not {what} ({:?})", e.kind()),
        };
        // Every component below the repo, each created at 0700 when new.
        let mut path = repo.to_path_buf();
        let components: Vec<&str> = rel.split('/').collect();
        let (leaf, parents) = components.split_last().expect("rel has components");
        for part in parents {
            path.push(part);
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    harden_dir(&path).map_err(|e| fail("restrict a run directory parent", &e))?
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // `git check-ignore` vetted the repo-relative path, so an
                    // existing parent must be a real directory: a symlink
                    // would send the run's files somewhere git never checked.
                    let real_dir = std::fs::symlink_metadata(&path)
                        .map(|m| m.file_type().is_dir())
                        .unwrap_or(false);
                    if !real_dir {
                        return Err(CoreError::DeployRunInvalid {
                            run_id: run_id.as_str().to_string(),
                            problem: format!(
                                "`{}` is a symlink or not a directory",
                                path.strip_prefix(repo).unwrap_or(&path).display()
                            ),
                        });
                    }
                }
                Err(e) => return Err(fail("create a run directory parent", &e)),
            }
        }
        path.push(leaf);
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
            Err(e) => return Err(fail("create the run directory", &e)),
        }
        // Explicit, so the umask cannot widen it.
        if let Err(e) = harden_dir(&path) {
            // Nothing was written into it yet; do not leave a loose directory.
            let _ = std::fs::remove_dir(&path);
            return Err(fail("restrict the run directory", &e));
        }
        Ok(Some(RunDir { path, git: status }))
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What git said about this directory before it was created
    /// (`run_dir_git` in `run.json`).
    pub fn git_status(&self) -> IgnoreStatus {
        self.git
    }

    /// Create `name` in the directory exclusively at mode 0600 and return the
    /// handle. `name` is a plain file name: no separator, not dot-led.
    pub fn create_private_file(&self, name: &str) -> Result<std::fs::File, CoreError> {
        if component_problem(name).is_some() {
            return Err(CoreError::InvalidInput {
                reason: "a run directory file name is one plain name, with no separator and no leading dot"
                    .to_string(),
            });
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(self.path.join(name)).map_err(|e| {
            let problem = if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("{name} already exists and is written once")
            } else {
                format!("could not create {name} ({:?})", e.kind())
            };
            CoreError::DeployRunInvalid {
                run_id: self.run_id_text(),
                problem,
            }
        })
    }

    /// Create `name` at mode 0600 inside the subdirectory `dir` of the run
    /// directory, creating `dir` (0700) when it is not there yet, and return
    /// the handle. Both are plain names: no separator, not dot-led. `dir`
    /// must be a real directory; a symlink under that name is refused.
    pub fn create_private_file_in(
        &self,
        dir: &str,
        name: &str,
    ) -> Result<std::fs::File, CoreError> {
        if component_problem(dir).is_some() || component_problem(name).is_some() {
            return Err(CoreError::InvalidInput {
                reason: "a run directory file name is one plain name, with no separator and no leading dot"
                    .to_string(),
            });
        }
        let sub = self.path.join(dir);
        let invalid = |problem: String| CoreError::DeployRunInvalid {
            run_id: self.run_id_text(),
            problem,
        };
        match std::fs::create_dir(&sub) {
            Ok(()) => harden_dir(&sub)
                .map_err(|e| invalid(format!("could not restrict {dir} ({:?})", e.kind())))?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let real_dir = std::fs::symlink_metadata(&sub)
                    .map(|m| m.file_type().is_dir())
                    .unwrap_or(false);
                if !real_dir {
                    return Err(invalid(format!("{dir} is a symlink or not a directory")));
                }
            }
            Err(e) => return Err(invalid(format!("could not create {dir} ({:?})", e.kind()))),
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(sub.join(name)).map_err(|e| {
            invalid(if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("{dir}/{name} already exists and is written once")
            } else {
                format!("could not create {dir}/{name} ({:?})", e.kind())
            })
        })
    }

    /// Write `value` as pretty JSON plus LF to `name`, once: a second write
    /// of the same name fails and leaves the first untouched.
    pub fn write_json(&self, name: &str, value: &impl Serialize) -> Result<(), CoreError> {
        let mut bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| CoreError::Internal(format!("could not serialize {name}: {e}")))?;
        bytes.push(b'\n');
        let mut file = self.create_private_file(name)?;
        file.write_all(&bytes)
            .and_then(|()| file.flush())
            .map_err(|e| CoreError::DeployRunInvalid {
                run_id: self.run_id_text(),
                problem: format!("could not write {name} ({:?})", e.kind()),
            })
    }

    fn run_id_text(&self) -> String {
        self.path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string()
    }
}

/// Owner-only access for a directory this module just created.
#[cfg(unix)]
fn harden_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

/// Owner-only access for a directory this module just created: inheritance
/// removed and one inheritable full-control grant (object and container
/// inherit, so the files created inside are owner-only too) to the SID of the
/// account this process runs as (G-25-10: a service account has no name
/// `icacls` can map). Any failure refuses the run, because the directory holds
/// secrets.
#[cfg(windows)]
fn harden_dir(path: &Path) -> std::io::Result<()> {
    crate::deploy::owner_acl::restrict_to_current_user(path, "(OI)(CI)F").map_err(|why| {
        std::io::Error::other(format!(
            "cannot restrict the run directory to the current user: {why}"
        ))
    })
}

/// No owner-only mechanism exists for this platform: refuse rather than
/// create a directory for secrets that anyone could read.
#[cfg(not(any(unix, windows)))]
fn harden_dir(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no owner-only directory mode on this platform",
    ))
}

/// The append-only `events.ndjson` of one run.
///
/// Lines carry value-free fields only: paths, hashes, ids, counts and check
/// ids, never a file's content, a secret or a gateway response body (T-23-13).
/// The caller owns that rule; the log cannot see what a field means.
#[derive(Debug)]
pub struct EventLog {
    file: std::fs::File,
    seq: u64,
}

impl EventLog {
    /// Create `events.ndjson` in `dir` (once per run directory).
    pub fn open(dir: &RunDir) -> Result<EventLog, CoreError> {
        Ok(EventLog {
            file: dir.create_private_file("events.ndjson")?,
            seq: 0,
        })
    }

    /// A log over an arbitrary handle, so a test can hand it one that cannot
    /// be written (a file opened read-only).
    #[cfg(test)]
    pub(crate) fn from_file(file: std::fs::File) -> EventLog {
        EventLog { file, seq: 0 }
    }

    /// Append one line `{"seq":N,"t":"<rfc3339>","event":"<event>",<fields>}`.
    /// `seq` counts 1, 2, 3 in emission order. `fields` is a JSON object (or
    /// null for none) and may not use the keys `seq`, `t` or `event`.
    pub fn append(&mut self, event: &str, fields: serde_json::Value) -> Result<(), CoreError> {
        let extra = match fields {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            _ => {
                return Err(CoreError::Internal(
                    "an event's fields are a JSON object".to_string(),
                ));
            }
        };
        let mut line = serde_json::Map::new();
        line.insert("seq".to_string(), (self.seq + 1).into());
        line.insert("t".to_string(), rfc3339_now_utc().into());
        line.insert("event".to_string(), event.into());
        for (key, value) in extra {
            if line.contains_key(&key) {
                return Err(CoreError::Internal(format!(
                    "an event's fields may not use the reserved key `{key}`"
                )));
            }
            line.insert(key, value);
        }
        let mut bytes = serde_json::to_vec(&serde_json::Value::Object(line))
            .map_err(|e| CoreError::Internal(format!("could not serialize an event: {e}")))?;
        bytes.push(b'\n');
        self.file
            .write_all(&bytes)
            .and_then(|()| self.file.flush())
            .map_err(|e| {
                CoreError::Internal(format!(
                    "could not append to the events log ({:?})",
                    e.kind()
                ))
            })?;
        self.seq += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Command;

    /// 25-11: a file in a subdirectory of the run directory is private and the
    /// subdirectory is 0700; names are plain, a file is written once and a
    /// symlink where the subdirectory belongs is refused.
    #[test]
    fn create_private_file_in_makes_a_private_subdirectory() {
        let repo = git_repo(Some(".ign/\n"));
        let (_, dir) = RunDir::create_fresh(repo.path(), "gw").expect("run dir");
        let mut file = dir
            .create_private_file_in("export", "CapApp.zip")
            .expect("file");
        std::io::Write::write_all(&mut file, b"zip").expect("write");
        let sub = dir.path().join("export");
        assert_eq!(std::fs::read(sub.join("CapApp.zip")).expect("read"), b"zip");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode(&sub), 0o700);
            assert_eq!(mode(&sub.join("CapApp.zip")), 0o600);
        }
        // Written once.
        let err = dir
            .create_private_file_in("export", "CapApp.zip")
            .expect_err("second write");
        assert_eq!(err.code(), "deploy_run_invalid");
        // A second file reuses the directory.
        dir.create_private_file_in("export", "Other.zip")
            .expect("second file");
        // Plain names only.
        for (d, n) in [
            ("a/b", "x"),
            ("export", "../x"),
            (".hidden", "x"),
            ("export", ""),
        ] {
            let err = dir.create_private_file_in(d, n).expect_err("plain names");
            assert_eq!(err.code(), "invalid_input", "{d:?} {n:?}");
        }
        #[cfg(unix)]
        {
            let elsewhere = tempfile::tempdir().expect("elsewhere");
            std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("link")).expect("link");
            let err = dir
                .create_private_file_in("link", "x.zip")
                .expect_err("a symlink is not a directory");
            assert_eq!(err.code(), "deploy_run_invalid");
            assert_eq!(std::fs::read_dir(elsewhere.path()).expect("dir").count(), 0);
        }
    }

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
        for ok in [
            "20261015T142233Z-a1b2c3",
            "run_1.x-2",
            "a",
            &"a".repeat(128),
        ] {
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
        assert!(
            a.as_str().starts_with("20261015T142233Z-"),
            "{}",
            a.as_str()
        );
        assert!(
            b.as_str().starts_with("20261015T142234Z-"),
            "{}",
            b.as_str()
        );
        assert!(
            c.as_str().starts_with("20261016T142233Z-"),
            "{}",
            c.as_str()
        );
        let mut ids = vec![c.clone(), a.clone(), b.clone()];
        ids.sort();
        assert_eq!(ids, vec![a, b, c]);
    }

    #[test]
    fn run_ids_in_one_second_differ_by_suffix() {
        let ids: std::collections::BTreeSet<String> = (0..64)
            .map(|_| RunId::generate_at(1_792_074_153_000).as_str().to_string())
            .collect();
        assert!(
            ids.len() > 60,
            "six random hex digits collided: {}",
            ids.len()
        );
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
        log.append("started", json!({"count": 2}))
            .expect("append 1");
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

    /// The run directory's ACL names the current user and no broad group.
    #[cfg(windows)]
    #[test]
    fn run_dir_created_private_windows() {
        let repo = git_repo(Some(".ign/\n"));
        let dir = RunDir::create(repo.path(), "gw", &id("r1")).expect("create");
        let out = Command::new("icacls")
            .arg(dir.path())
            .output()
            .expect("icacls runs");
        let acl = String::from_utf8_lossy(&out.stdout).to_string();
        let user = std::env::var("USERNAME").expect("USERNAME");
        assert!(acl.contains(&user), "{acl}");
        for broad in ["Everyone", "BUILTIN\\Users", "Authenticated Users"] {
            assert!(!acl.contains(broad), "{broad} still has access: {acl}");
        }
        let grants: Vec<&str> = acl.lines().filter(|l| l.contains(":(")).collect();
        assert_eq!(grants.len(), 1, "{acl}");
        assert!(grants[0].contains("(OI)(CI)(F)"), "{acl}");
        dir.write_json("run.json", &json!({"a": 1})).expect("write");
        let text = std::fs::read_to_string(dir.path().join("run.json")).expect("readable");
        assert!(text.contains("\"a\""), "{text}");
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
        assert!(
            !ignored.path().join(".ign").exists(),
            "nothing may be written"
        );
    }

    #[test]
    fn not_a_repo_is_allowed_and_recorded() {
        let plain = tempfile::tempdir().expect("tempdir");
        let dir = RunDir::create(plain.path(), "gw", &id("r1")).expect("create");
        assert_eq!(dir.git_status(), IgnoreStatus::NotARepo);
        assert!(dir.path().is_dir());
    }

    /// A `.git` entry that points nowhere: git exits 128 on it, as it does for
    /// "dubious ownership" and a corrupt repo, but the directory is a repo.
    fn broken_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(".git"),
            "gitdir: /definitely/not/a/gitdir\n",
        )
        .expect("write .git");
        dir
    }

    #[test]
    fn git_fatal_inside_a_repo_is_not_a_non_repo() {
        let rel = ".ign/deploy/gw/r1";
        let broken = broken_repo();
        assert_eq!(
            ignore_status(broken.path(), rel),
            IgnoreStatus::GitUnavailable,
            "exit 128 under a `.git` entry must fail closed"
        );
        // A subdirectory of a repo is just as much a repo.
        let sub = broken.path().join("sub");
        std::fs::create_dir(&sub).expect("mkdir");
        assert_eq!(ignore_status(&sub, rel), IgnoreStatus::GitUnavailable);

        let err = RunDir::create(broken.path(), "gw", &id("r1")).expect_err("fail closed");
        assert_eq!(err.code(), "deploy_preflight_failed");
        assert!(
            !broken.path().join(".ign").exists(),
            "nothing may be written"
        );

        // No `.git` here or above: a genuine non-repo (skipped on a host whose
        // temp dir sits inside a repo).
        let plain = tempfile::tempdir().expect("tempdir");
        if !has_git_entry(plain.path()) {
            assert_eq!(ignore_status(plain.path(), rel), IgnoreStatus::NotARepo);
        }
    }

    #[test]
    fn has_git_entry_looks_at_the_directory_and_its_ancestors() {
        let broken = broken_repo();
        assert!(has_git_entry(broken.path()));
        let deep = broken.path().join("a/b");
        std::fs::create_dir_all(&deep).expect("mkdir");
        assert!(has_git_entry(&deep));
        let real = git_repo(None);
        assert!(has_git_entry(real.path()));
    }

    #[test]
    fn a_gateway_with_a_colon_is_refused_before_any_io() {
        let repo = git_repo(Some(".ign/\n"));
        for bad in ["C:evil", "gw:stream", "gw:", ":gw"] {
            let err = RunDir::create(repo.path(), bad, &id("r1")).expect_err(bad);
            assert_eq!(err.code(), "invalid_input", "{bad:?}");
        }
        assert!(!repo.path().join(".ign").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_parent_is_refused_and_nothing_is_written_through_it() {
        use std::os::unix::fs::symlink;
        // `.ign`, `.ign/deploy` and `.ign/deploy/gw` each as the symlink.
        for depth in 1..=3 {
            let repo = tempfile::tempdir().expect("tempdir");
            if has_git_entry(repo.path()) {
                continue; // git would answer first; see the guard tests
            }
            let outside = tempfile::tempdir().expect("tempdir");
            let parts = [".ign", "deploy", "gw"];
            let mut at = repo.path().to_path_buf();
            for part in &parts[..depth - 1] {
                at.push(part);
                std::fs::create_dir(&at).expect("mkdir");
            }
            symlink(outside.path(), at.join(parts[depth - 1])).expect("symlink");

            let err = RunDir::create(repo.path(), "gw", &id("r1")).expect_err("symlink");
            assert_eq!(err.code(), "deploy_run_invalid", "depth {depth}: {err}");
            assert!(err.to_string().contains("symlink"), "{err}");
            assert_eq!(
                std::fs::read_dir(outside.path()).expect("ls").count(),
                0,
                "depth {depth}: nothing may be written through the symlink"
            );
        }
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
        let (drawn, second) =
            RunDir::create_fresh_with(OsStr::new("git"), repo.path(), "gw", move || {
                ids.pop().expect("an id")
            })
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
