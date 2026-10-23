//! `impl ProjectHost for SftpTransport` (Phase 25, 25-08): the host half of
//! `deploy projects` on a gateway reached over ssh.
//!
//! ```text
//! <data_dir>/projects/<project>/.git                          probed, never opened
//! ~/ign-deploy/<gateway>/projects/<project>/release.json      the release record
//! ```
//!
//! ## The `.git` probe is a `stat`, not a listing (K9)
//!
//! The 25-03 captures (`## C-H before grant`) showed that an sftp listing of a
//! directory the deploy account cannot traverse answers `not found`, the same
//! words as a path that does not exist, and that `test -e` answers false. Both
//! would read a denied `.git` as "no git module here" and let a deploy run
//! over a project the module still manages (research Pitfall 3, T-25-20). A
//! GNU `stat` names the cause (`No such file or directory` against
//! `Permission denied`), so the Linux probe is one fixed `sh` script over the
//! hardened exec channel that `stat`s the three levels and reports a word per
//! level:
//!
//! | word | meaning |
//! |---|---|
//! | `dir`, `file`, `symlink`, `other` | the entry exists; a symlink is reported, never followed (`stat` without `-L`) |
//! | `none` | `stat` said `No such file or directory` and nothing else |
//! | `skip` | not asked: an earlier level was absent or not a directory |
//!
//! Any other `stat` failure (denied, I/O error, not a directory, no `stat`)
//! makes the script exit non-zero with `stat`'s message on stderr, which the
//! shell turns into `transport_failed`. The script is fixed text; the three
//! paths and the interpreter name travel as separate single-quoted arguments
//! ([`RemoteCommand::posix`]), so no host or repo value is ever interpreted
//! by a shell.
//!
//! ## Windows hosts: `Get-Item -Force`
//!
//! A Windows node runs [`winps::GIT_ENTRY`](crate::deploy::transport::winps::GIT_ENTRY)
//! through `encoded_command`. It asks the same three levels with
//! `Get-Item -LiteralPath ... -Force`: `-Force` sees a Hidden `.git` (the
//! attribute `git init` and the Git module's own directories carry on
//! Windows), `Test-Path` and `[IO.Directory]::Exists` read a denied entry as
//! false (25-03, `## Host grant and prod git.yaml (25-03)`), and only an
//! `ObjectNotFound` error is `none`. A reparse point (junction or symlink) is
//! present and is never opened. The sftp listing is not used for the probe on
//! either family. The data directory and project name become native paths
//! here and travel as parameters of the fixed script, never in its text.

use super::{SftpTransport, windows_native_path};
use crate::deploy::component_problem;
use crate::deploy::ledger::{
    LedgerRoot, RELEASE_RECORD_MAX_BYTES, release_record_location, release_record_rel,
};
use crate::deploy::transport::projects::{GitEntry, ProjectHost, git_entry_location};
use crate::deploy::transport::remote::RemoteCommand;
use crate::deploy::transport::winps::{WinScript, windows_component_problem};
use crate::error::CoreError;

/// The fixed `sh -c` text of the POSIX probe, one line (a remote command
/// argument may hold no control character). `$1` is `<data>/projects`, `$2`
/// the project directory and `$3` its `.git`. `LC_ALL=C` pins the English
/// `strerror` text the script matches. A `stat` failure other than a clean
/// `No such file or directory` is printed to stderr and exits 3.
pub(super) const GIT_PROBE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "t() { if o=$(stat -c %F -- \"$1\" 2>&1); then ",
    "case \"$o\" in directory) r=dir;; \"regular file\"|\"regular empty file\") r=file;; ",
    "\"symbolic link\") r=symlink;; *) r=other;; esac; ",
    "else case \"$o\" in *\": No such file or directory\") r=none;; ",
    "*) printf \"%s\\n\" \"$o\" >&2; exit 3;; esac; fi; }; ",
    "t \"$1\"; a=$r; b=skip; c=skip; ",
    "if [ \"$a\" = dir ]; then t \"$2\"; b=$r; ",
    "if [ \"$b\" = dir ]; then t \"$3\"; c=$r; fi; fi; ",
    "printf 'IGN-RESULT:{\"projects\":\"%s\",\"project\":\"%s\",\"git\":\"%s\"}\\n' \"$a\" \"$b\" \"$c\""
);

/// The `$0` the probe runs under, so a `stat` message names the script.
const GIT_PROBE_NAME: &str = "ign-git-probe";

impl SftpTransport {
    /// The project name as one safe component on this host's family, or the
    /// refusal. Runs before any path is built (T-25-21, T-25-25).
    fn project_name_problem(&self, project: &str) -> Option<String> {
        if let Some(why) = component_problem(project) {
            return Some(format!("the project name {why}"));
        }
        if self.windows
            && let Some(why) = windows_component_problem(project)
        {
            return Some(format!("the project name: {why}"));
        }
        None
    }

    /// The POSIX probe: one `stat` script over the exec channel.
    async fn git_entry_posix(&self, project: &str) -> Result<GitEntry, CoreError> {
        let projects = self.abs(&["projects".to_string()]);
        let project_dir = format!("{projects}/{project}");
        let git = format!("{project_dir}/.git");
        let command = RemoteCommand::posix(&[
            "sh",
            "-c",
            GIT_PROBE_SH,
            GIT_PROBE_NAME,
            &projects,
            &project_dir,
            &git,
        ])
        .map_err(|why| self.fail(format!("the .git probe cannot be built: {why}")))?;
        let answer = self.shell.exec_posix("git_entry", &command).await?;
        let word = |key: &str| answer.get(key).and_then(|v| v.as_str()).unwrap_or("");

        // Level 1: `data/projects`. A missing or odd one means data_dir is not
        // the gateway data directory, or the account cannot see it: never
        // "no git".
        match word("projects") {
            "dir" => {}
            "none" => {
                return Err(
                    self.fail("projects does not exist; is data_dir the gateway data directory?")
                );
            }
            "symlink" => {
                return Err(self.fail("projects is a symlink; symlinks are refused"));
            }
            "file" | "other" => return Err(self.fail("projects is not a directory")),
            other => return Err(self.unexpected("projects", other)),
        }
        // Level 2: the project directory. Missing is the `create` case.
        match word("project") {
            "dir" => {}
            "none" => return Ok(GitEntry::Absent),
            "symlink" => {
                return Err(self.fail(format!(
                    "projects/{project} is a symlink; symlinks are refused"
                )));
            }
            "file" | "other" => {
                return Err(self.fail(format!("projects/{project} is not a directory")));
            }
            other => return Err(self.unexpected("project", other)),
        }
        // Level 3: `.git`, of any kind (K9).
        Ok(match word("git") {
            "none" => GitEntry::Absent,
            "dir" => GitEntry::Directory,
            "file" => GitEntry::File,
            "symlink" => GitEntry::Symlink,
            "other" => GitEntry::Other,
            other => return Err(self.unexpected("git", other)),
        })
    }

    /// The Windows probe: one `Get-Item -Force` script over the encoded
    /// PowerShell route.
    async fn git_entry_windows(&self, project: &str) -> Result<GitEntry, CoreError> {
        let projects = windows_native_path(&self.abs(&["projects".to_string()]));
        let project_dir = format!("{projects}\\{project}");
        let git = format!("{project_dir}\\.git");
        let script = WinScript::git_entry(&projects, &project_dir, &git);
        let answer = self.shell.exec_powershell(&script).await?;
        windows_git_entry(&answer, project).map_err(|why| self.fail(why))
    }

    /// An answer word the probe does not define is a failure, never a kind.
    fn unexpected(&self, level: &str, word: &str) -> CoreError {
        let shown: String = word
            .chars()
            .filter(|c| c.is_ascii_graphic())
            .take(20)
            .collect();
        self.fail(format!(
            "the .git probe answered {shown:?} for {level}, which it does not define"
        ))
    }
}

/// The [`GitEntry`] of a [`winps::GIT_ENTRY`] result, or why it cannot be
/// read. Anything the script does not define, a missing field included, is an
/// error: the result is never guessed into `Absent`.
fn windows_git_entry(answer: &serde_json::Value, project: &str) -> Result<GitEntry, String> {
    let location = git_entry_location(project);
    let bad = |why: String| format!("{location}: the .git probe result is unusable ({why})");
    if answer.get("ok").and_then(|v| v.as_i64()) != Some(1) {
        return Err(bad("ok is not 1".to_string()));
    }
    let word = |key: &str| answer.get(key).and_then(|v| v.as_str());
    // Level 1: `data\projects`. A missing one means data_dir is not the
    // gateway data directory: never "no git".
    match word("projects") {
        Some("dir") => {}
        Some("none") => {
            return Err(format!(
                "{location}: projects does not exist; is data_dir the gateway data directory?"
            ));
        }
        Some("symlink") => {
            return Err(format!(
                "{location}: projects is a junction or symlink; symlinks are refused"
            ));
        }
        Some("file") => return Err(format!("{location}: projects is not a directory")),
        other => return Err(bad(format!("projects is {other:?}"))),
    }
    // Level 2: the project directory. Missing is the `create` case.
    match word("project") {
        Some("dir") => {}
        Some("none") => return Ok(GitEntry::Absent),
        Some("symlink") => {
            return Err(format!(
                "{location}: projects/{project} is a junction or symlink; symlinks are refused"
            ));
        }
        Some("file") => {
            return Err(format!("{location}: projects/{project} is not a directory"));
        }
        other => return Err(bad(format!("project is {other:?}"))),
    }
    // Level 3: `.git`, of any kind; a reparse point is present and not followed.
    let flag = |key: &str| match answer.get(key).and_then(|v| v.as_i64()) {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        other => Err(bad(format!("{key} is {other:?}"))),
    };
    match word("kind") {
        Some("none") => Ok(GitEntry::Absent),
        Some(kind @ ("dir" | "file")) => {
            let _hidden = flag("hidden")?;
            if flag("reparse")? {
                Ok(GitEntry::Symlink)
            } else if kind == "dir" {
                Ok(GitEntry::Directory)
            } else {
                Ok(GitEntry::File)
            }
        }
        other => Err(bad(format!("kind is {other:?}"))),
    }
}

#[async_trait::async_trait]
impl ProjectHost for SftpTransport {
    async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        if self.windows {
            return self.git_entry_windows(project).await;
        }
        self.git_entry_posix(project).await
    }

    async fn read_release_record(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = release_record_location(LedgerRoot::AccountHome, gateway, project);
        self.read_home_file(
            &release_record_rel(gateway, project),
            RELEASE_RECORD_MAX_BYTES,
            "the release record",
            [("gateway name", gateway), ("project name", project)],
            &|problem| CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {problem}"),
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::transport::process::{ProcessOutput, ProcessRunner};
    use crate::deploy::transport::remote::RemoteShell;
    use std::ffi::{OsStr, OsString};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Answers each `ssh` call with the next canned output and records argv.
    struct Scripted {
        outs: Mutex<Vec<ProcessOutput>>,
        argv: Mutex<Vec<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for Scripted {
        async fn run(
            &self,
            _program: &OsStr,
            args: &[OsString],
            _stdin: Option<Vec<u8>>,
            _timeout: Duration,
        ) -> ProcessOutput {
            self.argv.lock().expect("argv").push(
                args.iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
            );
            self.outs.lock().expect("outs").remove(0)
        }
    }

    fn ok(stdout: &str) -> ProcessOutput {
        ProcessOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            code: 0,
            timed_out: false,
        }
    }

    fn answer(projects: &str, project: &str, git: &str) -> ProcessOutput {
        ok(&format!(
            "IGN-RESULT:{{\"projects\":\"{projects}\",\"project\":\"{project}\",\"git\":\"{git}\"}}\n"
        ))
    }

    fn host(outs: Vec<ProcessOutput>, windows: bool) -> (Arc<Scripted>, SftpTransport) {
        let runner = Arc::new(Scripted {
            outs: Mutex::new(outs),
            argv: Mutex::new(Vec::new()),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            runner.clone(),
            Duration::from_secs(3),
        );
        let transport = if windows {
            SftpTransport::new_windows(shell, "/C:/gw/data".to_string(), "nodes.master")
        } else {
            SftpTransport::new(shell, "/gw/data".to_string(), "nodes.master")
        };
        (runner, transport)
    }

    fn detail(err: CoreError) -> String {
        match err {
            CoreError::TransportFailed { detail, .. } => detail,
            other => panic!("expected transport_failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sftp_git_entry_posix() {
        let cases = [
            ("dir", GitEntry::Directory),
            ("file", GitEntry::File),
            ("symlink", GitEntry::Symlink),
            ("other", GitEntry::Other),
            ("none", GitEntry::Absent),
        ];
        for (word, want) in cases {
            let (runner, t) = host(vec![answer("dir", "dir", word)], false);
            assert_eq!(t.git_entry("CapApp").await.expect(word), want, "{word}");
            // one exec call: a fixed script, the paths as separate arguments
            let argv = runner.argv.lock().expect("argv");
            assert_eq!(argv.len(), 1);
            let command = argv[0].last().expect("remote command");
            assert!(command.starts_with("'sh' '-c' '"), "{command}");
            for path in [
                "'/gw/data/projects'",
                "'/gw/data/projects/CapApp'",
                "'/gw/data/projects/CapApp/.git'",
            ] {
                assert!(command.contains(path), "{path} in {command}");
            }
            assert!(!command.contains("sftp"), "{command}");
        }
        // a missing project directory is absent (the create case)
        let (_, t) = host(vec![answer("dir", "none", "skip")], false);
        assert_eq!(
            t.git_entry("CapApp").await.expect("absent"),
            GitEntry::Absent
        );
    }

    #[tokio::test]
    async fn sftp_git_entry_posix_failures_are_transport_failed_never_absent() {
        // permission denied: the script prints stat's message and exits 3
        let (_, t) = host(
            vec![ProcessOutput {
                stdout: Vec::new(),
                stderr: b"stat: cannot statx '/gw/data/projects/CapApp/.git': Permission denied\n"
                    .to_vec(),
                code: 3,
                timed_out: false,
            }],
            false,
        );
        let why = detail(t.git_entry("CapApp").await.expect_err("denied"));
        assert!(why.contains("Permission denied"), "{why}");
        assert!(why.contains("git_entry"), "{why}");

        // ssh itself failed (255), a timeout, no result line, a doubled line
        let failures = [
            ProcessOutput {
                stdout: Vec::new(),
                stderr: b"ssh: connect to host x port 22: Connection refused\n".to_vec(),
                code: 255,
                timed_out: false,
            },
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: -1,
                timed_out: true,
            },
            ok("no result here\n"),
            ok("IGN-RESULT:not json\n"),
        ];
        for out in failures {
            let (_, t) = host(vec![out], false);
            let err = t.git_entry("CapApp").await.expect_err("failure");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }

        // the projects directory is missing, a symlink or a file
        for (word, needle) in [
            ("none", "does not exist"),
            ("symlink", "symlink"),
            ("file", "not a directory"),
        ] {
            let (_, t) = host(vec![answer(word, "skip", "skip")], false);
            let why = detail(t.git_entry("CapApp").await.expect_err(word));
            assert!(why.contains(needle), "{word}: {why}");
        }
        // the project entry is a symlink or a file
        for (word, needle) in [("symlink", "symlink"), ("file", "not a directory")] {
            let (_, t) = host(vec![answer("dir", word, "skip")], false);
            let why = detail(t.git_entry("CapApp").await.expect_err(word));
            assert!(why.contains(needle), "{word}: {why}");
        }
        // a word the probe does not define is never a kind
        for out in [
            answer("dir", "dir", "weird"),
            answer("dir", "dir", "skip"),
            answer("dir", "maybe", "skip"),
            answer("", "dir", "dir"),
        ] {
            let (_, t) = host(vec![out], false);
            let err = t.git_entry("CapApp").await.expect_err("unknown word");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }
    }

    #[tokio::test]
    async fn sftp_git_entry_refuses_unsafe_names_before_any_call() {
        for name in ["", ".", "..", "../x", "a/b", "a\nb", "a\u{7}b"] {
            let (runner, t) = host(Vec::new(), false);
            let err = t.git_entry(name).await.expect_err(name);
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
            assert!(runner.argv.lock().expect("argv").is_empty(), "{name:?}");
        }
        // a name with a quote cannot break out of its single-quoted argument
        let (runner, t) = host(vec![answer("dir", "none", "skip")], false);
        let name = "it's$(x)";
        t.git_entry(name).await.expect("absent");
        let argv = runner.argv.lock().expect("argv");
        let command = argv[0].last().expect("command");
        assert!(
            command.contains("'/gw/data/projects/it'\\''s$(x)'"),
            "{command}"
        );
    }

    #[tokio::test]
    async fn sftp_git_entry_windows_name_rules_run_before_the_probe() {
        for name in ["CON", "a:b", "trail.", "a<b"] {
            let (runner, t) = host(Vec::new(), true);
            let err = t.git_entry(name).await.expect_err(name);
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
            assert!(runner.argv.lock().expect("argv").is_empty(), "{name:?}");
        }
    }

    fn win_answer(projects: &str, project: &str, kind: &str, hidden: i64, reparse: i64) -> String {
        format!(
            "IGN-RESULT:{{\"ok\":1,\"projects\":\"{projects}\",\"project\":\"{project}\",\"kind\":\"{kind}\",\"hidden\":{hidden},\"reparse\":{reparse}}}\n"
        )
    }

    #[tokio::test]
    async fn win_git_entry_parse() {
        // (kind, hidden, reparse) of the `.git` entry
        let cases = [
            ("dir", 1, 0, GitEntry::Directory),
            ("dir", 0, 0, GitEntry::Directory),
            ("dir", 1, 1, GitEntry::Symlink),
            ("dir", 0, 1, GitEntry::Symlink),
            ("file", 0, 0, GitEntry::File),
            ("file", 1, 0, GitEntry::File),
            ("file", 0, 1, GitEntry::Symlink),
            ("none", 0, 0, GitEntry::Absent),
        ];
        for (kind, hidden, reparse, want) in cases {
            let (runner, t) = host(
                vec![ok(&win_answer("dir", "dir", kind, hidden, reparse))],
                true,
            );
            assert_eq!(
                t.git_entry("CapApp").await.expect(kind),
                want,
                "{kind} hidden={hidden} reparse={reparse}"
            );
            assert_eq!(runner.argv.lock().expect("argv").len(), 1, "one call");
        }
        // a missing project directory is absent (the create case)
        let (_, t) = host(vec![ok(&win_answer("dir", "none", "skip", 0, 0))], true);
        assert_eq!(
            t.git_entry("CapApp").await.expect("absent"),
            GitEntry::Absent
        );
    }

    #[tokio::test]
    async fn win_git_entry_failures_are_transport_failed_never_absent() {
        let failures = [
            // the script died (UnauthorizedAccessException under Stop): exit 1
            ProcessOutput {
                stdout: Vec::new(),
                stderr: b"Get-Item : Access to the path 'C:\\x\\.git' is denied.\n".to_vec(),
                code: 1,
                timed_out: false,
            },
            ProcessOutput {
                stdout: Vec::new(),
                stderr: b"ssh: connect to host x port 22: Connection refused\n".to_vec(),
                code: 255,
                timed_out: false,
            },
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: -1,
                timed_out: true,
            },
            ok("no result here\n"),
            ok("IGN-RESULT:not json\n"),
            ok("IGN-RESULT:{\"ok\":0}\n"),
            ok("IGN-RESULT:{}\n"),
            // an exit 0 with an error record on stderr is a failure too
            ProcessOutput {
                stdout: win_answer("dir", "dir", "none", 0, 0).into_bytes(),
                stderr: b"#< CLIXML\n<Objs><S S=\"Error\">denied</S></Objs>".to_vec(),
                code: 0,
                timed_out: false,
            },
            // fields the script does not define
            ok(&win_answer("dir", "dir", "weird", 0, 0)),
            ok(&win_answer("dir", "dir", "skip", 0, 0)),
            ok(&win_answer("dir", "maybe", "skip", 0, 0)),
            ok(&win_answer("", "dir", "dir", 0, 0)),
            ok(&win_answer("dir", "dir", "dir", 2, 0)),
            ok(&win_answer("dir", "dir", "dir", 0, 7)),
            ok("IGN-RESULT:{\"ok\":1,\"projects\":\"dir\",\"project\":\"dir\",\"kind\":\"dir\"}\n"),
        ];
        for out in failures {
            let (_, t) = host(vec![out], true);
            let err = t.git_entry("CapApp").await.expect_err("failure");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }
        // levels above `.git` that are not plain directories
        for (projects, project, needle) in [
            ("none", "skip", "does not exist"),
            ("symlink", "skip", "junction or symlink"),
            ("file", "skip", "not a directory"),
            ("dir", "symlink", "junction or symlink"),
            ("dir", "file", "not a directory"),
        ] {
            let (_, t) = host(vec![ok(&win_answer(projects, project, "skip", 0, 0))], true);
            let why = detail(t.git_entry("CapApp").await.expect_err(needle));
            assert!(why.contains(needle), "{projects}/{project}: {why}");
        }
    }

    #[tokio::test]
    async fn win_git_entry_script_is_fixed_text() {
        use crate::deploy::transport::winps::{COMMAND_BUDGET, GIT_ENTRY, encoded_command};

        // the body is a constant: no host or repo value is in it, and it never
        // uses the probes C-H ruled out
        assert!(!GIT_ENTRY.chars().any(|c| c == '\n'), "one line");
        assert!(GIT_ENTRY.contains("Get-Item -LiteralPath $x -Force"));
        for forbidden in [
            "Test-Path",
            "Exists(",
            "-Recurse",
            "Remove-",
            "Set-",
            "New-",
        ] {
            assert!(!GIT_ENTRY.contains(forbidden), "{forbidden} in GIT_ENTRY");
        }
        // an error other than ObjectNotFound is rethrown, never read as none
        assert!(GIT_ENTRY.contains("'ObjectNotFound'){return"));
        assert!(GIT_ENTRY.contains(";throw}"));

        // a hostile project name never reaches the script text: it is refused
        // by the component gate before anything is built, and a legal but odd
        // name travels in the base64 parameters
        let odd = "it's$(x)`;y";
        let script = WinScript::git_entry(
            "C:\\gw\\data\\projects",
            &format!("C:\\gw\\data\\projects\\{odd}"),
            &format!("C:\\gw\\data\\projects\\{odd}\\.git"),
        );
        assert_eq!(script.body, GIT_ENTRY);
        let command = encoded_command(&script, COMMAND_BUDGET).expect("fits the budget");
        assert!(command.as_str().chars().count() <= COMMAND_BUDGET);
        assert!(!command.as_str().contains("it's"));
        // a long realistic path set still fits the budget
        let long = format!(
            "C:\\Program Files\\Inductive Automation\\Ignition\\data\\projects\\{}",
            "P".repeat(100)
        );
        let script = WinScript::git_entry(
            "C:\\Program Files\\Inductive Automation\\Ignition\\data\\projects",
            &long,
            &format!("{long}\\.git"),
        );
        assert!(encoded_command(&script, COMMAND_BUDGET).is_ok());

        // through the transport: the paths are native, the sftp spelling is gone
        let (runner, t) = host(vec![ok(&win_answer("dir", "none", "skip", 0, 0))], true);
        t.git_entry("CapApp").await.expect("absent");
        let argv = runner.argv.lock().expect("argv");
        let text = argv[0].join(" ");
        assert!(text.contains("-EncodedCommand"), "{text}");
        assert!(
            !text.contains("CapApp"),
            "name is inside the encoding: {text}"
        );
    }

    #[test]
    fn git_probe_script_is_one_fixed_line() {
        assert!(!GIT_PROBE_SH.chars().any(char::is_control), "one line");
        assert!(GIT_PROBE_SH.contains("stat -c %F --"));
        assert!(
            !GIT_PROBE_SH.contains("stat -L"),
            "a symlink is reported, never followed"
        );
        assert!(
            !GIT_PROBE_SH.contains("test -e"),
            "C-H: test -e says absent"
        );
        assert!(GIT_PROBE_SH.contains("LC_ALL=C"));
    }

    /// Runs the real probe text under the local `sh` with a `stat` shim that
    /// answers like GNU `stat -c %F -- PATH` (the messages are the ones
    /// `## C-H before grant` captured), so the script's own branching is
    /// exercised on any unix host. A path with `/denied/` in it answers
    /// `Permission denied`; the `.git` of a project named `special*` is a
    /// socket.
    #[cfg(unix)]
    mod script {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        const SHIM: &str = "#!/bin/sh\n\
            p=$4\n\
            case \"$p\" in */denied/*) echo \"stat: cannot statx '$p': Permission denied\" >&2; exit 1;; esac\n\
            if [ -L \"$p\" ]; then echo 'symbolic link'\n\
            elif [ -d \"$p\" ]; then echo directory\n\
            elif [ -f \"$p\" ] && [ ! -s \"$p\" ]; then echo 'regular empty file'\n\
            elif [ -f \"$p\" ]; then echo 'regular file'\n\
            else case \"$p\" in */special*/.git) echo socket;; *) echo \"stat: cannot statx '$p': No such file or directory\" >&2; exit 1;; esac\n\
            fi\n";

        struct Run {
            code: i32,
            stdout: String,
            stderr: String,
        }

        /// The probe for `project` below `root/projects`.
        fn run(root: &Path, project: &str) -> Run {
            let bin = root.join("bin");
            std::fs::create_dir_all(&bin).expect("bin");
            let shim = bin.join("stat");
            std::fs::write(&shim, SHIM).expect("shim");
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            let projects = root.join("projects");
            let dir = projects.join(project);
            let path = format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let out = Command::new("sh")
                .args(["-c", GIT_PROBE_SH, GIT_PROBE_NAME])
                .arg(&projects)
                .arg(&dir)
                .arg(dir.join(".git"))
                .env("PATH", path)
                .output()
                .expect("sh");
            Run {
                code: out.status.code().expect("exit code"),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            }
        }

        fn words(run: &Run) -> serde_json::Value {
            assert_eq!(run.code, 0, "{}", run.stderr);
            let line = run.stdout.trim_end();
            serde_json::from_str(line.strip_prefix("IGN-RESULT:").expect("sentinel")).expect("json")
        }

        #[test]
        fn probe_script_classifies_every_shape() {
            let tmp = tempfile::tempdir().expect("tmp");
            let root = tmp.path();
            std::fs::create_dir_all(root.join("projects")).expect("projects");

            // no project directory: level 2 is none, `.git` is never asked
            assert_eq!(
                words(&run(root, "CapApp")),
                serde_json::json!({"projects":"dir","project":"none","git":"skip"})
            );

            // a project directory without `.git`
            std::fs::create_dir_all(root.join("projects/CapApp")).expect("app");
            assert_eq!(
                words(&run(root, "CapApp")),
                serde_json::json!({"projects":"dir","project":"dir","git":"none"})
            );

            let git = root.join("projects/CapApp/.git");
            std::fs::create_dir(&git).expect("git dir");
            assert_eq!(words(&run(root, "CapApp"))["git"], "dir");
            std::fs::remove_dir(&git).expect("rm");
            std::fs::write(&git, "gitdir: x").expect("git file");
            assert_eq!(words(&run(root, "CapApp"))["git"], "file");
            std::fs::write(&git, "").expect("empty");
            assert_eq!(words(&run(root, "CapApp"))["git"], "file");
            std::fs::remove_file(&git).expect("rm");
            // a dangling symlink is reported, not followed
            std::os::unix::fs::symlink("nowhere", &git).expect("link");
            assert_eq!(words(&run(root, "CapApp"))["git"], "symlink");

            // the project entry is a file or a symlink
            std::fs::write(root.join("projects/AFile"), "x").expect("file");
            assert_eq!(words(&run(root, "AFile"))["project"], "file");
            std::os::unix::fs::symlink("CapApp", root.join("projects/ALink")).expect("link");
            let w = words(&run(root, "ALink"));
            assert_eq!(w["project"], "symlink");
            assert_eq!(w["git"], "skip");

            // another kind of entry
            std::fs::create_dir_all(root.join("projects/special1")).expect("dir");
            assert_eq!(words(&run(root, "special1"))["git"], "other");
        }

        #[test]
        fn probe_script_denied_is_a_failure_never_none() {
            let tmp = tempfile::tempdir().expect("tmp");
            let root = tmp.path();
            std::fs::create_dir_all(root.join("projects/denied")).expect("dir");
            let r = run(root, "denied");
            assert_eq!(r.code, 3, "{}", r.stderr);
            assert!(r.stderr.contains("Permission denied"), "{}", r.stderr);
            assert!(!r.stdout.contains("IGN-RESULT"), "{}", r.stdout);
        }

        #[test]
        fn probe_script_missing_projects_directory_is_none_at_level_one() {
            let tmp = tempfile::tempdir().expect("tmp");
            assert_eq!(
                words(&run(tmp.path(), "CapApp")),
                serde_json::json!({"projects":"none","project":"skip","git":"skip"})
            );
        }
    }
}
