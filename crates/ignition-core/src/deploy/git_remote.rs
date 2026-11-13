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
//! `git config --file <tmp> --no-includes` under an empty global config and no
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

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::git_state::{GIT_SMALL_FILE_MAX, redact_text};
use super::git_state::{GitBranchConfig, GitConfig, GitFiles, GitHead, GitRefs, GitRemoteConfig};
use crate::error::{CoreError, PreflightFailure};

/// The oldest git ign drives (the floor for `fetch --no-write-fetch-head`).
pub const GIT_MIN_VERSION: (u32, u32) = (2, 29);

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

/// The host-git subprocess runner.
#[derive(Debug, Clone)]
pub struct HostGit {
    program: OsString,
}

impl Default for HostGit {
    fn default() -> Self {
        HostGit::new()
    }
}

impl HostGit {
    /// The runner for `git` on `PATH`.
    pub fn new() -> HostGit {
        HostGit {
            program: OsString::from("git"),
        }
    }

    /// The runner for a named program (the test seam, as
    /// `ignore_status_with`).
    pub fn with_program(program: impl AsRef<OsStr>) -> HostGit {
        HostGit {
            program: program.as_ref().to_os_string(),
        }
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
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_NAMESPACE",
            "GIT_COMMON_DIR",
            "GIT_ASKPASS",
            "SSH_ASKPASS",
        ] {
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
            Ok(done) => done,
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
        match var {
            "remote" if branch.remote.is_none() => branch.remote = Some(value),
            "merge" if branch.merge.is_none() => branch.merge = Some(value),
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
}
