//! The host-git runner for the git-module handover (Phase 27).
//!
//! Every git operation the handover needs goes through [`HostGit`]: one
//! fail-closed subprocess runner with argv only (never a shell), no URL or
//! credential on any argument list, a stdout cap and a timeout. A missing
//! program, an unexpected exit status or unparseable output is
//! `deploy_preflight_failed` with check `git_host`; it is never read as
//! "absent" or "no objects".
//!
//! **Gateway data never becomes a git dir.** The bytes a gateway host returns
//! (`config`, `HEAD`, refs) are untrusted: this module parses the config with
//! `git config --file <tmp> --no-includes` under an empty global config
//! (`GIT_CONFIG_GLOBAL`, which is why git 2.32 is the floor) and no
//! system config, and reads refs from a scratch git dir it builds itself after
//! `check-ref-format`, so a hook, `core.fsmonitor`, an include or a filter in a
//! gateway repo never executes (T-27-01).
//!
//! Operations on ign's own release checkout (`tree_paths`, `blobs`,
//! `has_commit`, `build_pack`, `build_index`, `status_over`) keep the
//! operator's own git configuration, as every other git call of the operator
//! does; they never name a gateway path.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use base64::Engine as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::git_state::{GIT_SMALL_FILE_MAX, branch_name_problem, redact_text};
use super::git_state::{GitBranchConfig, GitConfig, GitFiles, GitHead, GitRefs, GitRemoteConfig};
use crate::error::{CoreError, PreflightFailure};

/// The oldest git ign drives. The floor is set by `GIT_CONFIG_GLOBAL` (git
/// 2.32), which the isolated calls rely on for an empty global config, and by
/// `GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>` (git
/// 2.31), which carry the token's extraheader. Below the floor git ignores
/// both, silently: the gateway-repo isolation would not hold and the token
/// would not be sent (WR-A3).
pub const GIT_MIN_VERSION: (u32, u32) = (2, 32);

/// The preflight check id every host-git failure carries.
pub const GIT_HOST_CHECK: &str = "git_host";

/// Largest stdout of a small query (config, refs, ls-tree).
const SMALL_OUTPUT_MAX: usize = 64 * 1024 * 1024;
/// Largest pack or blob stream.
const LARGE_OUTPUT_MAX: usize = 1024 * 1024 * 1024;
/// How much stderr is kept for a message.
const STDERR_KEEP: usize = 4096;
/// Longest a single git call may run.
const GIT_TIMEOUT: Duration = Duration::from_secs(600);
/// Longest `git --version` may take.
const VERSION_TIMEOUT: Duration = Duration::from_secs(15);

/// The `deploy_preflight_failed` of a host-git problem. `detail` passes
/// through [`redact_text`].
pub(crate) fn host_failure(detail: impl AsRef<str>) -> CoreError {
    let mut detail = redact_text(detail.as_ref());
    if detail.len() > 600 {
        let mut end = 600;
        while !detail.is_char_boundary(end) {
            end -= 1;
        }
        detail.truncate(end);
        detail.push_str("...");
    }
    CoreError::DeployPreflightFailed {
        failures: vec![PreflightFailure {
            check: GIT_HOST_CHECK.to_string(),
            detail,
        }]
        .into(),
    }
}

/// Whether `id` is a full lowercase hex object id (SHA-1 or SHA-256).
pub(crate) fn is_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64)
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A conservative check that `name` is a safe full ref name
/// (`refs/heads/x`). It accepts a strict subset of what git accepts, because
/// the name becomes a path in a scratch directory: no `..`, no component that
/// starts with `.` or ends with `.lock`, no empty component, nothing outside
/// `[A-Za-z0-9._+/-]`.
pub(crate) fn safe_ref_name(name: &str) -> bool {
    if !name.starts_with("refs/") || name.len() > 255 || name.ends_with('.') || name.contains("..")
    {
        return false;
    }
    name.split('/').all(|part| {
        !part.is_empty()
            && !part.starts_with('.')
            && !part.ends_with(".lock")
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
    })
}

/// The files of a built pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFiles {
    /// The pack's name: the hex digest in `pack-<name>.pack`.
    pub name: String,
    /// The `.pack` bytes.
    pub pack: Vec<u8>,
    /// The `.idx` bytes.
    pub idx: Vec<u8>,
}

/// What `git status` over a work tree says, as path lists (byte-sorted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusLines {
    /// Tracked paths whose content differs from the index.
    pub modified: Vec<String>,
    /// Tracked paths that are absent and not assume-unchanged.
    pub deleted: Vec<String>,
    /// Paths on disk that the index does not hold.
    pub untracked: Vec<String>,
}

/// One subprocess to run.
struct Call {
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    /// Run under an empty global config (every call that reads gateway data).
    isolate: bool,
    stdout_max: usize,
    timeout: Duration,
}

impl Call {
    fn new<I, S>(args: I) -> Call
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Call {
            args: args
                .into_iter()
                .map(|a| a.as_ref().to_os_string())
                .collect(),
            cwd: None,
            envs: Vec::new(),
            stdin: None,
            isolate: false,
            stdout_max: SMALL_OUTPUT_MAX,
            timeout: GIT_TIMEOUT,
        }
    }

    fn cwd(mut self, dir: &Path) -> Call {
        self.cwd = Some(dir.to_path_buf());
        self
    }

    fn env(mut self, key: &str, value: impl AsRef<OsStr>) -> Call {
        self.envs
            .push((OsString::from(key), value.as_ref().to_os_string()));
        self
    }

    fn stdin(mut self, bytes: Vec<u8>) -> Call {
        self.stdin = Some(bytes);
        self
    }

    fn isolated(mut self) -> Call {
        self.isolate = true;
        self
    }

    fn max_output(mut self, max: usize) -> Call {
        self.stdout_max = max;
        self
    }
}

/// What a finished subprocess said.
struct Done {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

impl Done {
    fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// The environment variable that carries the token for ign's own reads of
/// the gateway's git remote (D-23, 27-20). The composite actions set it from
/// their `git-token` input as a step environment variable and nowhere else.
pub const GIT_TOKEN_ENV: &str = "IGNITION_GIT_TOKEN";

/// Environment variables no git subprocess inherits. The last is the remote
/// token: the two remote reads are given it explicitly, scoped to one host
/// ([`remote_auth_env`]); no other git process, hook or ssh sees it.
const SCRUBBED_ENV: [&str; 10] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_COMMON_DIR",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    GIT_TOKEN_ENV,
];

/// The host-git subprocess runner.
///
/// It may hold a read token for the gateway's remote ([`GIT_TOKEN_ENV`]). The
/// token is private, never in `Debug` or an error, and reaches only the
/// environment of ign's own remote reads ([`HostGit::ls_remote_heads`],
/// [`HostGit::fetch_heads`]), scoped to the matched remote's host by
/// [`remote_auth_env`]. No other git process sees it: [`HostGit::run`] removes
/// the variable from every child.
#[derive(Clone)]
pub struct HostGit {
    program: OsString,
    remote_token: Option<String>,
}

impl std::fmt::Debug for HostGit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token is never printed, not even its length.
        struct Presence(bool);
        impl std::fmt::Debug for Presence {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(if self.0 { "<set>" } else { "<unset>" })
            }
        }
        f.debug_struct("HostGit")
            .field("program", &self.program)
            .field("remote_token", &Presence(self.remote_token.is_some()))
            .finish()
    }
}

impl Default for HostGit {
    fn default() -> Self {
        HostGit::new()
    }
}

impl HostGit {
    /// The runner for `git` on `PATH`, with the remote token of
    /// [`GIT_TOKEN_ENV`] when it is set and not empty. The variable is read
    /// once, here.
    pub fn new() -> HostGit {
        HostGit {
            program: OsString::from("git"),
            remote_token: std::env::var(GIT_TOKEN_ENV)
                .ok()
                .filter(|token| !token.is_empty()),
        }
    }

    /// The runner for a named program (the test seam, as
    /// `ignore_status_with`). It holds no token: the environment is never
    /// read.
    pub fn with_program(program: impl AsRef<OsStr>) -> HostGit {
        HostGit {
            program: program.as_ref().to_os_string(),
            remote_token: None,
        }
    }

    /// The runner for `git` on `PATH` with `token` as the remote token and
    /// the environment ignored (the test seam of [`GIT_TOKEN_ENV`]). An empty
    /// token is no token.
    pub fn with_remote_token(token: Option<String>) -> HostGit {
        HostGit {
            program: OsString::from("git"),
            remote_token: None,
        }
        .and_remote_token(token)
    }

    /// This runner with `token` as the remote token (the test seam, for a
    /// stub program).
    pub fn and_remote_token(mut self, token: Option<String>) -> HostGit {
        self.remote_token = token.filter(|t| !t.is_empty());
        self
    }

    async fn run(&self, call: Call) -> Result<Done, CoreError> {
        let global_dir;
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(&call.args)
            .stdin(if call.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("LC_ALL", "C");
        for var in SCRUBBED_ENV {
            command.env_remove(var);
        }
        if call.isolate {
            global_dir = tempfile::tempdir()
                .map_err(|e| host_failure(format!("a private temp dir could not be made: {e}")))?;
            let global = global_dir.path().join("empty-gitconfig");
            std::fs::write(&global, b"")
                .map_err(|e| host_failure(format!("a private temp file could not be made: {e}")))?;
            command.env("GIT_CONFIG_GLOBAL", &global);
        }
        if let Some(cwd) = &call.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &call.envs {
            command.env(key, value);
        }
        let mut child = command.spawn().map_err(|e| {
            host_failure(format!(
                "git could not be started ({}): {}",
                self.program.to_string_lossy(),
                e.kind()
            ))
        })?;
        let writer = match (child.stdin.take(), call.stdin) {
            (Some(mut stdin), Some(bytes)) => Some(tokio::spawn(async move {
                // A child that exits early closes the pipe; its exit status
                // is what the caller checks.
                let _ = stdin.write_all(&bytes).await;
                let _ = stdin.shutdown().await;
            })),
            _ => None,
        };
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let max = call.stdout_max;
        let work = async {
            let (out, err) = tokio::join!(
                read_capped(&mut stdout, max),
                read_stderr(&mut stderr, STDERR_KEEP)
            );
            let out = out?;
            let status = child
                .wait()
                .await
                .map_err(|e| host_failure(format!("git did not finish: {}", e.kind())))?;
            Ok::<_, CoreError>(Done {
                code: status.code(),
                stdout: out,
                stderr: err,
            })
        };
        let result = tokio::time::timeout(call.timeout, work).await;
        if let Some(writer) = writer {
            writer.abort();
        }
        match result {
            Ok(done) => done.map(|mut done| {
                // Whatever git printed, the token is not in it.
                if let Some(token) = &self.remote_token {
                    done.stderr = scrub_token(&done.stderr, token);
                }
                done
            }),
            Err(_) => Err(host_failure(format!(
                "git did not finish within {} s",
                call.timeout.as_secs()
            ))),
        }
    }

    /// Run `call` and require exit 0.
    async fn run_ok(&self, what: &str, call: Call) -> Result<Done, CoreError> {
        let done = self.run(call).await?;
        if done.ok() {
            Ok(done)
        } else {
            Err(host_failure(format!(
                "git {what} failed ({}): {}",
                done.code
                    .map_or_else(|| "no exit status".to_string(), |c| format!("exit {c}")),
                done.stderr.trim()
            )))
        }
    }

    /// The installed git's `(major, minor)`, refused below
    /// [`GIT_MIN_VERSION`].
    pub async fn version(&self) -> Result<(u32, u32), CoreError> {
        let mut call = Call::new(["--version"]);
        call.timeout = VERSION_TIMEOUT;
        let done = self.run_ok("--version", call).await?;
        let text = String::from_utf8_lossy(&done.stdout);
        let version = text
            .trim()
            .strip_prefix("git version ")
            .and_then(parse_version)
            .ok_or_else(|| host_failure("git --version answered something unexpected"))?;
        if version < GIT_MIN_VERSION {
            return Err(host_failure(format!(
                "git {}.{} is older than the required {}.{}",
                version.0, version.1, GIT_MIN_VERSION.0, GIT_MIN_VERSION.1
            )));
        }
        Ok(version)
    }

    /// Parse a gateway `config` with `git config --file <tmp> --no-includes
    /// --list -z`. Git never runs with the gateway repo as its git dir.
    pub async fn parse_config(&self, bytes: &[u8]) -> Result<GitConfig, CoreError> {
        if bytes.len() > GIT_SMALL_FILE_MAX {
            return Err(host_failure("the gateway config is over its size limit"));
        }
        let dir = scratch_dir()?;
        let file = dir.path().join("config");
        std::fs::write(&file, bytes)
            .map_err(|e| host_failure(format!("a private temp file could not be written: {e}")))?;
        let call = Call::new([
            OsStr::new("config"),
            OsStr::new("--file"),
            file.as_os_str(),
            OsStr::new("--no-includes"),
            OsStr::new("--list"),
            OsStr::new("-z"),
        ])
        .cwd(dir.path())
        .isolated();
        let done = self.run_ok("config", call).await?;
        let mut config = GitConfig::default();
        for entry in done.stdout.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            let (key, value) = match entry.iter().position(|b| *b == b'\n') {
                Some(at) => (&entry[..at], &entry[at + 1..]),
                None => (entry, &entry[entry.len()..]),
            };
            let key = String::from_utf8_lossy(key).into_owned();
            let value = String::from_utf8_lossy(value).into_owned();
            record_config_entry(&mut config, &key, value);
        }
        Ok(config)
    }

    /// Check that `name` is a branch name git itself accepts, after ign's own
    /// pure check ([`branch_name_problem`]) so nothing that starts with `-` or
    /// holds a revision-syntax character reaches an argument list. The name
    /// comes from a gateway file or a target file: the refusal never echoes it
    /// (27-17, T-27-38).
    pub async fn check_branch_name(&self, name: &str) -> Result<(), CoreError> {
        if branch_name_problem(name).is_some() {
            return Err(host_failure(
                "the adoption branch name is not a valid branch name",
            ));
        }
        let full = format!("refs/heads/{name}");
        let done = self
            .run(Call::new(["check-ref-format", full.as_str()]).isolated())
            .await?;
        match done.code {
            Some(0) => Ok(()),
            Some(1) => Err(host_failure(
                "the adoption branch name is not a valid branch name",
            )),
            _ => Err(host_failure(format!(
                "git check-ref-format failed: {}",
                done.stderr.trim()
            ))),
        }
    }

    /// The gateway `config` with a `[branch "<branch>"]` section that tracks
    /// `remote`'s `<branch>` (27-17, D-13): `branch.<b>.remote = <remote>` and
    /// `branch.<b>.merge = refs/heads/<b>`, each added only when absent.
    ///
    /// The bytes are copied to a private temp file and edited there with
    /// `git config --file <tmp> --no-includes` under an empty global config:
    /// git never runs with the gateway repo as its git dir and follows no
    /// include (T-27-40). A value that is already set and differs refuses:
    /// ign does not rewrite an upstream somebody chose.
    pub async fn adopt_branch_config(
        &self,
        config: &[u8],
        branch: &str,
        remote: &str,
    ) -> Result<Vec<u8>, CoreError> {
        if config.len() > GIT_SMALL_FILE_MAX {
            return Err(host_failure("the gateway config is over its size limit"));
        }
        self.check_branch_name(branch).await?;
        require_safe_name(remote, "remote")?;
        let dir = scratch_dir()?;
        let file = dir.path().join("config");
        write_scratch(&file, config)?;
        let merge = format!("refs/heads/{branch}");
        for (var, want) in [("remote", remote), ("merge", merge.as_str())] {
            let key = format!("branch.{branch}.{var}");
            let have = self
                .run(
                    Call::new([
                        OsStr::new("config"),
                        OsStr::new("--file"),
                        file.as_os_str(),
                        OsStr::new("--no-includes"),
                        OsStr::new("--get-all"),
                        OsStr::new(&key),
                    ])
                    .cwd(dir.path())
                    .isolated(),
                )
                .await?;
            match have.code {
                Some(0) => {
                    let text = String::from_utf8_lossy(&have.stdout);
                    let mut values = text.lines();
                    if values.next() != Some(want) || values.next().is_some() {
                        return Err(host_failure(format!(
                            "the gateway config already sets {} to another value; it is not \
                             rewritten",
                            redact_text(&key)
                        )));
                    }
                }
                Some(1) => {
                    self.run_ok(
                        "config",
                        Call::new([
                            OsStr::new("config"),
                            OsStr::new("--file"),
                            file.as_os_str(),
                            OsStr::new("--no-includes"),
                            OsStr::new(&key),
                            OsStr::new(want),
                        ])
                        .cwd(dir.path())
                        .isolated(),
                    )
                    .await?;
                }
                _ => {
                    return Err(host_failure(format!(
                        "git config failed: {}",
                        have.stderr.trim()
                    )));
                }
            }
        }
        let bytes = std::fs::read(&file)
            .map_err(|e| host_failure(format!("a private temp file could not be read: {e}")))?;
        if bytes.len() > GIT_SMALL_FILE_MAX {
            return Err(host_failure("the adopted config is over its size limit"));
        }
        Ok(bytes)
    }

    /// Read a gateway repo's refs through a scratch git dir. `Ok(None)` when
    /// `HEAD` is absent or is not a branch ref or a commit id. A loose ref
    /// with an unsafe name or unparseable content is an error and nothing is
    /// parsed.
    pub async fn parse_refs(&self, files: &GitFiles) -> Result<Option<GitRefs>, CoreError> {
        let Some(head_bytes) = &files.head else {
            return Ok(None);
        };
        let head_text = String::from_utf8_lossy(head_bytes);
        let head_text = head_text.trim();
        let head_ref = match head_text.strip_prefix("ref:") {
            Some(target) => {
                let target = target.trim();
                if !target.starts_with("refs/heads/") || !safe_ref_name(target) {
                    return Ok(None);
                }
                Some(target.to_string())
            }
            None => {
                if !is_object_id(head_text) {
                    return Ok(None);
                }
                None
            }
        };
        let dir = scratch_dir()?;
        let git_dir = dir.path().join("scratch.git");
        for sub in ["objects", "refs/heads", "refs/remotes", "refs/tags"] {
            std::fs::create_dir_all(git_dir.join(sub))
                .map_err(|e| host_failure(format!("a scratch git dir could not be made: {e}")))?;
        }
        write_scratch(
            &git_dir.join("config"),
            b"[core]\n\trepositoryformatversion = 0\n",
        )?;
        let head_line = match &head_ref {
            Some(target) => format!("ref: {target}\n"),
            None => format!("{head_text}\n"),
        };
        write_scratch(&git_dir.join("HEAD"), head_line.as_bytes())?;
        if let Some(packed) = &files.packed_refs {
            write_scratch(&git_dir.join("packed-refs"), packed)?;
        }
        for (name, content) in &files.loose_refs {
            if !safe_ref_name(name) {
                return Err(host_failure(format!(
                    "the gateway repo holds a ref with an unsafe name (`{}`)",
                    name.escape_debug()
                )));
            }
            self.check_ref_format(name).await?;
            let text = String::from_utf8_lossy(content);
            let text = text.trim();
            let line = if is_object_id(text) {
                format!("{text}\n")
            } else if let Some(target) = text.strip_prefix("ref:").map(str::trim)
                && safe_ref_name(target)
            {
                format!("ref: {target}\n")
            } else {
                return Err(host_failure(format!(
                    "the gateway repo's ref `{name}` is neither a commit id nor a symbolic ref"
                )));
            };
            let path = git_dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    host_failure(format!("the ref `{name}` conflicts with another ref: {e}"))
                })?;
            }
            write_scratch(&path, line.as_bytes())?;
        }
        let git_dir_arg = OsString::from(format!("--git-dir={}", git_dir.display()));
        let listing = self
            .run_ok(
                "for-each-ref",
                Call::new([
                    git_dir_arg.clone(),
                    OsString::from("for-each-ref"),
                    OsString::from("--format=%(objectname) %(refname)"),
                ])
                .cwd(dir.path())
                .isolated(),
            )
            .await?;
        let mut heads = BTreeMap::new();
        let mut remotes = BTreeMap::new();
        for line in String::from_utf8_lossy(&listing.stdout).lines() {
            let Some((id, name)) = line.split_once(' ') else {
                return Err(host_failure(
                    "git for-each-ref answered something unexpected",
                ));
            };
            if !is_object_id(id) {
                return Err(host_failure(
                    "git for-each-ref answered something unexpected",
                ));
            }
            if name.starts_with("refs/heads/") {
                heads.insert(name.to_string(), id.to_string());
            } else if name.starts_with("refs/remotes/") {
                remotes.insert(name.to_string(), id.to_string());
            }
        }
        let head = match head_ref {
            Some(target) => {
                let symbolic = self
                    .run(
                        Call::new([
                            git_dir_arg,
                            OsString::from("symbolic-ref"),
                            OsString::from("-q"),
                            OsString::from("HEAD"),
                        ])
                        .cwd(dir.path())
                        .isolated(),
                    )
                    .await?;
                if !symbolic.ok() || String::from_utf8_lossy(&symbolic.stdout).trim() != target {
                    return Ok(None);
                }
                let name = target
                    .strip_prefix("refs/heads/")
                    .expect("checked above")
                    .to_string();
                let commit = heads.get(&target).cloned();
                GitHead::Branch { name, commit }
            }
            None => GitHead::Detached {
                commit: head_text.to_string(),
            },
        };
        Ok(Some(GitRefs {
            head,
            heads,
            remotes,
        }))
    }

    async fn check_ref_format(&self, name: &str) -> Result<(), CoreError> {
        let done = self
            .run(Call::new(["check-ref-format", name]).isolated())
            .await?;
        match done.code {
            Some(0) => Ok(()),
            Some(1) => Err(host_failure(format!(
                "the gateway repo holds a ref with an invalid name (`{}`)",
                name.escape_debug()
            ))),
            _ => Err(host_failure(format!(
                "git check-ref-format failed: {}",
                done.stderr.trim()
            ))),
        }
    }

    /// Every path of `commit`'s tree (`ls-tree -r -z --name-only`),
    /// byte-sorted.
    pub async fn tree_paths(
        &self,
        checkout: &Path,
        commit: &str,
    ) -> Result<Vec<String>, CoreError> {
        require_object_id(commit)?;
        let done = self
            .run_ok(
                "ls-tree",
                Call::new([
                    OsStr::new("-C"),
                    checkout.as_os_str(),
                    OsStr::new("ls-tree"),
                    OsStr::new("-r"),
                    OsStr::new("-z"),
                    OsStr::new("--name-only"),
                    OsStr::new(commit),
                ]),
            )
            .await?;
        let mut paths: Vec<String> = done
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        paths.sort();
        Ok(paths)
    }

    /// The bytes of each of `paths` at `commit`, through one `cat-file
    /// --batch` process. A path the tree lacks is an error.
    pub async fn blobs(
        &self,
        checkout: &Path,
        commit: &str,
        paths: &[String],
    ) -> Result<BTreeMap<String, Vec<u8>>, CoreError> {
        require_object_id(commit)?;
        if paths.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut input = Vec::new();
        for path in paths {
            if path.contains('\n') || path.contains('\0') {
                return Err(host_failure(
                    "a release path holds a newline or NUL and cannot be read",
                ));
            }
            input.extend_from_slice(commit.as_bytes());
            input.push(b':');
            input.extend_from_slice(path.as_bytes());
            input.push(b'\n');
        }
        let done = self
            .run_ok(
                "cat-file --batch",
                Call::new([
                    OsStr::new("-C"),
                    checkout.as_os_str(),
                    OsStr::new("cat-file"),
                    OsStr::new("--batch"),
                ])
                .stdin(input)
                .max_output(LARGE_OUTPUT_MAX),
            )
            .await?;
        let out = done.stdout;
        let mut pos = 0usize;
        let mut blobs = BTreeMap::new();
        for path in paths {
            let bad = || host_failure("git cat-file --batch answered something unexpected");
            let eol = out[pos..]
                .iter()
                .position(|b| *b == b'\n')
                .ok_or_else(bad)?;
            let header = String::from_utf8_lossy(&out[pos..pos + eol]).into_owned();
            pos += eol + 1;
            let mut fields = header.split(' ');
            let (_, kind, size) = (fields.next(), fields.next(), fields.next());
            if kind != Some("blob") {
                return Err(host_failure(format!(
                    "the release has no file `{}` ({})",
                    path.escape_debug(),
                    header.rsplit(' ').next().unwrap_or("")
                )));
            }
            let size: usize = size.and_then(|s| s.parse().ok()).ok_or_else(bad)?;
            let end = pos.checked_add(size).ok_or_else(bad)?;
            if end >= out.len() || out[end] != b'\n' {
                return Err(bad());
            }
            blobs.insert(path.clone(), out[pos..end].to_vec());
            pos = end + 1;
        }
        Ok(blobs)
    }

    /// Whether `sha` names a commit in the checkout's object store. Anything
    /// but a clean yes or no fails closed.
    pub async fn has_commit(&self, checkout: &Path, sha: &str) -> Result<bool, CoreError> {
        require_object_id(sha)?;
        let done = self
            .run_ok(
                "cat-file --batch-check",
                Call::new([
                    OsStr::new("-C"),
                    checkout.as_os_str(),
                    OsStr::new("cat-file"),
                    OsStr::new("--batch-check"),
                ])
                .stdin(format!("{sha}\n").into_bytes()),
            )
            .await?;
        let text = String::from_utf8_lossy(&done.stdout);
        let line = text.trim();
        if line == format!("{sha} missing") {
            return Ok(false);
        }
        match line.split(' ').collect::<Vec<_>>().as_slice() {
            [id, kind, size] if *id == sha && size.bytes().all(|b| b.is_ascii_digit()) => {
                Ok(*kind == "commit")
            }
            _ => Err(host_failure(
                "git cat-file --batch-check answered something unexpected",
            )),
        }
    }

    /// The pack of every object reachable from `include` and not from any of
    /// `bases`, with its index. `None` when the pack holds no object. The
    /// caller guarantees every base is in the checkout's store.
    pub async fn build_pack(
        &self,
        checkout: &Path,
        include: &str,
        bases: &[String],
    ) -> Result<Option<PackFiles>, CoreError> {
        require_object_id(include)?;
        let mut input = format!("{include}\n");
        for base in bases {
            require_object_id(base)?;
            input.push_str(&format!("^{base}\n"));
        }
        let packed = self
            .run_ok(
                "pack-objects",
                Call::new([
                    OsStr::new("-C"),
                    checkout.as_os_str(),
                    OsStr::new("pack-objects"),
                    OsStr::new("--revs"),
                    OsStr::new("--stdout"),
                    OsStr::new("-q"),
                ])
                .stdin(input.into_bytes())
                .max_output(LARGE_OUTPUT_MAX),
            )
            .await?;
        let stream = packed.stdout;
        if stream.len() < 32 || &stream[..4] != b"PACK" {
            return Err(host_failure(
                "git pack-objects answered something unexpected",
            ));
        }
        let count = u32::from_be_bytes([stream[8], stream[9], stream[10], stream[11]]);
        if count == 0 {
            return Ok(None);
        }
        let dir = scratch_dir()?;
        let bare = dir.path().join("index.git");
        self.run_ok(
            "init",
            Call::new([
                OsStr::new("init"),
                OsStr::new("--bare"),
                OsStr::new("-q"),
                OsStr::new("--template="),
                bare.as_os_str(),
            ])
            .cwd(dir.path())
            .isolated(),
        )
        .await?;
        self.run_ok(
            "index-pack",
            Call::new([
                OsString::from(format!("--git-dir={}", bare.display())),
                OsString::from("index-pack"),
                OsString::from("--stdin"),
            ])
            .cwd(dir.path())
            .isolated()
            .stdin(stream),
        )
        .await?;
        let pack_dir = bare.join("objects").join("pack");
        let mut name = None;
        for entry in std::fs::read_dir(&pack_dir)
            .map_err(|e| host_failure(format!("the indexed pack could not be found: {e}")))?
        {
            let entry = entry
                .map_err(|e| host_failure(format!("the indexed pack could not be found: {e}")))?;
            let file = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = file
                .strip_prefix("pack-")
                .and_then(|f| f.strip_suffix(".pack"))
            {
                if name.is_some() || !is_object_id(stem) {
                    return Err(host_failure("git index-pack left an unexpected pack"));
                }
                name = Some(stem.to_string());
            }
        }
        let name = name.ok_or_else(|| host_failure("git index-pack wrote no pack"))?;
        let read = |ext: &str| {
            std::fs::read(pack_dir.join(format!("pack-{name}.{ext}")))
                .map_err(|e| host_failure(format!("the indexed pack could not be read: {e}")))
        };
        Ok(Some(PackFiles {
            pack: read("pack")?,
            idx: read("idx")?,
            name,
        }))
    }

    /// The index of `commit`'s tree (version 2, zero stat) with
    /// `assume_unchanged` marked, built in a temp `GIT_INDEX_FILE`.
    pub async fn build_index(
        &self,
        checkout: &Path,
        commit: &str,
        assume_unchanged: &[String],
    ) -> Result<Vec<u8>, CoreError> {
        require_object_id(commit)?;
        let dir = scratch_dir()?;
        let index = dir.path().join("index");
        let work = dir.path().join("empty-work-tree");
        std::fs::create_dir(&work)
            .map_err(|e| host_failure(format!("a private temp dir could not be made: {e}")))?;
        let neutral: [&OsStr; 12] = [
            OsStr::new("-c"),
            OsStr::new("index.version=2"),
            OsStr::new("-c"),
            OsStr::new("core.splitIndex=false"),
            OsStr::new("-c"),
            OsStr::new("core.untrackedCache=false"),
            OsStr::new("-c"),
            OsStr::new("core.fsmonitor=false"),
            OsStr::new("-c"),
            OsStr::new("feature.manyFiles=false"),
            OsStr::new("-C"),
            checkout.as_os_str(),
        ];
        let call = |tail: Vec<OsString>| {
            let mut args: Vec<OsString> = neutral.iter().map(|a| a.to_os_string()).collect();
            args.extend(tail);
            Call::new(args).env("GIT_INDEX_FILE", &index)
        };
        self.run_ok(
            "read-tree",
            call(vec![OsString::from("read-tree"), OsString::from(commit)]),
        )
        .await?;
        if !assume_unchanged.is_empty() {
            let mut input = Vec::new();
            for path in assume_unchanged {
                if path.contains('\0') {
                    return Err(host_failure("a release path holds a NUL"));
                }
                input.extend_from_slice(path.as_bytes());
                input.push(0);
            }
            self.run_ok(
                "update-index",
                call(vec![
                    OsString::from(format!("--work-tree={}", work.display())),
                    OsString::from("update-index"),
                    OsString::from("--assume-unchanged"),
                    OsString::from("-z"),
                    OsString::from("--stdin"),
                ])
                .stdin(input),
            )
            .await?;
        }
        std::fs::read(&index)
            .map_err(|e| host_failure(format!("the built index could not be read: {e}")))
    }

    /// What git says about `worktree` against `index`, run with the checkout's
    /// git dir and nothing else of the checkout: `diff` of the work tree
    /// against the index (a zero-stat entry is compared by content, and an
    /// assume-unchanged entry is skipped, as JGit skips it) and `ls-files
    /// --others` with only the `.gitignore` files inside the work tree
    /// applying (never the checkout's `info/exclude` or a global excludes
    /// file).
    pub async fn status_over(
        &self,
        checkout: &Path,
        index: &[u8],
        worktree: &Path,
    ) -> Result<StatusLines, CoreError> {
        let git_dir = self
            .run_ok(
                "rev-parse",
                Call::new([
                    OsStr::new("-C"),
                    checkout.as_os_str(),
                    OsStr::new("rev-parse"),
                    OsStr::new("--absolute-git-dir"),
                ]),
            )
            .await?;
        let git_dir = String::from_utf8_lossy(&git_dir.stdout).trim().to_string();
        if git_dir.is_empty() {
            return Err(host_failure("git rev-parse answered something unexpected"));
        }
        let dir = scratch_dir()?;
        let index_file = dir.path().join("index");
        std::fs::write(&index_file, index)
            .map_err(|e| host_failure(format!("a private temp file could not be written: {e}")))?;
        let prefix = |tail: &[&str]| {
            let mut args: Vec<OsString> = vec![
                OsString::from("-c"),
                OsString::from("core.autocrlf=false"),
                OsString::from("-c"),
                OsString::from("core.filemode=false"),
                OsString::from("-c"),
                OsString::from("core.fsmonitor=false"),
                OsString::from(format!("--git-dir={git_dir}")),
                OsString::from(format!("--work-tree={}", worktree.display())),
            ];
            args.extend(tail.iter().map(OsString::from));
            Call::new(args)
                .cwd(worktree)
                .env("GIT_INDEX_FILE", &index_file)
        };
        let files = self
            .run_ok(
                "diff",
                prefix(&[
                    "diff",
                    "--name-status",
                    "-z",
                    "--no-renames",
                    "--no-ext-diff",
                ]),
            )
            .await?;
        let others = self
            .run_ok(
                "ls-files",
                prefix(&[
                    "ls-files",
                    "--others",
                    "--exclude-per-directory=.gitignore",
                    "-z",
                ]),
            )
            .await?;
        let mut lines = StatusLines::default();
        let mut fields = files.stdout.split(|b| *b == 0).filter(|f| !f.is_empty());
        while let Some(status) = fields.next() {
            let path = fields
                .next()
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .ok_or_else(|| host_failure("git diff answered something unexpected"))?;
            match status {
                b"M" | b"T" => lines.modified.push(path),
                b"D" => lines.deleted.push(path),
                _ => {
                    return Err(host_failure(format!(
                        "git diff reported `{}` for `{}`",
                        String::from_utf8_lossy(status),
                        path.escape_debug()
                    )));
                }
            }
        }
        lines.untracked = others
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        lines.modified.sort();
        lines.deleted.sort();
        lines.untracked.sort();
        Ok(lines)
    }
}

/// A git directory objects are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitDir {
    /// A normal checkout (`-C <path>`).
    Checkout(PathBuf),
    /// A bare git dir (`--git-dir=<path>`), such as a scratch dir whose
    /// alternates point at another object store.
    Bare(PathBuf),
}

impl GitDir {
    /// `call` as it must run against this git dir: a bare dir is a scratch
    /// dir holding another party's objects, so it runs under an empty global
    /// config; a checkout is the operator's own and keeps their git
    /// configuration (`safe.directory` and the like).
    fn reading(&self, call: Call) -> Call {
        match self {
            GitDir::Checkout(_) => call,
            GitDir::Bare(_) => call.isolated(),
        }
    }

    fn prefix(&self) -> Vec<OsString> {
        match self {
            GitDir::Checkout(path) => vec![OsString::from("-C"), path.as_os_str().to_os_string()],
            GitDir::Bare(path) => {
                let mut arg = OsString::from("--git-dir=");
                arg.push(path.as_os_str());
                vec![arg]
            }
        }
    }
}

/// What ign's own release checkout looks like to git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutStatus {
    /// `HEAD` as a full commit id.
    pub head: String,
    /// Tracked paths whose content differs from `HEAD` (untracked files are
    /// not listed), byte-sorted.
    pub modified: Vec<String>,
}

/// Branches and tags are named by remotes and operators; the name becomes a
/// part of an argument (never of a shell line). A permissive subset check:
/// non-empty, no leading `-`, no control or space, none of git's forbidden
/// characters, no `..`, `@{`, `//`, trailing `/`, `.` or `.lock`.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("@{")
        && !name.contains("//")
        && name
            .chars()
            .all(|c| !c.is_control() && !" ~^:?*[\\".contains(c))
}

fn require_safe_name(name: &str, what: &str) -> Result<(), CoreError> {
    if safe_name(name) {
        Ok(())
    } else {
        Err(host_failure(format!(
            "the {what} name `{}` is not one ign passes to git",
            name.escape_debug()
        )))
    }
}

/// Argument vector `-C <checkout> <tail...>`.
fn in_checkout<I, S>(checkout: &Path, tail: I) -> Vec<OsString>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut args = vec![OsString::from("-C"), checkout.as_os_str().to_os_string()];
    args.extend(tail.into_iter().map(|a| a.as_ref().to_os_string()));
    args
}

/// How many refs one `fetch` names.
const FETCH_CHUNK: usize = 100;

/// The environment and arguments a remote read adds for the token.
#[derive(Default)]
struct RemoteAuth {
    envs: Vec<(String, String)>,
    args: Vec<OsString>,
}

impl RemoteAuth {
    fn apply(&self, mut call: Call) -> Call {
        for (key, value) in &self.envs {
            call = call.env(key, value);
        }
        call
    }
}

/// `text` without the token, its base64 and the base64 of the credentials
/// the extraheader carries: git's stderr is the only text of a child that
/// reaches a message, and nothing there may be the token.
fn scrub_token(text: &str, token: &str) -> String {
    let engine = base64::engine::general_purpose::STANDARD;
    let mut out = text.to_string();
    for secret in [
        token.to_string(),
        engine.encode(token),
        engine.encode(format!("x-access-token:{token}")),
    ] {
        if !secret.is_empty() {
            out = out.replace(&secret, "***");
        }
    }
    out
}

/// The text of a refusal to send the token over a remote that is not https.
fn token_refusal(uses: &str) -> String {
    if uses == "ssh" {
        "a git token applies only to https remotes; this remote uses ssh: give the machine \
         running ign ssh access to it and leave git-token empty"
            .to_string()
    } else {
        format!(
            "a git token applies only to https remotes; this remote uses {uses}: use an https \
             remote, or leave git-token empty"
        )
    }
}

/// The `GIT_CONFIG_*` environment that gives one remote read the token as an
/// `http.<scheme>://<host>[:<port>]/.extraheader`, scoped to exactly the
/// scheme, host and port of `url` (the matched remote's own URL, userinfo
/// dropped), never to any other host (T-27-41, T-27-74).
///
/// - No token (`None` or empty): nothing is added, for every kind of remote.
/// - `https://`: the scoped extraheader, `AUTHORIZATION: basic
///   base64(x-access-token:<token>)`, appended after `existing_count` entries
///   already in the environment (`GIT_CONFIG_COUNT`), never replacing them.
/// - `http://` to a loopback host (a local rig): the same, for `http`.
/// - `http://` to any other host, `ssh://` and scp-like remotes, and any
///   other transport: `Err` with the text that names the fix. The token is
///   never sent in clear and never to a transport it cannot serve.
/// - `file://` and local paths: nothing; the token has no use there.
///
/// The `Err` never holds the token or the URL.
pub fn remote_auth_env(
    url: &str,
    token: Option<&str>,
    existing_count: Option<usize>,
) -> Result<Vec<(String, String)>, String> {
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return Ok(Vec::new());
    };
    let url = url.trim();
    let (scheme, rest) = match url.find("://") {
        Some(at) => (url[..at].to_ascii_lowercase(), &url[at + 3..]),
        None => match url.find(':') {
            // `ext::command`, `transport::address`: a remote helper.
            Some(colon) if url[colon..].starts_with("::") => {
                return Err(token_refusal(&url[..colon].to_ascii_lowercase()));
            }
            // scp-like `user@host:path` (a one-letter head is a drive).
            Some(colon) if colon > 1 && !url[..colon].contains('/') => {
                return Err(token_refusal("ssh"));
            }
            // A local path.
            _ => return Ok(Vec::new()),
        },
    };
    match scheme.as_str() {
        "file" => return Ok(Vec::new()),
        "https" | "http" => {}
        "ssh" | "git+ssh" | "ssh+git" => return Err(token_refusal("ssh")),
        other => return Err(token_refusal(other)),
    }
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let host_port = authority
        .rfind('@')
        .map_or(authority, |at| &authority[at + 1..])
        .to_ascii_lowercase();
    let host = split_host_port(&host_port)
        .ok_or_else(|| "the remote's URL has no host a git token can be scoped to".to_string())?;
    if scheme == "http" && !is_loopback_host(host) {
        return Err(token_refusal(
            "plain http to a host that is not a loopback address",
        ));
    }
    let base = format!("{scheme}://{host_port}/");
    let index = existing_count.unwrap_or(0);
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    Ok(vec![
        (
            format!("GIT_CONFIG_KEY_{index}"),
            format!("http.{base}.extraheader"),
        ),
        (
            format!("GIT_CONFIG_VALUE_{index}"),
            format!("AUTHORIZATION: basic {credentials}"),
        ),
        ("GIT_CONFIG_COUNT".to_string(), (index + 1).to_string()),
    ])
}

/// The host of a lowercase `host[:port]` (an IPv6 host in brackets). `None`
/// for anything that could change what a config key means, a port that is not
/// 1 to 5 digits, or no host at all.
fn split_host_port(host_port: &str) -> Option<&str> {
    let (host, port) = if let Some(rest) = host_port.strip_prefix('[') {
        let close = rest.find(']')?;
        let host = &host_port[..close + 2];
        match &rest[close + 1..] {
            "" => (host, None),
            tail => (host, Some(tail.strip_prefix(':')?)),
        }
    } else {
        match host_port.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        }
    };
    let host_ok = !host.is_empty()
        && host.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'[' | b']' | b':')
        });
    let port_ok =
        port.is_none_or(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()));
    (host_ok && port_ok).then_some(host)
}

/// Whether `host` (lowercase, IPv6 in brackets) is this machine: `localhost`,
/// an address in `127.0.0.0/8` or `::1`.
fn is_loopback_host(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare == "localhost"
        || bare.ends_with(".localhost")
        || bare
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The remote and release-side operations of the break/fix gate (Phase 27,
/// 27-05). Everything here reads or adds objects in ign's own checkout; none
/// writes a ref, `FETCH_HEAD` or a config value. A remote is always named,
/// never passed as a URL.
impl HostGit {
    /// The commit `refs/tags/<tag>` peels to, or `None` when the tag does
    /// not exist.
    pub async fn release_commit(
        &self,
        checkout: &Path,
        tag: &str,
    ) -> Result<Option<String>, CoreError> {
        require_safe_name(tag, "tag")?;
        let done = self
            .run(Call::new(in_checkout(
                checkout,
                [
                    "rev-parse".to_string(),
                    "--verify".to_string(),
                    "--quiet".to_string(),
                    format!("refs/tags/{tag}^{{commit}}"),
                ],
            )))
            .await?;
        match done.code {
            Some(0) => {
                let id = String::from_utf8_lossy(&done.stdout).trim().to_string();
                require_object_id(&id)?;
                Ok(Some(id))
            }
            Some(1) => Ok(None),
            _ => Err(host_failure(format!(
                "git rev-parse of the release tag failed: {}",
                done.stderr.trim()
            ))),
        }
    }

    /// `HEAD` and the modified tracked paths of the checkout. Optional index
    /// locks are off, so the call never rewrites the index.
    pub async fn checkout_status(&self, checkout: &Path) -> Result<CheckoutStatus, CoreError> {
        let head = self
            .run_ok(
                "rev-parse HEAD",
                Call::new(in_checkout(checkout, ["rev-parse", "HEAD"])),
            )
            .await?;
        let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
        require_object_id(&head)?;
        let status = self
            .run_ok(
                "status",
                Call::new(in_checkout(
                    checkout,
                    [
                        "-c",
                        "core.fsmonitor=false",
                        "status",
                        "--porcelain=v1",
                        "-z",
                        "--untracked-files=no",
                        "--no-renames",
                    ],
                ))
                .env("GIT_OPTIONAL_LOCKS", "0"),
            )
            .await?;
        let mut modified: Vec<String> = status
            .stdout
            .split(|b| *b == 0)
            .filter(|entry| entry.len() > 3)
            .map(|entry| String::from_utf8_lossy(&entry[3..]).into_owned())
            .collect();
        modified.sort();
        Ok(CheckoutStatus { head, modified })
    }

    /// Whether the checkout is a shallow clone.
    pub async fn is_shallow(&self, checkout: &Path) -> Result<bool, CoreError> {
        let done = self
            .run_ok(
                "rev-parse --is-shallow-repository",
                Call::new(in_checkout(
                    checkout,
                    ["rev-parse", "--is-shallow-repository"],
                )),
            )
            .await?;
        match String::from_utf8_lossy(&done.stdout).trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(host_failure(
                "git rev-parse --is-shallow-repository answered something unexpected",
            )),
        }
    }

    /// The checkout's remotes as `(name, url)`, name-sorted. The URL may hold
    /// userinfo: keep it in memory, never put it on an argument list or in a
    /// message without [`redact_url`](super::git_state::redact_url).
    pub async fn remotes(&self, checkout: &Path) -> Result<Vec<(String, String)>, CoreError> {
        let done = self
            .run(Call::new(in_checkout(
                checkout,
                [
                    "config",
                    "--local",
                    "--get-regexp",
                    "-z",
                    r"^remote\..+\.url$",
                ],
            )))
            .await?;
        match done.code {
            Some(0) => {}
            Some(1) => return Ok(Vec::new()),
            _ => {
                return Err(host_failure(format!(
                    "git config --get-regexp failed: {}",
                    done.stderr.trim()
                )));
            }
        }
        let mut remotes: BTreeMap<String, String> = BTreeMap::new();
        for entry in done.stdout.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            let text = String::from_utf8_lossy(entry);
            let Some((key, url)) = text.split_once('\n') else {
                continue;
            };
            let Some(name) = key
                .strip_prefix("remote.")
                .and_then(|rest| rest.strip_suffix(".url"))
            else {
                continue;
            };
            if safe_name(name) {
                remotes
                    .entry(name.to_string())
                    .or_insert_with(|| url.to_string());
            }
        }
        Ok(remotes.into_iter().collect())
    }

    /// The `-c core.sshCommand=...` pair that makes ssh refuse to prompt, or
    /// nothing when the operator already chose an ssh command.
    async fn ssh_args(&self, checkout: &Path) -> Result<Vec<OsString>, CoreError> {
        if std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some() {
            return Ok(Vec::new());
        }
        let done = self
            .run(Call::new(in_checkout(
                checkout,
                ["config", "--get", "core.sshCommand"],
            )))
            .await?;
        match done.code {
            Some(0) => Ok(Vec::new()),
            Some(1) => Ok(vec![
                OsString::from("-c"),
                OsString::from("core.sshCommand=ssh -o BatchMode=yes"),
            ]),
            _ => Err(host_failure(format!(
                "git config --get core.sshCommand failed: {}",
                done.stderr.trim()
            ))),
        }
    }

    /// The branch heads of a named remote: short branch name to tip id.
    /// A branch whose name is not one ign passes to git is left out.
    pub async fn ls_remote_heads(
        &self,
        checkout: &Path,
        remote: &str,
    ) -> Result<BTreeMap<String, String>, CoreError> {
        require_safe_name(remote, "remote")?;
        let auth = self.remote_auth(checkout, remote).await?;
        let mut args = self.ssh_args(checkout).await?;
        args.extend(auth.args.clone());
        args.extend(in_checkout(
            checkout,
            ["ls-remote", "--heads", "--", remote],
        ));
        let done = self
            .run_ok("ls-remote", auth.apply(Call::new(args)))
            .await?;
        let mut heads = BTreeMap::new();
        for line in String::from_utf8_lossy(&done.stdout).lines() {
            let Some((id, name)) = line.split_once('\t') else {
                return Err(host_failure("git ls-remote answered something unexpected"));
            };
            require_object_id(id)?;
            let Some(branch) = name.strip_prefix("refs/heads/") else {
                continue;
            };
            if safe_name(branch) {
                heads.insert(branch.to_string(), id.to_string());
            }
        }
        Ok(heads)
    }

    /// Fetch the objects of `branches` of a named remote into the checkout's
    /// object store and nothing else: no destination, no remote-tracking
    /// update (`--refmap=` with no value), no `FETCH_HEAD`, no tags, no
    /// prune, no gc.
    pub async fn fetch_heads(
        &self,
        checkout: &Path,
        remote: &str,
        branches: &[String],
    ) -> Result<(), CoreError> {
        require_safe_name(remote, "remote")?;
        for branch in branches {
            require_safe_name(branch, "branch")?;
        }
        let auth = self.remote_auth(checkout, remote).await?;
        let ssh = self.ssh_args(checkout).await?;
        for chunk in branches.chunks(FETCH_CHUNK) {
            let mut args = ssh.clone();
            args.extend(auth.args.clone());
            args.extend(in_checkout(
                checkout,
                [
                    "-c",
                    "gc.auto=0",
                    "-c",
                    "maintenance.auto=false",
                    "-c",
                    "fetch.writeCommitGraph=false",
                    "fetch",
                    "--no-write-fetch-head",
                    "--no-tags",
                    "--no-prune",
                    "--no-recurse-submodules",
                    "--quiet",
                    "--refmap=",
                    remote,
                ],
            ));
            args.extend(
                chunk
                    .iter()
                    .map(|b| OsString::from(format!("refs/heads/{b}"))),
            );
            self.run_ok("fetch", auth.apply(Call::new(args))).await?;
        }
        Ok(())
    }

    /// What the remote read of `remote` adds for the token: nothing when no
    /// token is set (no extra process runs then), else the token as an
    /// extraheader scoped to the named remote's own URL ([`remote_auth_env`]).
    /// The URL is read from the checkout (with the operator's `insteadOf`
    /// rewrites, which decide what git contacts), kept in memory, and never
    /// put on an argument list or in a message.
    async fn remote_auth(&self, checkout: &Path, remote: &str) -> Result<RemoteAuth, CoreError> {
        let Some(token) = self.remote_token.as_deref() else {
            return Ok(RemoteAuth::default());
        };
        let done = self
            .run_ok(
                "ls-remote --get-url",
                Call::new(in_checkout(
                    checkout,
                    ["ls-remote", "--get-url", "--", remote],
                )),
            )
            .await?;
        let url = String::from_utf8_lossy(&done.stdout).trim().to_string();
        let existing = match std::env::var("GIT_CONFIG_COUNT") {
            Ok(count) => Some(count.trim().parse::<usize>().map_err(|_| {
                host_failure(
                    "the GIT_CONFIG_COUNT environment variable is not a number, so the git token \
                     cannot be added to it: unset it",
                )
            })?),
            Err(_) => None,
        };
        let envs = remote_auth_env(&url, Some(token), existing).map_err(host_failure)?;
        // With a token, a redirect is never followed: git would send the
        // header to wherever the remote points. A remote that moved needs its
        // URL updated.
        let args = if envs.is_empty() {
            Vec::new()
        } else {
            vec![
                OsString::from("-c"),
                OsString::from("http.followRedirects=false"),
            ]
        };
        Ok(RemoteAuth { envs, args })
    }

    /// Which of `ids` are commits in the checkout's object store, through
    /// one `cat-file --batch-check`.
    pub async fn has_commits(
        &self,
        checkout: &Path,
        ids: &[String],
    ) -> Result<std::collections::BTreeSet<String>, CoreError> {
        let mut found = std::collections::BTreeSet::new();
        if ids.is_empty() {
            return Ok(found);
        }
        let mut input = String::new();
        for id in ids {
            require_object_id(id)?;
            input.push_str(id);
            input.push('\n');
        }
        let done = self
            .run_ok(
                "cat-file --batch-check",
                Call::new(in_checkout(checkout, ["cat-file", "--batch-check"]))
                    .stdin(input.into_bytes()),
            )
            .await?;
        let text = String::from_utf8_lossy(&done.stdout);
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() != ids.len() {
            return Err(host_failure(
                "git cat-file --batch-check answered something unexpected",
            ));
        }
        for (id, line) in ids.iter().zip(lines) {
            let fields: Vec<&str> = line.split(' ').collect();
            match fields.as_slice() {
                [got, "missing"] if got == id => {}
                [got, kind, size] if got == id && size.bytes().all(|b| b.is_ascii_digit()) => {
                    if *kind == "commit" {
                        found.insert(id.clone());
                    }
                }
                _ => {
                    return Err(host_failure(
                        "git cat-file --batch-check answered something unexpected",
                    ));
                }
            }
        }
        Ok(found)
    }

    /// Whether `a` is an ancestor of `b` (equal counts). Both must be commits
    /// in the store; anything but a clean yes or no fails closed.
    pub async fn is_ancestor(&self, checkout: &Path, a: &str, b: &str) -> Result<bool, CoreError> {
        require_object_id(a)?;
        require_object_id(b)?;
        let done = self
            .run(Call::new(in_checkout(
                checkout,
                ["merge-base", "--is-ancestor", a, b],
            )))
            .await?;
        match done.code {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(host_failure(format!(
                "git merge-base --is-ancestor failed: {}",
                done.stderr.trim()
            ))),
        }
    }

    /// The first `cap` commits reachable from `include` and not from
    /// `exclude`, newest first.
    pub async fn rev_list(
        &self,
        checkout: &Path,
        include: &str,
        exclude: &str,
        cap: usize,
    ) -> Result<Vec<String>, CoreError> {
        require_object_id(include)?;
        require_object_id(exclude)?;
        let done = self
            .run_ok(
                "rev-list",
                Call::new(in_checkout(
                    checkout,
                    [
                        "rev-list".to_string(),
                        format!("--max-count={cap}"),
                        include.to_string(),
                        format!("^{exclude}"),
                    ],
                )),
            )
            .await?;
        let mut ids = Vec::new();
        for line in String::from_utf8_lossy(&done.stdout).lines() {
            require_object_id(line)?;
            ids.push(line.to_string());
        }
        Ok(ids)
    }

    /// How many commits are reachable from `include` and not from `exclude`.
    pub async fn rev_count(
        &self,
        checkout: &Path,
        include: &str,
        exclude: &str,
    ) -> Result<usize, CoreError> {
        require_object_id(include)?;
        require_object_id(exclude)?;
        let done = self
            .run_ok(
                "rev-list --count",
                Call::new(in_checkout(
                    checkout,
                    [
                        "rev-list".to_string(),
                        "--count".to_string(),
                        include.to_string(),
                        format!("^{exclude}"),
                    ],
                )),
            )
            .await?;
        String::from_utf8_lossy(&done.stdout)
            .trim()
            .parse()
            .map_err(|_| host_failure("git rev-list --count answered something unexpected"))
    }

    /// Write every entry of `commit`'s tree under `dir` (created if needed)
    /// from the raw blob bytes, so the repository's own HEAD, index and work
    /// tree are untouched.
    ///
    /// The tree is listed with `ls-tree -r -z` and the blobs are read with
    /// `cat-file --batch`, then written by this function. Nothing here is a
    /// checkout: gitattributes (`text`, `eol`, `ident`, `working-tree-encoding`,
    /// `filter`) from the commit's own `.gitattributes`, and any filter an
    /// operator's git config defines, never apply, so no filter process runs
    /// and the bytes are what the gateway itself would write. Regular files
    /// keep the exec bit, a symlink is written as a symlink (never followed:
    /// the canonical digest refuses it), a gitlink is an empty directory, and
    /// a path that is absolute, holds `..`, `.` or a `.git` component, or
    /// meets anything already on disk is refused before it is written.
    pub async fn materialize(
        &self,
        repo: &GitDir,
        commit: &str,
        dir: &Path,
    ) -> Result<(), CoreError> {
        require_object_id(commit)?;
        let dir = std::path::absolute(dir)
            .map_err(|e| host_failure(format!("the target path could not be resolved: {e}")))?;
        let mut list_args = repo.prefix();
        list_args.extend(["ls-tree", "-r", "-z", "--full-tree", commit].map(OsString::from));
        let listed = self
            .run_ok("ls-tree", repo.reading(Call::new(list_args)))
            .await?;
        let entries = parse_tree_entries(&listed.stdout)?;
        let mut want = Vec::new();
        for entry in &entries {
            if entry.kind.has_blob() {
                want.push(entry.oid.as_str());
            }
        }
        let mut blobs = if want.is_empty() {
            Vec::new().into_iter()
        } else {
            let mut input = Vec::new();
            for oid in &want {
                input.extend_from_slice(oid.as_bytes());
                input.push(b'\n');
            }
            let mut read_args = repo.prefix();
            read_args.extend(["cat-file", "--batch"].map(OsString::from));
            let done = self
                .run_ok(
                    "cat-file --batch",
                    repo.reading(Call::new(read_args))
                        .stdin(input)
                        .max_output(LARGE_OUTPUT_MAX),
                )
                .await?;
            split_batch(&done.stdout, &want)?.into_iter()
        };
        std::fs::create_dir_all(&dir)
            .map_err(|e| host_failure(format!("the target dir could not be made: {e}")))?;
        for entry in &entries {
            let bytes = if entry.kind.has_blob() {
                blobs.next()
            } else {
                None
            };
            write_tree_entry(&dir, entry, bytes.as_deref())?;
        }
        Ok(())
    }
}

/// Largest `.idx` ign reads into memory to list the objects a pack holds.
const IDX_READ_MAX: u64 = 512 * 1024 * 1024;

/// A private git dir ign builds to read a gateway repo's objects (Phase 27,
/// 27-10, D-24).
///
/// The skeleton is made by hand, as `parse_refs` makes its own: an empty
/// object store, a `config` with only `repositoryformatversion`, a `HEAD`, no
/// hooks, no templates, no `info/`. Its `objects/info/alternates` names ign's
/// own checkout's object directory, so the objects a gateway commit shares
/// with the release are never fetched. Nothing from the gateway's `config`,
/// hooks or `info/` ever enters it, and the alternates point from here to
/// ign's store, never the other way. The directory is removed when the value
/// is dropped, on every return path.
#[derive(Debug)]
pub struct ScratchGitDir {
    dir: tempfile::TempDir,
    git_dir: PathBuf,
}

impl ScratchGitDir {
    /// The git dir (`objects/` holds the copied gateway objects).
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// The private temp dir that holds the git dir and any staging dir beside
    /// it.
    pub fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// Object-level reads over a [`ScratchGitDir`] (27-10).
impl HostGit {
    /// A fresh scratch git dir whose alternates name `checkout`'s object
    /// directory.
    pub async fn init_scratch(&self, checkout: &Path) -> Result<ScratchGitDir, CoreError> {
        let checkout = std::path::absolute(checkout)
            .map_err(|e| host_failure(format!("the checkout path could not be resolved: {e}")))?;
        let printed = self
            .run_ok(
                "rev-parse --git-path",
                Call::new(in_checkout(
                    &checkout,
                    ["rev-parse", "--git-path", "objects"],
                )),
            )
            .await?;
        let printed = String::from_utf8_lossy(&printed.stdout);
        let printed = printed.trim_end_matches('\n');
        if printed.is_empty() || printed.contains(['\n', '\0']) {
            return Err(host_failure(
                "git rev-parse --git-path answered something unexpected",
            ));
        }
        let objects = checkout.join(printed);
        if !objects.is_dir() {
            return Err(host_failure(
                "the release checkout has no object directory to read through",
            ));
        }
        let alternate = objects.to_string_lossy().into_owned();
        if alternate.contains(['\n', '\r']) || alternate.starts_with('#') {
            return Err(host_failure(
                "the release checkout's object directory path cannot be named in an alternates file",
            ));
        }
        let dir = scratch_dir()?;
        let git_dir = dir.path().join("scratch.git");
        for sub in ["objects/info", "objects/pack", "refs/heads", "refs/tags"] {
            std::fs::create_dir_all(git_dir.join(sub))
                .map_err(|e| host_failure(format!("a scratch git dir could not be made: {e}")))?;
        }
        write_scratch(
            &git_dir.join("config"),
            b"[core]\n\trepositoryformatversion = 0\n",
        )?;
        write_scratch(&git_dir.join("HEAD"), b"ref: refs/heads/scratch\n")?;
        write_scratch(
            &git_dir.join("objects").join("info").join("alternates"),
            format!("{alternate}\n").as_bytes(),
        )?;
        Ok(ScratchGitDir { dir, git_dir })
    }

    /// The ids of the objects `commit`'s tree needs that neither the scratch
    /// dir nor ign's store (through the alternates) holds.
    ///
    /// When the commit itself is missing the answer is that one id (its tree
    /// cannot be walked yet). Otherwise `rev-list --objects --no-walk
    /// --missing=print` names every missing tree and blob with a `?` line.
    /// Any other failure is an error: a missing object is never read as "none
    /// missing".
    pub async fn missing_objects(
        &self,
        scratch: &ScratchGitDir,
        commit: &str,
    ) -> Result<std::collections::BTreeSet<String>, CoreError> {
        require_object_id(commit)?;
        let repo = GitDir::Bare(scratch.git_dir().to_path_buf());
        let mut check = repo.prefix();
        check.extend(["cat-file", "--batch-check"].map(OsString::from));
        let done = self
            .run_ok(
                "cat-file --batch-check",
                Call::new(check)
                    .cwd(scratch.root())
                    .isolated()
                    .stdin(format!("{commit}\n").into_bytes()),
            )
            .await?;
        let text = String::from_utf8_lossy(&done.stdout);
        let line = text.trim();
        let mut missing = std::collections::BTreeSet::new();
        if line == format!("{commit} missing") {
            missing.insert(commit.to_string());
            return Ok(missing);
        }
        match line.split(' ').collect::<Vec<_>>().as_slice() {
            [id, "commit", size] if *id == commit && size.bytes().all(|b| b.is_ascii_digit()) => {}
            _ => {
                return Err(host_failure(
                    "the gateway HEAD is not a commit in the gateway's objects",
                ));
            }
        }
        let mut walk = repo.prefix();
        walk.extend(
            [
                "rev-list",
                "--objects",
                "--no-walk",
                "--missing=print",
                commit,
            ]
            .map(OsString::from),
        );
        let done = self
            .run_ok(
                "rev-list --missing=print",
                Call::new(walk).cwd(scratch.root()).isolated(),
            )
            .await?;
        for line in String::from_utf8_lossy(&done.stdout).lines() {
            if let Some(id) = line.strip_prefix('?') {
                if !is_object_id(id) {
                    return Err(host_failure(
                        "git rev-list --missing=print answered something unexpected",
                    ));
                }
                missing.insert(id.to_string());
            }
        }
        Ok(missing)
    }

    /// The ids of the objects the pack index `idx` lists, through `git
    /// show-index` fed the file on stdin (the index bytes are parsed by host
    /// git, never by ign).
    pub async fn pack_ids(
        &self,
        scratch: &ScratchGitDir,
        idx: &Path,
    ) -> Result<Vec<String>, CoreError> {
        let meta = std::fs::symlink_metadata(idx)
            .map_err(|e| host_failure(format!("a pack index cannot be inspected: {}", e.kind())))?;
        if !meta.file_type().is_file() || meta.len() > IDX_READ_MAX {
            return Err(host_failure(
                "a pack index is not a regular file within the size ign reads",
            ));
        }
        let bytes = std::fs::read(idx)
            .map_err(|e| host_failure(format!("a pack index cannot be read: {}", e.kind())))?;
        // Inside the scratch git dir: `git show-index` run outside any
        // repository crashes on some versions (2.46 exits on a signal).
        let mut args = GitDir::Bare(scratch.git_dir().to_path_buf()).prefix();
        args.push(OsString::from("show-index"));
        let done = self
            .run_ok(
                "show-index",
                Call::new(args)
                    .cwd(scratch.root())
                    .isolated()
                    .stdin(bytes)
                    .max_output(LARGE_OUTPUT_MAX),
            )
            .await?;
        let mut ids = Vec::new();
        for line in String::from_utf8_lossy(&done.stdout).lines() {
            let id = line.split(' ').nth(1).unwrap_or("");
            if !is_object_id(id) {
                return Err(host_failure("git show-index answered something unexpected"));
            }
            ids.push(id.to_string());
        }
        Ok(ids)
    }
}

/// What a tree entry is, from its mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Executable,
    Symlink,
    Gitlink,
}

impl EntryKind {
    fn has_blob(self) -> bool {
        !matches!(self, EntryKind::Gitlink)
    }
}

/// One line of `ls-tree -r -z`.
#[derive(Debug)]
struct TreeEntry {
    kind: EntryKind,
    oid: String,
    path: PathBuf,
}

fn tree_failure() -> CoreError {
    host_failure("git ls-tree answered something unexpected")
}

/// Parse `ls-tree -r -z` output (`<mode> SP <type> SP <oid> TAB <path> NUL`).
/// A mode or type outside blob 100644 / 100755 / 120000 and commit 160000,
/// or an unsafe path, is an error.
fn parse_tree_entries(out: &[u8]) -> Result<Vec<TreeEntry>, CoreError> {
    let mut entries = Vec::new();
    for record in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or_else(tree_failure)?;
        let meta = std::str::from_utf8(&record[..tab]).map_err(|_| tree_failure())?;
        let mut fields = meta.split(' ');
        let (Some(mode), Some(kind), Some(oid), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(tree_failure());
        };
        let kind = match (mode, kind) {
            ("100644", "blob") => EntryKind::File,
            ("100755", "blob") => EntryKind::Executable,
            ("120000", "blob") => EntryKind::Symlink,
            ("160000", "commit") => EntryKind::Gitlink,
            _ => {
                return Err(host_failure(format!(
                    "the tree holds an entry of mode {} that cannot be written",
                    mode.escape_debug()
                )));
            }
        };
        if !is_object_id(oid) {
            return Err(tree_failure());
        }
        entries.push(TreeEntry {
            kind,
            oid: oid.to_string(),
            path: tree_entry_path(&record[tab + 1..])?,
        });
    }
    Ok(entries)
}

/// `raw` as a relative path of plain components. Absolute paths, empty
/// components, `.`, `..` and any `.git` component (any case, and the 8.3 name
/// `git~1`) are refused; NUL cannot appear in a `-z` record, but is refused
/// all the same.
fn tree_entry_path(raw: &[u8]) -> Result<PathBuf, CoreError> {
    let unsafe_path = || {
        host_failure(format!(
            "the tree holds an unsafe path (`{}`)",
            String::from_utf8_lossy(raw).escape_debug()
        ))
    };
    if raw.is_empty() || raw.contains(&0) {
        return Err(unsafe_path());
    }
    let mut path = PathBuf::new();
    for part in raw.split(|b| *b == b'/') {
        if part.is_empty()
            || part == b"."
            || part == b".."
            || part.eq_ignore_ascii_case(b".git")
            || part.eq_ignore_ascii_case(b"git~1")
        {
            return Err(unsafe_path());
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            path.push(OsStr::from_bytes(part));
        }
        #[cfg(not(unix))]
        {
            let part = std::str::from_utf8(part).map_err(|_| unsafe_path())?;
            if part.contains(['\\', ':']) {
                return Err(unsafe_path());
            }
            path.push(part);
        }
    }
    Ok(path)
}

/// Cut `cat-file --batch` output (`<oid> blob <size> LF <bytes> LF` per
/// requested id, in order) into one byte vector per id.
fn split_batch(out: &[u8], oids: &[&str]) -> Result<Vec<Vec<u8>>, CoreError> {
    let bad = || host_failure("git cat-file --batch answered something unexpected");
    let mut pos = 0usize;
    let mut blobs = Vec::with_capacity(oids.len());
    for oid in oids {
        let eol = out[pos..]
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(bad)?;
        let header = String::from_utf8_lossy(&out[pos..pos + eol]).into_owned();
        pos += eol + 1;
        let mut fields = header.split(' ');
        let (Some(id), Some("blob"), Some(size), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(bad());
        };
        if id != *oid {
            return Err(bad());
        }
        let size: usize = size.parse().map_err(|_| bad())?;
        let end = pos.checked_add(size).ok_or_else(bad)?;
        if end >= out.len() || out[end] != b'\n' {
            return Err(bad());
        }
        blobs.push(out[pos..end].to_vec());
        pos = end + 1;
    }
    Ok(blobs)
}

/// Make the directories above `entry` under `root`, one component at a time,
/// refusing to step through anything that is not a plain directory (a
/// symlink in particular).
fn make_parents(root: &Path, entry: &Path) -> Result<PathBuf, CoreError> {
    let mut at = root.to_path_buf();
    let parent = entry.parent().unwrap_or(Path::new(""));
    for part in parent.components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(_) => {
                return Err(host_failure(
                    "a tree path runs through something that is not a directory",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&at).map_err(|e| {
                    host_failure(format!("a tree directory could not be made: {e}"))
                })?;
            }
            Err(e) => return Err(host_failure(format!("a tree path could not be read: {e}"))),
        }
    }
    Ok(at.join(
        entry
            .file_name()
            .ok_or_else(|| host_failure("a tree path is empty"))?,
    ))
}

/// Write one entry as a new file, symlink or directory; anything already at
/// the path is refused.
fn write_tree_entry(root: &Path, entry: &TreeEntry, bytes: Option<&[u8]>) -> Result<(), CoreError> {
    let target = make_parents(root, &entry.path)?;
    let failed =
        |e: std::io::Error| host_failure(format!("a tree entry could not be written: {e}"));
    match entry.kind {
        EntryKind::Gitlink => std::fs::create_dir(&target).map_err(failed),
        EntryKind::Symlink => {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                let link = OsStr::from_bytes(bytes.unwrap_or_default());
                std::os::unix::fs::symlink(link, &target).map_err(failed)
            }
            #[cfg(not(unix))]
            {
                let _ = bytes;
                Err(host_failure("a symlink in the tree cannot be written here"))
            }
        }
        EntryKind::File | EntryKind::Executable => {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(failed)?;
            file.write_all(bytes.unwrap_or_default()).map_err(failed)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if entry.kind == EntryKind::Executable {
                    0o755
                } else {
                    0o644
                };
                file.set_permissions(std::fs::Permissions::from_mode(mode))
                    .map_err(failed)?;
            }
            Ok(())
        }
    }
}

fn require_object_id(id: &str) -> Result<(), CoreError> {
    if is_object_id(id) {
        Ok(())
    } else {
        Err(host_failure("a commit id is not a full lowercase hex id"))
    }
}

fn scratch_dir() -> Result<tempfile::TempDir, CoreError> {
    tempfile::tempdir()
        .map_err(|e| host_failure(format!("a private temp dir could not be made: {e}")))
}

fn write_scratch(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    std::fs::write(path, bytes)
        .map_err(|e| host_failure(format!("a scratch git file could not be written: {e}")))
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    Some((parts.next()??, parts.next()??))
}

/// Fold one `key` / `value` pair of `git config --list` into `config`.
fn record_config_entry(config: &mut GitConfig, key: &str, value: String) {
    if let Some(rest) = key.strip_prefix("remote.")
        && let Some((name, var)) = rest.rsplit_once('.')
    {
        let remote = config
            .remotes
            .entry(name.to_string())
            .or_insert_with(|| GitRemoteConfig {
                url: String::new(),
                fetch: Vec::new(),
            });
        match var {
            "url" if remote.url.is_empty() => remote.url = value,
            "fetch" => remote.fetch.push(value),
            _ => {}
        }
    } else if let Some(rest) = key.strip_prefix("branch.")
        && let Some((name, var)) = rest.rsplit_once('.')
    {
        let branch = config
            .branches
            .entry(name.to_string())
            .or_insert_with(|| GitBranchConfig {
                remote: None,
                merge: None,
            });
        // git reads these two as single values: the last one listed wins
        // (WR-A5). `remote.<n>.url` above stays first-wins, because git
        // fetches from the first url.
        match var {
            "remote" => branch.remote = Some(value),
            "merge" => branch.merge = Some(value),
            _ => {}
        }
    }
}

/// Read up to `max` bytes; more is an error (and the child is killed when the
/// caller drops it).
async fn read_capped<R: AsyncRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> Result<Vec<u8>, CoreError> {
    let mut buf = Vec::new();
    let mut limited = reader.take(max as u64 + 1);
    limited
        .read_to_end(&mut buf)
        .await
        .map_err(|e| host_failure(format!("git output could not be read: {}", e.kind())))?;
    if buf.len() > max {
        return Err(host_failure(format!(
            "git wrote more than the {max} byte output limit"
        )));
    }
    Ok(buf)
}

/// Keep the first `keep` bytes of stderr and drain the rest so the child
/// never blocks on a full pipe.
async fn read_stderr<R: AsyncRead + Unpin>(reader: &mut R, keep: usize) -> String {
    let mut buf = Vec::new();
    let mut limited = reader.take(keep as u64);
    let _ = limited.read_to_end(&mut buf).await;
    let reader = limited.into_inner();
    let _ = tokio::io::copy(reader, &mut tokio::io::sink()).await;
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// Run host git in tests under a clean environment.
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

    /// A repo with one commit holding `files`.
    fn repo_with(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "production"]);
        for (name, body) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, body).expect("write");
        }
        git(dir.path(), &["add", "-A"]);
        git(
            dir.path(),
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "one"],
        );
        let commit = git(dir.path(), &["rev-parse", "HEAD"]);
        (dir, commit)
    }

    #[cfg(unix)]
    fn stub(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write stub");
        let mut perms = std::fs::metadata(&path).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        path
    }

    fn failed_check(err: &CoreError) -> String {
        match err {
            CoreError::DeployPreflightFailed { failures } => failures[0].check.clone(),
            other => panic!("not a preflight failure: {other:?}"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn version_floor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = stub(dir.path(), "old", "echo 'git version 2.20.0'");
        let err = HostGit::with_program(&old)
            .version()
            .await
            .expect_err("below the floor");
        assert_eq!(err.code(), "deploy_preflight_failed");
        assert_eq!(failed_check(&err), "git_host");

        // WR-A3 (27-27): 2.31 has GIT_CONFIG_COUNT but not GIT_CONFIG_GLOBAL.
        let almost = stub(dir.path(), "almost", "echo 'git version 2.31.9'");
        let err = HostGit::with_program(&almost)
            .version()
            .await
            .expect_err("below the floor");
        assert_eq!(failed_check(&err), "git_host");
        assert!(err.to_string().contains("2.32"), "{err}");
        let floor = stub(dir.path(), "floor", "echo 'git version 2.32.0'");
        assert_eq!(
            HostGit::with_program(&floor).version().await.expect("ok"),
            (2, 32)
        );

        let new = stub(dir.path(), "new", "echo 'git version 2.46.2'");
        assert_eq!(
            HostGit::with_program(&new).version().await.expect("ok"),
            (2, 46)
        );
        let apple = stub(
            dir.path(),
            "apple",
            "echo 'git version 2.39.5 (Apple Git-154)'",
        );
        assert_eq!(
            HostGit::with_program(&apple).version().await.expect("ok"),
            (2, 39)
        );
        let junk = stub(dir.path(), "junk", "echo hello");
        assert!(HostGit::with_program(&junk).version().await.is_err());
        assert!(HostGit::new().version().await.expect("host git") >= GIT_MIN_VERSION);
    }

    /// WR-A5 (27-27): git reads `branch.<b>.remote` and `.merge` as single
    /// values, so the last one listed wins; `remote.<n>.url` stays the first
    /// (git fetches from the first url).
    #[tokio::test]
    async fn config_branch_keys_are_last_wins() {
        let text = "[remote \"origin\"]\n\turl = https://first.example/a.git\n\
            \turl = https://second.example/b.git\n\
            [branch \"master\"]\n\tremote = a\n\tmerge = refs/heads/one\n\
            \tremote = origin\n\tmerge = refs/heads/two\n";
        let parsed = HostGit::new()
            .parse_config(text.as_bytes())
            .await
            .expect("parse");
        let branch = &parsed.branches["master"];
        assert_eq!(branch.remote.as_deref(), Some("origin"));
        assert_eq!(branch.merge.as_deref(), Some("refs/heads/two"));
        assert_eq!(parsed.remotes["origin"].url, "https://first.example/a.git");
    }

    #[tokio::test]
    async fn missing_program_fails_closed() {
        let git = HostGit::with_program("/nonexistent/ign-test-git");
        for err in [
            git.version().await.expect_err("version"),
            git.parse_config(b"").await.expect_err("config"),
            git.has_commit(Path::new("."), SHA_A)
                .await
                .expect_err("has_commit"),
        ] {
            assert_eq!(failed_check(&err), "git_host");
        }
    }

    const MODULE_CONFIG: &str = "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\
[remote \"capapp\"]\n\turl = https://user:secret@git.example/org/capapp.git\n\
\tfetch = +refs/heads/production:refs/remotes/capapp/production\n\
[branch \"production\"]\n\tremote = capapp\n\tmerge = refs/heads/production\n\
[http]\n\tsslVerify = false\n";

    #[tokio::test]
    async fn parse_config_module_clone() {
        let config = HostGit::new()
            .parse_config(MODULE_CONFIG.as_bytes())
            .await
            .expect("parse");
        assert_eq!(config.remotes.len(), 1);
        let remote = &config.remotes["capapp"];
        assert_eq!(remote.url, "https://user:secret@git.example/org/capapp.git");
        assert_eq!(
            remote.fetch,
            vec!["+refs/heads/production:refs/remotes/capapp/production"]
        );
        assert_eq!(config.branches.len(), 1);
        assert_eq!(
            config.branches["production"].remote.as_deref(),
            Some("capapp")
        );
        assert_eq!(
            config.branches["production"].merge.as_deref(),
            Some("refs/heads/production")
        );
        assert!(!format!("{config:?}").contains("secret"));
    }

    #[tokio::test]
    async fn parse_config_ignores_includes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let extra = dir.path().join("extra");
        std::fs::write(
            &extra,
            "[remote \"other\"]\n\turl = https://git.example/o/r\n",
        )
        .expect("write");
        let text = format!("{MODULE_CONFIG}[include]\n\tpath = {}\n", extra.display());
        let config = HostGit::new()
            .parse_config(text.as_bytes())
            .await
            .expect("parse");
        assert_eq!(config.remotes.keys().collect::<Vec<_>>(), ["capapp"]);
    }

    const PARTIAL_CONFIG: &str = "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\
[remote \"origin\"]\n\turl = https://user:secret@git.example/org/capapp.git\n\
\tfetch = +refs/heads/*:refs/remotes/origin/*\n\
[http]\n\tsslVerify = false\n";

    #[tokio::test]
    async fn adopt_branch_config_adds_the_upstream_section() {
        let host = HostGit::new();
        let out = host
            .adopt_branch_config(PARTIAL_CONFIG.as_bytes(), "production", "origin")
            .await
            .expect("adopted");
        let text = String::from_utf8(out.clone()).expect("utf8");
        // What the module wrote stays, the section is added.
        assert!(text.starts_with(PARTIAL_CONFIG), "{text}");
        assert!(text.contains("[branch \"production\"]"), "{text}");
        let parsed = host.parse_config(&out).await.expect("parse");
        let branch = &parsed.branches["production"];
        assert_eq!(branch.remote.as_deref(), Some("origin"));
        assert_eq!(branch.merge.as_deref(), Some("refs/heads/production"));
        assert_eq!(parsed.remotes.len(), 1);

        // A branch name with a slash is one subsection.
        let out = host
            .adopt_branch_config(PARTIAL_CONFIG.as_bytes(), "release/1.2", "origin")
            .await
            .expect("adopted");
        let parsed = host.parse_config(&out).await.expect("parse");
        assert_eq!(
            parsed.branches["release/1.2"].merge.as_deref(),
            Some("refs/heads/release/1.2")
        );
    }

    /// Each value is added only when absent; the same value again changes
    /// nothing, another value refuses.
    #[tokio::test]
    async fn adopt_branch_config_only_when_absent() {
        let host = HostGit::new();
        let once = host
            .adopt_branch_config(PARTIAL_CONFIG.as_bytes(), "production", "origin")
            .await
            .expect("adopted");
        let twice = host
            .adopt_branch_config(&once, "production", "origin")
            .await
            .expect("idempotent");
        assert_eq!(once, twice);

        let half = format!("{PARTIAL_CONFIG}[branch \"production\"]\n\tremote = origin\n");
        let out = host
            .adopt_branch_config(half.as_bytes(), "production", "origin")
            .await
            .expect("completed");
        let parsed = host.parse_config(&out).await.expect("parse");
        assert_eq!(
            parsed.branches["production"].merge.as_deref(),
            Some("refs/heads/production")
        );

        for other in [
            "[branch \"production\"]\n\tremote = elsewhere\n",
            "[branch \"production\"]\n\tmerge = refs/heads/other\n",
        ] {
            let text = format!("{PARTIAL_CONFIG}{other}");
            let err = host
                .adopt_branch_config(text.as_bytes(), "production", "origin")
                .await
                .expect_err("another value is not rewritten");
            assert_eq!(err.code(), "deploy_preflight_failed");
            assert!(err.to_string().contains("already sets"), "{err}");
        }
    }

    /// The config is copied to a private file: an include is never followed
    /// and the gateway's own bytes are never a git dir (T-27-40).
    #[tokio::test]
    async fn adopt_branch_config_follows_no_include_and_checks_names() {
        let host = HostGit::new();
        let dir = tempfile::tempdir().expect("tempdir");
        let extra = dir.path().join("extra");
        std::fs::write(&extra, "[branch \"production\"]\n\tremote = elsewhere\n").expect("write");
        let text = format!("{PARTIAL_CONFIG}[include]\n\tpath = {}\n", extra.display());
        let out = host
            .adopt_branch_config(text.as_bytes(), "production", "origin")
            .await
            .expect("an include is not followed");
        assert!(String::from_utf8_lossy(&out).contains("remote = origin"));

        for bad in ["-x", "a..b", "x.lock", "", "a b", "@{-1}"] {
            let err = host
                .adopt_branch_config(PARTIAL_CONFIG.as_bytes(), bad, "origin")
                .await
                .expect_err(bad);
            assert_eq!(err.code(), "deploy_preflight_failed", "{bad}");
        }
        for bad in ["-x", "a b", "", "a..b"] {
            assert!(
                host.adopt_branch_config(PARTIAL_CONFIG.as_bytes(), "production", bad)
                    .await
                    .is_err(),
                "remote {bad:?}"
            );
        }
        let big = vec![b'#'; GIT_SMALL_FILE_MAX + 1];
        assert!(
            host.adopt_branch_config(&big, "production", "origin")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn check_branch_name_is_pure_then_git() {
        let host = HostGit::new();
        for ok in ["production", "release/1.2"] {
            host.check_branch_name(ok).await.expect(ok);
        }
        for bad in [
            "",
            "-x",
            "--upload-pack=x",
            "a..b",
            "x.lock",
            "a b",
            "x~1",
            "@",
        ] {
            assert!(host.check_branch_name(bad).await.is_err(), "{bad:?}");
        }
        let err = host.check_branch_name("a..b").await.expect_err("bad");
        assert!(!err.to_string().contains("a..b"), "the name is not echoed");
    }

    fn files_with(head: &str) -> GitFiles {
        let mut files =
            GitFiles::without_contents(crate::deploy::transport::projects::GitEntry::Directory);
        files.head = Some(head.as_bytes().to_vec());
        files
    }

    #[tokio::test]
    async fn parse_refs_loose_over_packed() {
        let mut files = files_with("ref: refs/heads/production\n");
        files.packed_refs = Some(
            format!(
                "# pack-refs with: peeled fully-peeled sorted \n{SHA_A} refs/remotes/capapp/production\n{SHA_A} refs/remotes/capapp/other\n"
            )
            .into_bytes(),
        );
        files.loose_refs = vec![
            (
                "refs/remotes/capapp/production".to_string(),
                format!("{SHA_B}\n").into_bytes(),
            ),
            (
                "refs/heads/production".to_string(),
                format!("{SHA_B}\n").into_bytes(),
            ),
        ];
        let refs = HostGit::new()
            .parse_refs(&files)
            .await
            .expect("parse")
            .expect("some refs");
        assert_eq!(
            refs.head,
            GitHead::Branch {
                name: "production".to_string(),
                commit: Some(SHA_B.to_string())
            }
        );
        assert_eq!(refs.remotes["refs/remotes/capapp/production"], SHA_B);
        assert_eq!(refs.remotes["refs/remotes/capapp/other"], SHA_A);
        assert_eq!(refs.heads["refs/heads/production"], SHA_B);

        // An unborn branch, and a detached HEAD.
        let unborn = HostGit::new()
            .parse_refs(&files_with("ref: refs/heads/master\n"))
            .await
            .expect("parse")
            .expect("some");
        assert_eq!(
            unborn.head,
            GitHead::Branch {
                name: "master".to_string(),
                commit: None
            }
        );
        let detached = HostGit::new()
            .parse_refs(&files_with(&format!("{SHA_A}\n")))
            .await
            .expect("parse")
            .expect("some");
        assert_eq!(
            detached.head,
            GitHead::Detached {
                commit: SHA_A.to_string()
            }
        );
        // HEAD that is neither: no refs.
        assert_eq!(
            HostGit::new()
                .parse_refs(&files_with("garbage\n"))
                .await
                .expect("parse"),
            None
        );
    }

    #[tokio::test]
    async fn parse_refs_rejects_bad_name() {
        for bad in [
            "refs/heads/../x",
            "refs/heads/a..b",
            "/etc/passwd",
            "refs/heads/.hidden",
            "refs/heads/x.lock",
            "refs/heads/a b",
        ] {
            let mut files = files_with("ref: refs/heads/production\n");
            files.loose_refs = vec![(bad.to_string(), format!("{SHA_A}\n").into_bytes())];
            let err = HostGit::new()
                .parse_refs(&files)
                .await
                .expect_err("a bad ref name");
            assert_eq!(failed_check(&err), "git_host", "{bad}");
        }
        let mut files = files_with("ref: refs/heads/production\n");
        files.loose_refs = vec![(
            "refs/heads/production".to_string(),
            b"not a commit id\n".to_vec(),
        )];
        assert!(HostGit::new().parse_refs(&files).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn argv_never_holds_url() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = dir.path().join("argv.log");
        let recorder = stub(
            dir.path(),
            "git-recorder",
            &format!(
                "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\nexec git \"$@\"",
                log.display()
            ),
        );
        let git = HostGit::with_program(&recorder);

        git.parse_config(MODULE_CONFIG.as_bytes())
            .await
            .expect("config");
        let mut files = files_with("ref: refs/heads/production\n");
        files.loose_refs = vec![(
            "refs/heads/production".to_string(),
            format!("{SHA_A}\n").into_bytes(),
        )];
        git.parse_refs(&files).await.expect("refs");

        let (repo, commit) = repo_with(&[("a.txt", "a\n"), ("b/c.txt", "c\n")]);
        let pack = git
            .build_pack(repo.path(), &commit, &[])
            .await
            .expect("pack")
            .expect("some objects");
        assert!(!pack.pack.is_empty());
        let index = git
            .build_index(repo.path(), &commit, &["b/c.txt".to_string()])
            .await
            .expect("index");
        let work = tempfile::tempdir().expect("tempdir");
        std::fs::write(work.path().join("a.txt"), "a\n").expect("write");
        git.status_over(repo.path(), &index, work.path())
            .await
            .expect("status");

        let argv = std::fs::read_to_string(&log).expect("log");
        assert!(argv.lines().count() > 20, "the recorder saw the calls");
        for arg in argv.lines() {
            assert!(
                !arg.contains("://") && !arg.contains('@'),
                "a git argument holds a URL or userinfo: {arg}"
            );
        }
    }

    #[tokio::test]
    async fn status_over_lists_untracked() {
        let (repo, commit) = repo_with(&[
            ("a.txt", "a\n"),
            ("b.txt", "b\n"),
            ("dir/c.txt", "c\n"),
            ("dir/d.txt", "d\n"),
        ]);
        let git = HostGit::new();
        let index = git
            .build_index(repo.path(), &commit, &["dir/d.txt".to_string()])
            .await
            .expect("index");
        let work = tempfile::tempdir().expect("tempdir");
        std::fs::write(work.path().join("a.txt"), "changed\n").expect("write");
        std::fs::write(work.path().join("b.txt"), "b\n").expect("write");
        std::fs::create_dir(work.path().join("dir")).expect("mkdir");
        std::fs::write(work.path().join("dir/c.txt"), "c\n").expect("write");
        std::fs::write(work.path().join("new.txt"), "new\n").expect("write");
        std::fs::write(work.path().join("dir/ignored.log"), "x\n").expect("write");
        std::fs::write(work.path().join(".gitignore"), "*.log\n").expect("write");
        // dir/d.txt is assume-unchanged and absent: neither deleted nor modified.
        let status = git
            .status_over(repo.path(), &index, work.path())
            .await
            .expect("status");
        assert_eq!(status.modified, vec!["a.txt"]);
        assert_eq!(status.deleted, Vec::<String>::new());
        assert_eq!(status.untracked, vec![".gitignore", "new.txt"]);

        // Without the mark an absent path is deleted.
        let plain = git
            .build_index(repo.path(), &commit, &[])
            .await
            .expect("index");
        let status = git
            .status_over(repo.path(), &plain, work.path())
            .await
            .expect("status");
        assert_eq!(status.deleted, vec!["dir/d.txt"]);
    }

    #[tokio::test]
    async fn tree_blobs_and_commit_lookup() {
        let (repo, commit) = repo_with(&[("a.txt", "alpha\n"), ("dir/b.bin", "bee\0\n")]);
        let git = HostGit::new();
        assert_eq!(
            git.tree_paths(repo.path(), &commit).await.expect("tree"),
            vec!["a.txt", "dir/b.bin"]
        );
        let blobs = git
            .blobs(
                repo.path(),
                &commit,
                &["dir/b.bin".to_string(), "a.txt".to_string()],
            )
            .await
            .expect("blobs");
        assert_eq!(blobs["a.txt"], b"alpha\n");
        assert_eq!(blobs["dir/b.bin"], b"bee\0\n");
        assert!(
            git.blobs(repo.path(), &commit, &["nope.txt".to_string()])
                .await
                .is_err()
        );
        assert!(git.has_commit(repo.path(), &commit).await.expect("known"));
        assert!(!git.has_commit(repo.path(), SHA_A).await.expect("unknown"));
        assert!(git.has_commit(repo.path(), "not-an-id").await.is_err());
    }

    #[tokio::test]
    async fn pack_none_without_new_objects() {
        let (repo, commit) = repo_with(&[("a.txt", "a\n")]);
        let git = HostGit::new();
        assert!(
            git.build_pack(repo.path(), &commit, std::slice::from_ref(&commit))
                .await
                .expect("pack")
                .is_none()
        );
        let full = git
            .build_pack(repo.path(), &commit, &[])
            .await
            .expect("pack")
            .expect("objects");
        assert_eq!(full.name.len(), 40);
        assert_eq!(&full.idx[..4], &[0xff, b't', b'O', b'c']);
    }

    /// The raw bytes of `path` at `commit`, as git stores them.
    fn blob_bytes(repo: &Path, commit: &str, path: &str) -> Vec<u8> {
        let out = Command::new("git")
            .args(["cat-file", "blob", &format!("{commit}:{path}")])
            .current_dir(repo)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "cat-file {path} failed");
        out.stdout
    }

    /// A `git` that runs with `config` as its global config, as an operator's
    /// own configuration would apply.
    #[cfg(unix)]
    fn git_with_global(stubs: &Path, config: &Path) -> HostGit {
        let program = stub(
            stubs,
            "git-with-global",
            &format!("GIT_CONFIG_GLOBAL='{}' exec git \"$@\"", config.display()),
        );
        HostGit::with_program(program)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn materialize_is_raw_blobs_whatever_the_attributes_and_filters_say() {
        let marker_dir = tempfile::tempdir().expect("tempdir");
        let marker = marker_dir.path().join("filter-ran");
        let smudge = stub(
            marker_dir.path(),
            "smudge.sh",
            &format!("touch '{}'\ncat", marker.display()),
        );
        let config = marker_dir.path().join("operator-gitconfig");
        std::fs::write(
            &config,
            format!("[filter \"evil\"]\n\tsmudge = {}\n", smudge.display()),
        )
        .expect("write config");
        let attributes = "*.py text eol=crlf\n*.txt ident\n*.bin filter=evil\n\
                          *.enc working-tree-encoding=UTF-16LE\n";
        let files = [
            ("code.py", "x = 1\ny = 2\n"),
            ("sub/deep.py", "z = 3\n"),
            ("id.txt", "$Id$\nsecond line\n"),
            ("payload.bin", "payload\n"),
            ("text.enc", "plain ascii\n"),
        ];
        // The blobs go in before the attributes exist, so they are stored
        // exactly as written; the second commit adds the attributes.
        let (repo, _) = repo_with(&files);
        std::fs::write(repo.path().join(".gitattributes"), attributes).expect("attributes");
        git(repo.path(), &["add", ".gitattributes"]);
        git(
            repo.path(),
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "attrs"],
        );
        let commit = git(repo.path(), &["rev-parse", "HEAD"]);
        let host = git_with_global(marker_dir.path(), &config);
        let scratch = tempfile::tempdir().expect("tempdir");
        let bare = scratch.path().join("bare.git");
        git(
            scratch.path(),
            &[
                "clone",
                "-q",
                "--bare",
                &repo.path().display().to_string(),
                &bare.display().to_string(),
            ],
        );
        for (n, source) in [
            GitDir::Checkout(repo.path().to_path_buf()),
            GitDir::Bare(bare),
        ]
        .into_iter()
        .enumerate()
        {
            let out = scratch.path().join(format!("tree-{n}"));
            host.materialize(&source, &commit, &out)
                .await
                .expect("materialize");
            for path in files
                .iter()
                .map(|(path, _)| *path)
                .chain([".gitattributes"])
            {
                assert_eq!(
                    std::fs::read(out.join(path)).expect("read written file"),
                    blob_bytes(repo.path(), &commit, path),
                    "{path} is not the blob's bytes"
                );
            }
        }
        assert!(
            !marker.exists(),
            "a configured filter ran during materialize"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn materialize_keeps_modes_symlinks_and_gitlinks() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "production"]);
        std::fs::write(dir.path().join("plain.txt"), "plain\n").expect("write");
        std::fs::write(dir.path().join("run.sh"), "#!/bin/sh\n").expect("write");
        std::fs::set_permissions(
            dir.path().join("run.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
        std::os::unix::fs::symlink("plain.txt", dir.path().join("link")).expect("symlink");
        git(dir.path(), &["add", "-A"]);
        let module = "1".repeat(40);
        git(
            dir.path(),
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{module},vendor/module"),
            ],
        );
        git(
            dir.path(),
            &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "modes"],
        );
        let commit = git(dir.path(), &["rev-parse", "HEAD"]);
        let out = tempfile::tempdir().expect("tempdir");
        let target = out.path().join("tree");
        HostGit::new()
            .materialize(
                &GitDir::Checkout(dir.path().to_path_buf()),
                &commit,
                &target,
            )
            .await
            .expect("materialize");
        let mode = |name: &str| {
            std::fs::symlink_metadata(target.join(name))
                .expect("stat")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("run.sh"), 0o755);
        assert_eq!(mode("plain.txt"), 0o644);
        assert_eq!(
            std::fs::read_link(target.join("link")).expect("a symlink, not its target"),
            Path::new("plain.txt")
        );
        let gitlink = target.join("vendor/module");
        assert!(gitlink.is_dir(), "a gitlink is an empty directory");
        assert_eq!(std::fs::read_dir(gitlink).expect("read").count(), 0);
    }

    /// The id of the tree `entries` (`mode type oid\tname` lines) names, built
    /// with plumbing because porcelain refuses these names.
    fn tree_of(repo: &Path, entries: &str) -> String {
        use std::io::Write;
        let mut mktree = Command::new("git")
            .arg("mktree")
            .current_dir(repo)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("mktree starts");
        mktree
            .stdin
            .take()
            .expect("stdin")
            .write_all(entries.as_bytes())
            .expect("write entries");
        let out = mktree.wait_with_output().expect("mktree finishes");
        assert!(out.status.success(), "mktree failed");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn materialize_refuses_paths_that_leave_the_target() {
        let repo = tempfile::tempdir().expect("tempdir");
        git(repo.path(), &["init", "-q", "-b", "production"]);
        std::fs::write(repo.path().join("seed"), "escaped\n").expect("write");
        let blob = git(repo.path(), &["hash-object", "-w", "seed"]);
        let sub = tree_of(repo.path(), &format!("100644 blob {blob}\tx\n"));
        for name in ["..", ".git", ".GIT"] {
            let root = tree_of(repo.path(), &format!("040000 tree {sub}\t{name}\n"));
            let commit = git(repo.path(), &["commit-tree", &root, "-m", "hostile"]);
            let out = tempfile::tempdir().expect("tempdir");
            let target = out.path().join("inner");
            let err = HostGit::new()
                .materialize(
                    &GitDir::Checkout(repo.path().to_path_buf()),
                    &commit,
                    &target,
                )
                .await
                .expect_err(name);
            assert_eq!(failed_check(&err), "git_host", "{name}");
            assert!(!out.path().join("x").exists(), "{name} escaped the target");
            assert!(!target.join("x").exists(), "{name}");
        }
    }

    #[test]
    fn tree_paths_are_plain_relative_components() {
        for good in ["a", "a/b.txt", ".gitattributes", "a/.gitignore", "gitx/y"] {
            let path = tree_entry_path(good.as_bytes()).expect(good);
            assert!(path.is_relative(), "{good}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "a//b",
            "a/",
            ".",
            "./a",
            "..",
            "a/../b",
            "../a",
            ".git",
            ".GiT/config",
            "a/.git/hooks/x",
            "GIT~1/x",
            "a\0b",
        ] {
            assert!(tree_entry_path(bad.as_bytes()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn tree_listing_refuses_modes_it_cannot_write() {
        let oid = "a".repeat(40);
        let ok = format!("100644 blob {oid}\tf\0160000 commit {oid}\tg/h\0");
        assert_eq!(parse_tree_entries(ok.as_bytes()).expect("parse").len(), 2);
        for bad in [
            format!("100664 blob {oid}\tf\0"),
            format!("040000 tree {oid}\tf\0"),
            format!("100644 blob {oid} \tf\0"),
            format!("100644 blob {}\tf\0", "A".repeat(40)),
            format!("100644 blob {oid}\0"),
        ] {
            assert!(parse_tree_entries(bad.as_bytes()).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn scratch_dir_holds_only_what_ign_wrote_and_alternates_to_the_checkout() {
        let (checkout, commit) = repo_with(&[("a.txt", "a\n")]);
        let host = HostGit::new();
        let scratch = host.init_scratch(checkout.path()).await.expect("scratch");
        let git_dir = scratch.git_dir().to_path_buf();
        let alternates =
            std::fs::read_to_string(git_dir.join("objects/info/alternates")).expect("alternates");
        let expected = checkout.path().join(".git").join("objects");
        assert_eq!(
            Path::new(alternates.trim_end())
                .canonicalize()
                .expect("canonical"),
            expected.canonicalize().expect("canonical")
        );
        // No hooks, no templates, a config of one setting.
        assert!(!git_dir.join("hooks").exists());
        assert_eq!(
            std::fs::read_to_string(git_dir.join("config")).expect("config"),
            "[core]\n\trepositoryformatversion = 0\n"
        );
        // The commit is readable through the alternates and nothing is missing.
        assert!(
            host.missing_objects(&scratch, &commit)
                .await
                .expect("walk")
                .is_empty()
        );
        // An id the store does not hold is reported as the commit itself.
        let absent = "1".repeat(40);
        let missing = host.missing_objects(&scratch, &absent).await.expect("walk");
        assert_eq!(missing.into_iter().collect::<Vec<_>>(), vec![absent]);
        // Removed with the value.
        let root = scratch.root().to_path_buf();
        drop(scratch);
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn missing_objects_names_a_tree_that_is_not_in_either_store() {
        let (gateway, commit) = repo_with(&[("a.txt", "one\n"), ("d/b.txt", "two\n")]);
        let (empty, _) = repo_with(&[("z.txt", "z\n")]);
        let host = HostGit::new();
        let scratch = host.init_scratch(empty.path()).await.expect("scratch");
        // Copy only the commit object (loose) into the scratch: its tree and
        // blobs are missing.
        let loose = |id: &str| format!("objects/{}/{}", &id[..2], &id[2..]);
        let src = gateway.path().join(".git").join(loose(&commit));
        let dst = scratch.git_dir().join(loose(&commit));
        std::fs::create_dir_all(dst.parent().expect("parent")).expect("mkdir");
        std::fs::copy(&src, &dst).expect("copy");
        let tree = git(
            gateway.path(),
            &["rev-parse", &format!("{commit}^{{tree}}")],
        );
        let missing = host.missing_objects(&scratch, &commit).await.expect("walk");
        assert_eq!(missing.into_iter().collect::<Vec<_>>(), vec![tree]);
    }

    #[tokio::test]
    async fn pack_ids_lists_the_objects_of_an_index() {
        let (repo, commit) = repo_with(&[("a.txt", "one\n")]);
        git(repo.path(), &["gc", "-q"]);
        let pack_dir = repo.path().join(".git/objects/pack");
        let idx = std::fs::read_dir(&pack_dir)
            .expect("pack dir")
            .map(|e| e.expect("entry").path())
            .find(|p| p.extension().is_some_and(|e| e == "idx"))
            .expect("an idx");
        let host = HostGit::new();
        let scratch = host.init_scratch(repo.path()).await.expect("scratch");
        let ids = host.pack_ids(&scratch, &idx).await.expect("ids");
        assert!(ids.contains(&commit), "{ids:?}");
        assert!(ids.iter().all(|id| is_object_id(id)));
        // Not an index: refused, never an empty list.
        let junk = scratch.root().join("junk.idx");
        std::fs::write(&junk, b"not an index").expect("junk");
        assert!(host.pack_ids(&scratch, &junk).await.is_err());
    }

    // ---- 27-20: the remote token, scoped to the matched remote's host ----

    const TOKEN: &str = "tok-7f3a91c2-not-a-real-token";

    fn env_map(pairs: Vec<(String, String)>) -> BTreeMap<String, String> {
        pairs.into_iter().collect()
    }

    fn basic(token: &str) -> String {
        format!(
            "AUTHORIZATION: basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"))
        )
    }

    #[test]
    fn remote_auth_scoped_to_remote_host() {
        let env = env_map(
            remote_auth_env("https://github.com/org/capapp.git", Some(TOKEN), None)
                .expect("an https remote"),
        );
        assert_eq!(env.len(), 3, "{env:?}");
        assert_eq!(env["GIT_CONFIG_COUNT"], "1");
        assert_eq!(
            env["GIT_CONFIG_KEY_0"],
            "http.https://github.com/.extraheader"
        );
        assert_eq!(env["GIT_CONFIG_VALUE_0"], basic(TOKEN));

        // Userinfo is dropped; the port stays; the host is lowercased.
        let env = env_map(
            remote_auth_env(
                "https://user:pw@Git.Example.Test:8443/org/capapp.git",
                Some(TOKEN),
                None,
            )
            .expect("an https remote"),
        );
        assert_eq!(
            env["GIT_CONFIG_KEY_0"],
            "http.https://git.example.test:8443/.extraheader"
        );
        let text = format!("{env:?}");
        assert!(!text.contains("pw") && !text.contains("user"), "{text}");

        // The entries already in the environment are kept.
        let env = env_map(
            remote_auth_env("https://git.example.test/o/r", Some(TOKEN), Some(2))
                .expect("an https remote"),
        );
        assert_eq!(env["GIT_CONFIG_COUNT"], "3");
        assert_eq!(
            env["GIT_CONFIG_KEY_2"],
            "http.https://git.example.test/.extraheader"
        );
        assert_eq!(env["GIT_CONFIG_VALUE_2"], basic(TOKEN));
        assert!(!env.contains_key("GIT_CONFIG_KEY_0"), "{env:?}");

        // A loopback rig over plain http is scoped the same way.
        let env = env_map(
            remote_auth_env("http://127.0.0.1:18292/capapp.git", Some(TOKEN), None)
                .expect("loopback"),
        );
        assert_eq!(
            env["GIT_CONFIG_KEY_0"],
            "http.http://127.0.0.1:18292/.extraheader"
        );
    }

    #[test]
    fn remote_auth_refuses_ssh_with_token() {
        let want = "a git token applies only to https remotes; this remote uses ssh: give the \
                    machine running ign ssh access to it and leave git-token empty";
        for url in [
            "git@git.example.test:org/capapp.git",
            "git.example.test:org/capapp.git",
            "ssh://git@git.example.test/org/capapp.git",
            "SSH://git.example.test:2222/org/capapp.git",
        ] {
            let err = remote_auth_env(url, Some(TOKEN), None).expect_err(url);
            assert_eq!(err, want, "{url}");
            assert!(!err.contains(TOKEN) && !err.contains("example"), "{err}");
        }
        // Another transport names itself.
        let err = remote_auth_env("git://git.example.test/org/capapp.git", Some(TOKEN), None)
            .expect_err("git protocol");
        assert!(err.contains("this remote uses git:"), "{err}");
        let err = remote_auth_env("ext::sh -c evil", Some(TOKEN), None).expect_err("a helper");
        assert!(err.contains("this remote uses ext:"), "{err}");
    }

    #[test]
    fn remote_auth_refuses_plain_http_remote_host() {
        for url in [
            "http://git.example.test/org/capapp.git",
            "http://127.0.0.1.example.test/capapp.git",
            "http://10.0.0.5:8080/capapp.git",
        ] {
            let err = remote_auth_env(url, Some(TOKEN), None).expect_err(url);
            assert!(
                err.contains("plain http to a host that is not a loopback address"),
                "{err}"
            );
            assert!(!err.contains(TOKEN) && !err.contains("example"), "{err}");
        }
        for url in [
            "http://127.0.0.1:18292/capapp.git",
            "http://127.1.2.3/capapp.git",
            "http://localhost:18292/capapp.git",
            "http://[::1]:18292/capapp.git",
        ] {
            let env = env_map(remote_auth_env(url, Some(TOKEN), None).expect(url));
            assert!(
                env["GIT_CONFIG_KEY_0"].starts_with("http.http://"),
                "{env:?}"
            );
        }
    }

    #[test]
    fn remote_auth_refuses_a_host_that_is_not_one() {
        for url in [
            "https:///org/capapp.git",
            "https://@/org/capapp.git",
            "https://bad host/org/capapp.git",
            "https://host:port/org/capapp.git",
            "https://host:123456789/org/capapp.git",
            "https://host\n[core]/org/capapp.git",
        ] {
            let err = remote_auth_env(url, Some(TOKEN), None).expect_err(url);
            assert!(
                err.contains("no host a git token can be scoped to"),
                "{err}"
            );
        }
    }

    #[test]
    fn remote_auth_none_without_token() {
        for token in [None, Some("")] {
            for url in [
                "https://github.com/org/capapp.git",
                "http://git.example.test/org/capapp.git",
                "git@git.example.test:org/capapp.git",
                "ssh://git@git.example.test/org/capapp.git",
                "file:///srv/git/capapp.git",
                "/srv/git/capapp.git",
            ] {
                assert_eq!(
                    remote_auth_env(url, token, Some(3)),
                    Ok(Vec::new()),
                    "{url}"
                );
            }
        }
        // `file://` and a path ignore a token that is set.
        for url in [
            "file:///srv/git/capapp.git",
            "/srv/git/capapp.git",
            "../capapp.git",
            "C:/repos/capapp.git",
        ] {
            assert_eq!(
                remote_auth_env(url, Some(TOKEN), None),
                Ok(Vec::new()),
                "{url}"
            );
        }
    }

    #[test]
    fn the_token_is_scrubbed_from_every_child() {
        assert!(SCRUBBED_ENV.contains(&GIT_TOKEN_ENV));
        assert_eq!(GIT_TOKEN_ENV, "IGNITION_GIT_TOKEN");
    }

    #[test]
    fn debug_hides_the_token() {
        let set = HostGit::with_remote_token(Some(TOKEN.to_string()));
        let unset = HostGit::with_remote_token(Some(String::new()));
        let text = format!("{set:?} {set:#?}");
        assert!(text.contains("remote_token: <set>"), "{text}");
        assert!(!text.contains(TOKEN) && !text.contains("tok-"), "{text}");
        assert!(format!("{unset:?}").contains("remote_token: <unset>"));
        assert!(format!("{:?}", HostGit::with_program("git")).contains("<unset>"));
    }

    /// A stub that logs argv and `GIT_CONFIG_*`, runs the two local queries
    /// with the real git and answers the two remote reads with nothing.
    #[cfg(unix)]
    fn recording_git(dir: &Path) -> (PathBuf, PathBuf) {
        let log = dir.join("calls.log");
        let body = r#"{ printf 'ARGV:'; for a in "$@"; do printf ' [%s]' "$a"; done; printf '\n'; env | grep '^GIT_CONFIG_'; echo END; } >> 'LOG'
case " $* " in
  *" --get-url "*|*" config "*) exec git "$@" ;;
esac
exit 0"#
            .replace("LOG", &log.display().to_string());
        (stub(dir, "recording-git", &body), log)
    }

    /// The token reaches the two remote reads in their environment, scoped to
    /// the remote's host, and nowhere else: no argument, no other call, no
    /// `Debug`, no error.
    #[cfg(unix)]
    #[tokio::test]
    async fn token_never_on_argv_or_debug() {
        let work = tempfile::tempdir().expect("tempdir");
        let repo = work.path().join("checkout");
        std::fs::create_dir(&repo).expect("mkdir");
        git(&repo, &["init", "-q", "-b", "production"]);
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://deploy:hunter2@git.example.test:8443/org/capapp.git",
            ],
        );
        let (program, log) = recording_git(work.path());
        let runner = HostGit::with_program(&program).and_remote_token(Some(TOKEN.to_string()));

        assert!(
            runner
                .ls_remote_heads(&repo, "origin")
                .await
                .expect("ls")
                .is_empty()
        );
        runner
            .fetch_heads(&repo, "origin", &["production".to_string()])
            .await
            .expect("fetch");

        let calls = std::fs::read_to_string(&log).expect("the log");
        let blocks: Vec<&str> = calls.split("END\n").filter(|b| !b.is_empty()).collect();
        let n = std::env::var("GIT_CONFIG_COUNT")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let b64 =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{TOKEN}"));
        let mut remote_reads = 0;
        for block in &blocks {
            for secret in [TOKEN, b64.as_str(), "hunter2", "deploy:"] {
                let argv = block.lines().next().expect("argv line");
                assert!(!argv.contains(secret), "{argv}");
            }
            let is_remote_read = block.starts_with("ARGV:")
                && (block
                    .lines()
                    .next()
                    .is_some_and(|l| l.contains("[ls-remote] [--heads]") || l.contains("[fetch]")));
            if is_remote_read {
                remote_reads += 1;
                assert!(
                    block.contains(&format!(
                        "GIT_CONFIG_KEY_{n}=http.https://git.example.test:8443/.extraheader"
                    )),
                    "{block}"
                );
                assert!(
                    block.contains(&format!("GIT_CONFIG_VALUE_{n}=AUTHORIZATION: basic {b64}")),
                    "{block}"
                );
                assert!(
                    block.contains(&format!("GIT_CONFIG_COUNT={}", n + 1)),
                    "{block}"
                );
                assert!(
                    block.contains("[-c] [http.followRedirects=false]"),
                    "{block}"
                );
            } else {
                // The local queries get no token entry at all.
                assert!(!block.contains(&b64), "{block}");
                assert!(
                    !block.contains("extraheader"),
                    "the token reached a local query: {block}"
                );
            }
        }
        assert_eq!(remote_reads, 2, "{calls}");

        // The refusal for an ssh remote holds nothing of the token or the URL.
        git(
            &repo,
            &[
                "remote",
                "add",
                "other",
                "git@git.example.test:org/other.git",
            ],
        );
        let err = runner
            .ls_remote_heads(&repo, "other")
            .await
            .expect_err("ssh with a token");
        let text = format!("{err} {err:?} {runner:?}");
        assert!(!text.contains(TOKEN) && !text.contains(&b64), "{text}");
        assert!(text.contains("this remote uses ssh"), "{text}");
        assert_eq!(failed_check(&err), "git_host");
        // And no remote read ran for it.
        let after = std::fs::read_to_string(&log).expect("the log");
        assert_eq!(
            after.matches("[ls-remote] [--heads]").count(),
            1,
            "the refused remote was never read"
        );
    }

    /// Without a token no extra process runs: the 27-05 calls are exactly
    /// the same, an ssh remote is still read under `BatchMode`, and a token
    /// with a `file://` remote changes nothing.
    #[cfg(unix)]
    #[tokio::test]
    async fn no_token_means_the_calls_are_unchanged() {
        let work = tempfile::tempdir().expect("tempdir");
        let repo = work.path().join("checkout");
        std::fs::create_dir(&repo).expect("mkdir");
        git(&repo, &["init", "-q", "-b", "production"]);
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "git@git.example.test:org/capapp.git",
            ],
        );
        let (program, log) = recording_git(work.path());
        let runner = HostGit::with_program(&program);
        runner.ls_remote_heads(&repo, "origin").await.expect("ls");
        let calls = std::fs::read_to_string(&log).expect("the log");
        assert!(!calls.contains("--get-url"), "{calls}");
        assert!(!calls.contains("followRedirects"), "{calls}");
        assert!(
            calls.contains("core.sshCommand=ssh -o BatchMode=yes"),
            "{calls}"
        );

        let file_remote = work.path().join("remote.git");
        git(work.path(), &["init", "-q", "--bare", "remote.git"]);
        git(
            &repo,
            &[
                "remote",
                "add",
                "local",
                &format!("file://{}", file_remote.display()),
            ],
        );
        let tokened = HostGit::with_program(&program).and_remote_token(Some(TOKEN.to_string()));
        tokened
            .ls_remote_heads(&repo, "local")
            .await
            .expect("file remote");
        let calls = std::fs::read_to_string(&log).expect("the log");
        assert!(!calls.contains("extraheader"), "{calls}");
        assert!(!calls.contains("followRedirects"), "{calls}");
    }

    /// A child that prints the token, in either form, into its error output
    /// does not get it into the error text.
    #[cfg(unix)]
    #[tokio::test]
    async fn token_is_scrubbed_from_git_errors() {
        let work = tempfile::tempdir().expect("tempdir");
        let repo = work.path().join("checkout");
        std::fs::create_dir(&repo).expect("mkdir");
        git(&repo, &["init", "-q", "-b", "production"]);
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://git.example.test/org/capapp.git",
            ],
        );
        let program = stub(
            work.path(),
            "leaky-git",
            &format!(
                "case \" $* \" in *\" --get-url \"*|*\" config \"*) exec git \"$@\";; esac\n\
                 echo \"fatal: bad credentials {TOKEN} / $GIT_CONFIG_VALUE_0\" >&2\nexit 128"
            ),
        );
        let runner = HostGit::with_program(&program).and_remote_token(Some(TOKEN.to_string()));
        let err = runner
            .ls_remote_heads(&repo, "origin")
            .await
            .expect_err("the stub fails");
        let b64 =
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{TOKEN}"));
        let text = format!("{err} {err:?}");
        assert!(text.contains("bad credentials"), "{text}");
        assert!(!text.contains(TOKEN) && !text.contains(&b64), "{text}");
    }
}
