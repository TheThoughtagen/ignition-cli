//! The remote shell under the ssh transport (22-05).
//!
//! [`RemoteShell`] turns one `nodes.*.ssh` target block into hardened `ssh`
//! and `sftp` argv vectors and runs them only through an
//! `Arc<dyn ProcessRunner>`:
//!
//! - **Argv only.** No shell string is ever built locally. `host` and `user`
//!   are validated by the target schema and every argv puts `--` before
//!   `user@host`, so no target value can be read as an ssh option (T-22-17).
//! - **Pinned host key (D-01).** The repo's reviewed `known_hosts` is read
//!   through the symlink-refusing repo reader and copied into the shell's own
//!   0700 temp dir; ssh gets that copy as `UserKnownHostsFile` with
//!   `StrictHostKeyChecking=yes`, and the runner's own files are never read.
//! - **Key handling (D-03).** `key_env` names a variable holding the key
//!   text; it is normalised, written 0600 into the temp dir, passed as
//!   `-i <file>` with `IdentitiesOnly=yes`, and deleted with the temp dir on
//!   drop. Only the path ever appears in argv. The key text is not retained.
//! - **Bounded in time (D-04).** `ConnectTimeout` plus `ServerAlive*` inside
//!   ssh, and a per-call timeout with kill in the process runner.
//! - **One temp dir per shell**, so two shells never share a file.
//! - **Typed remote input.** The remote shell only receives a
//!   [`RemoteCommand`] (every constructor yields an injection-safe string) and
//!   sftp only receives [`SftpOp`] values (every path checked and
//!   double-quoted), never a caller-built string. A raw-line runner exists
//!   under `cfg(test)` only.
//!
//! The exact `-o` order is [`hardened_options`]; `ssh -G` on the generated
//! argv is pinned by `tests/deploy_transport.rs`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;

use super::process::{ProcessOutput, ProcessRunner};
use super::winps::{self, COMMAND_BUDGET, WinScript};
use crate::deploy::target::{DEFAULT_CONNECT_TIMEOUT_S, DEFAULT_OP_TIMEOUT_S, Ssh};
use crate::deploy::tree::read_repo_file;
use crate::error::CoreError;

/// Lines of ssh or sftp stderr kept in an error detail.
pub const STDERR_TAIL_LINES: usize = 5;

/// An error detail never exceeds this many bytes.
const DETAIL_MAX_BYTES: usize = 1024;

/// Why `path` cannot be placed in an sftp batch line, or `Ok` when it can.
///
/// Remote paths reach sftp inside double quotes on a batch line, and the sftp
/// client globs what it is given, so these are refused outright:
///
/// | Refused | Why |
/// |---|---|
/// | empty | names the working directory, never a resource |
/// | control characters (newline, CR, tab, DEL, C1) | a newline ends the batch line and starts another command |
/// | `"` | closes the quoting |
/// | `\` | escapes inside the quoting (and is a separator on Windows hosts, normalised before this check) |
/// | `*` `?` `[` `]` `{` `}` | sftp glob characters: `ls "dir/odd[1]"` only worked because no `odd1` existed, a sibling would silently win |
///
/// Spaces, commas and other printable characters pass.
pub fn safe_remote_path(path: &str) -> Result<(), String> {
    refuse_unsafe_path("remote", path)
}

/// The shared character check behind [`safe_remote_path`] and
/// [`safe_local_path`]; `kind` names the side in the error.
fn refuse_unsafe_path(kind: &str, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err(format!("{kind} path must not be empty"));
    }
    for c in path.chars() {
        let why = match c {
            c if c.is_control() => "a control character",
            '"' => "a double quote",
            '\\' => "a backslash",
            '*' | '?' | '[' | ']' | '{' | '}' => "an sftp glob character",
            _ => continue,
        };
        return Err(format!(
            "{kind} path {path:?} contains {why} ({c:?}), which cannot be placed safely in an sftp batch line"
        ));
    }
    Ok(())
}

/// [`safe_remote_path`] for a path on the machine running `ign`, as it
/// appears in an sftp `get` or `put`: the same refusals (empty, control
/// characters, `"`, `\`, glob characters, because the sftp client globs local
/// paths too), plus a leading `-`, which the sftp client would read as an
/// option (`get "-R" "x"`) even though it was quoted. On a Windows runner
/// backslashes are normalised to `/` by the caller before this check.
pub fn safe_local_path(path: &str) -> Result<(), String> {
    refuse_unsafe_path("local", path)?;
    refuse_leading_dash("local", path)
}

fn refuse_leading_dash(kind: &str, path: &str) -> Result<(), String> {
    if path.starts_with('-') {
        return Err(format!(
            "{kind} path {path:?} starts with '-', which the sftp client would read as an option"
        ));
    }
    Ok(())
}

/// Drop ANSI CSI sequences and every control character except `\n`.
fn strip_controls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // Parameter and intermediate bytes, then one final byte.
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        if c == '\n' || !c.is_control() {
            out.push(c);
        }
    }
    out
}

/// A bounded, scrubbed tail of `stderr` fit for an error detail.
///
/// Lossy UTF-8; ANSI sequences and control characters (newline excepted) are
/// removed; the native and forward-slash spellings of `temp_dir` become
/// `<tmp>`; only the last [`STDERR_TAIL_LINES`] lines are kept, and the
/// result is at most 1 KiB (cut from the front on a character boundary).
pub fn scrub_stderr(stderr: &[u8], temp_dir: Option<&Path>) -> String {
    let mut text = strip_controls(&String::from_utf8_lossy(stderr));
    if let Some(dir) = temp_dir {
        let native = dir.display().to_string();
        let slashed = native.replace('\\', "/");
        if !native.is_empty() {
            text = text.replace(&native, "<tmp>");
        }
        if !slashed.is_empty() && slashed != native {
            text = text.replace(&slashed, "<tmp>");
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(STDERR_TAIL_LINES);
    let mut out = lines[from..].join("\n");
    if out.len() > DETAIL_MAX_BYTES {
        let mut cut = out.len() - DETAIL_MAX_BYTES;
        while !out.is_char_boundary(cut) {
            cut += 1;
        }
        out.drain(..cut);
    }
    out
}

/// The null device ssh is pointed at for files it must not consult.
#[cfg(windows)]
pub const NULL_DEVICE: &str = "NUL";
/// The null device ssh is pointed at for files it must not consult.
#[cfg(not(windows))]
pub const NULL_DEVICE: &str = "/dev/null";

/// Normalise private key text for OpenSSH: CRLF and lone CR become LF and
/// exactly one trailing LF is kept. OpenSSH 10 answers `invalid format` for a
/// key with no trailing newline and for CRLF, which a CI secret store
/// produces easily.
pub fn normalize_private_key(text: &str) -> String {
    let mut out = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = out.trim_end_matches('\n').len();
    out.truncate(trimmed);
    out.push('\n');
    out
}

/// A path as an ssh `-o` value: quoted when it holds whitespace, `%` doubled
/// (ssh expands `%h`-style tokens in file options), forward slashes on a
/// Windows runner. `"`, `$` (environment expansion) and control characters
/// are refused.
fn option_path(path: &Path) -> Result<String, String> {
    let text = path
        .to_str()
        .ok_or_else(|| "a path that is not valid UTF-8 cannot be passed to ssh".to_string())?;
    if let Some(bad) = text
        .chars()
        .find(|c| *c == '"' || *c == '$' || c.is_control())
    {
        return Err(format!(
            "path {text:?} contains {bad:?}, which cannot be passed safely as an ssh option"
        ));
    }
    let mut value = if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.to_string()
    };
    value = value.replace('%', "%%");
    if value.chars().any(char::is_whitespace) {
        value = format!("\"{value}\"");
    }
    Ok(value)
}

/// The security-relevant `-o key=value` pairs, in the fixed order every ssh
/// and sftp argv uses:
///
/// 1. `BatchMode=yes`
/// 2. `StrictHostKeyChecking=yes`
/// 3. `UserKnownHostsFile=<known_hosts>`
/// 4. `GlobalKnownHostsFile=<null device>`
/// 5. `UpdateHostKeys=no`
/// 6. `VerifyHostKeyDNS=no`
/// 7. `PreferredAuthentications=publickey`
/// 8. `PasswordAuthentication=no`
/// 9. `KbdInteractiveAuthentication=no`
/// 10. `NumberOfPasswordPrompts=0`
/// 11. `IdentitiesOnly=yes` (only when `identity` is given)
/// 12. `ForwardAgent=no`
/// 13. `ForwardX11=no`
/// 14. `ClearAllForwardings=yes`
/// 15. `PermitLocalCommand=no`
/// 16. `ControlMaster=no`
/// 17. `ControlPath=none`
/// 18. `ConnectTimeout=<connect_timeout_s>`
/// 19. `ServerAliveInterval=5`
/// 20. `ServerAliveCountMax=3`
/// 21. `LogLevel=ERROR`
///
/// Command-line `-o` wins over `~/.ssh/config`, so a user's agent and
/// `ProxyJump` still work while none of these can be weakened by it. A path
/// that cannot be passed safely (see [`option_path`]) is an error.
pub fn hardened_options(
    known_hosts: &Path,
    identity: Option<&Path>,
    connect_timeout_s: u32,
) -> Result<Vec<(String, String)>, String> {
    let pair = |k: &str, v: &str| (k.to_string(), v.to_string());
    let mut options = vec![
        pair("BatchMode", "yes"),
        pair("StrictHostKeyChecking", "yes"),
        ("UserKnownHostsFile".to_string(), option_path(known_hosts)?),
        pair("GlobalKnownHostsFile", NULL_DEVICE),
        pair("UpdateHostKeys", "no"),
        pair("VerifyHostKeyDNS", "no"),
        pair("PreferredAuthentications", "publickey"),
        pair("PasswordAuthentication", "no"),
        pair("KbdInteractiveAuthentication", "no"),
        pair("NumberOfPasswordPrompts", "0"),
    ];
    if let Some(identity) = identity {
        // Validate the identity path like any other path option.
        option_path(identity)?;
        options.push(pair("IdentitiesOnly", "yes"));
    }
    options.extend([
        pair("ForwardAgent", "no"),
        pair("ForwardX11", "no"),
        pair("ClearAllForwardings", "yes"),
        pair("PermitLocalCommand", "no"),
        pair("ControlMaster", "no"),
        pair("ControlPath", "none"),
        ("ConnectTimeout".to_string(), connect_timeout_s.to_string()),
        pair("ServerAliveInterval", "5"),
        pair("ServerAliveCountMax", "3"),
        pair("LogLevel", "ERROR"),
    ]);
    Ok(options)
}

fn setup_failed(label: &str, detail: impl Into<String>) -> CoreError {
    CoreError::TransportFailed {
        operation: "ssh-setup".to_string(),
        node: label.to_string(),
        detail: detail.into(),
    }
}

/// Create `path` exclusively with owner-only permissions and write `bytes`.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

/// Restrict `file` to the current user on a Windows runner.
#[cfg(windows)]
fn restrict_to_owner(file: &Path, label: &str) -> Result<(), CoreError> {
    let user = std::env::var("USERNAME")
        .map_err(|_| setup_failed(label, "USERNAME is not set; cannot restrict the key file"))?;
    let status = std::process::Command::new("icacls")
        .arg(file)
        .args(["/inheritance:r", "/grant:r"])
        .arg(format!("{user}:R"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| setup_failed(label, format!("could not run icacls ({:?})", e.kind())))?;
    if status.success() {
        Ok(())
    } else {
        Err(setup_failed(
            label,
            "icacls could not restrict the key file to the current user",
        ))
    }
}

/// A command string for the remote shell whose only constructors produce
/// injection-safe text. `ssh` hands its trailing argument to the remote login
/// shell, so a caller-built string would be a shell-injection sink; this type
/// is the only thing [`RemoteShell::ssh_argv`] accepts. The inner string is
/// private.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCommand(String);

impl RemoteCommand {
    /// `powershell -NoProfile -NonInteractive -EncodedCommand <base64>`, where
    /// the payload is `script` as UTF-16LE. Base64 (`A-Za-z0-9+/=`) has no
    /// character any shell, `cmd.exe` or PowerShell treats specially, so the
    /// script cannot break out whatever the host's default shell is.
    pub fn powershell_encoded(script: &str) -> RemoteCommand {
        let utf16le: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(utf16le);
        RemoteCommand(format!(
            "powershell -NoProfile -NonInteractive -EncodedCommand {encoded}"
        ))
    }

    /// A POSIX `sh` command line for `argv`: each argument single-quoted with
    /// every `'` written `'\''`, joined by spaces. Inside single quotes a POSIX
    /// shell interprets nothing, so `$(..)`, backticks, `;`, `!`, `*` and
    /// the rest reach the program as literal text. Refused: an empty `argv`,
    /// and NUL or any other control character in any argument (a newline or
    /// tab would otherwise survive quoting and still be a hazard to every log
    /// and terminal it reaches).
    pub fn posix(argv: &[&str]) -> Result<RemoteCommand, String> {
        if argv.is_empty() {
            return Err("a remote command needs at least a program".to_string());
        }
        let mut quoted = Vec::with_capacity(argv.len());
        for arg in argv {
            if let Some(bad) = arg.chars().find(|c| c.is_control()) {
                return Err(format!(
                    "remote command argument {arg:?} contains the control character {bad:?}"
                ));
            }
            quoted.push(format!("'{}'", arg.replace('\'', "'\\''")));
        }
        Ok(RemoteCommand(quoted.join(" ")))
    }

    /// The command text, exactly as it is sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One sftp batch command. [`RemoteShell::sftp_batch`] accepts only these, so
/// a caller cannot hand sftp a raw line: in batch mode a leading `!` runs a
/// LOCAL shell command and a leading `-` or `@` changes error handling.
///
/// Each op renders to `<command> "<path>"…` with every remote path through
/// [`safe_remote_path`] and every local path through [`safe_local_path`]
/// (both also refuse a leading `-`). Only the listing ops carry the `@`
/// quiet prefix, which hides the echoed command so stdout is the listing; no
/// op ever starts with `-`, so a failure is never suppressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SftpOp {
    /// `@ls -1 "<path>"`: names only, one per line.
    Ls { path: String },
    /// `@ls -l "<path>"`: the long listing.
    LsLong { path: String },
    /// `@ls -la "<path>"`: the long listing including dot-named entries and
    /// the `.` / `..` pair. The read transport lists with this so that no
    /// entry is hidden from it and a symlink or special file is visible.
    LsLongAll { path: String },
    /// `@pwd`: prints `Remote working directory: <absolute path>`, the
    /// account's home at the start of a session. A Windows OpenSSH server
    /// prints long-listing names WITHOUT the listed directory in front of them
    /// (C7a-2), so a batch that lists several directories puts a `pwd` before
    /// each listing as the marker that says where one directory's lines start.
    Pwd,
    /// `get "<remote>" "<local>"`.
    Get { remote: String, local: PathBuf },
    /// `get -R "<remote>" "<local>"`: a directory tree; symlinks are skipped
    /// by the client.
    GetRecursive { remote: String, local: PathBuf },
    /// `put "<local>" "<remote>"`.
    Put { local: PathBuf, remote: String },
    /// `rename "<from>" "<to>"`.
    Rename { from: String, to: String },
    /// `rm "<path>"`.
    Rm { path: String },
    /// `mkdir "<path>"`.
    Mkdir { path: String },
    /// `rmdir "<path>"`.
    Rmdir { path: String },
}

impl SftpOp {
    /// The batch line for this op, or why a path cannot be placed in one.
    pub fn render(&self) -> Result<String, String> {
        let remote = |path: &str| -> Result<String, String> {
            safe_remote_path(path)?;
            refuse_leading_dash("remote", path)?;
            Ok(format!("\"{path}\""))
        };
        let local = |path: &Path| -> Result<String, String> {
            let text = path
                .to_str()
                .ok_or_else(|| "a local path that is not valid UTF-8 cannot be used".to_string())?;
            let text = if cfg!(windows) {
                text.replace('\\', "/")
            } else {
                text.to_string()
            };
            safe_local_path(&text)?;
            Ok(format!("\"{text}\""))
        };
        Ok(match self {
            SftpOp::Ls { path } => format!("@ls -1 {}", remote(path)?),
            SftpOp::LsLong { path } => format!("@ls -l {}", remote(path)?),
            SftpOp::LsLongAll { path } => format!("@ls -la {}", remote(path)?),
            SftpOp::Pwd => "@pwd".to_string(),
            SftpOp::Get {
                remote: r,
                local: l,
            } => format!("get {} {}", remote(r)?, local(l)?),
            SftpOp::GetRecursive {
                remote: r,
                local: l,
            } => {
                format!("get -R {} {}", remote(r)?, local(l)?)
            }
            SftpOp::Put {
                local: l,
                remote: r,
            } => format!("put {} {}", local(l)?, remote(r)?),
            SftpOp::Rename { from, to } => {
                // A rename onto itself DELETES the file on a Windows OpenSSH
                // server (C7a-4): the source is removed and the target, the
                // same path, is gone. Refuse it for every host.
                if same_remote_path(from, to) {
                    return Err(format!(
                        "refused a rename of {from:?} onto itself (on a Windows OpenSSH host that deletes the file)"
                    ));
                }
                format!("rename {} {}", remote(from)?, remote(to)?)
            }
            SftpOp::Rm { path } => format!("rm {}", remote(path)?),
            SftpOp::Mkdir { path } => format!("mkdir {}", remote(path)?),
            SftpOp::Rmdir { path } => format!("rmdir {}", remote(path)?),
        })
    }
}

/// Whether two remote paths name the same entry as far as a Windows host is
/// concerned: equal after ASCII-lowercasing (NTFS is case-insensitive) and
/// collapsing repeated `/`, ignoring a trailing `/`. Over-matches on a
/// case-sensitive host, which is the safe direction for a refusal.
pub(crate) fn same_remote_path(a: &str, b: &str) -> bool {
    fn fold(path: &str) -> String {
        let mut out = String::with_capacity(path.len());
        let mut last_slash = false;
        for c in path.chars() {
            let c = c.to_ascii_lowercase();
            if c == '/' {
                if last_slash {
                    continue;
                }
                last_slash = true;
            } else {
                last_slash = false;
            }
            out.push(c);
        }
        while out.len() > 1 && out.ends_with('/') {
            out.pop();
        }
        out
    }
    fold(a) == fold(b)
}

/// What an sftp failure said, read from stderr text because every failure
/// exits 1 (C7a-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpFailure {
    /// "not found" / "No such file". On a Windows host a read the ACL denies
    /// on a file that exists reports this too.
    NotFound,
    /// "Permission denied".
    PermissionDenied,
    /// "Failure": no reason given (a held target, a non-empty directory, an
    /// existing directory).
    Failure,
    /// Anything else.
    Other,
}

/// Classify an sftp client's stderr by its text.
pub fn classify_sftp_stderr(stderr: &str) -> SftpFailure {
    let text = stderr.to_ascii_lowercase();
    if text.contains("permission denied") {
        SftpFailure::PermissionDenied
    } else if text.contains("no such file") || text.contains(" not found") {
        SftpFailure::NotFound
    } else if text.trim_end().ends_with(": failure") || text.contains(": failure") {
        SftpFailure::Failure
    } else {
        SftpFailure::Other
    }
}

/// The second layer behind [`SftpOp`]: a rendered batch line must hold no line
/// break (it would start another command) and must not begin, after optional
/// whitespace and `@` / `-` prefixes, with `!` (a local shell escape).
fn check_batch_line(line: &str) -> Result<(), String> {
    if line.contains(['\n', '\r']) {
        return Err(format!(
            "refused a batch line holding a line break: {line:?}"
        ));
    }
    let head = line.trim_start_matches(|c: char| c.is_whitespace() || c == '@' || c == '-');
    if head.starts_with('!') {
        return Err(format!(
            "refused a batch line starting with '!' (a local shell escape): {line:?}"
        ));
    }
    Ok(())
}

/// How one node's ssh and sftp invocations are built and run.
pub struct RemoteShell {
    runner: Arc<dyn ProcessRunner>,
    host: String,
    port: u16,
    user: String,
    /// Owns the known_hosts copy and the key file; removed on drop.
    temp: Option<tempfile::TempDir>,
    known_hosts: PathBuf,
    identity: Option<PathBuf>,
    options: Vec<(String, String)>,
    op_timeout: Duration,
    label: String,
    /// Replaces the network destination (the `-D <server>` test seam).
    destination_override: Option<Vec<OsString>>,
}

impl fmt::Debug for RemoteShell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteShell")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl RemoteShell {
    /// Build the shell for `ssh` of node `label` (`nodes.master`), reading
    /// the key from the process environment.
    pub fn from_ssh(
        repo: &Path,
        target_file: &str,
        label: &str,
        ssh: &Ssh,
        runner: Arc<dyn ProcessRunner>,
    ) -> Result<RemoteShell, CoreError> {
        Self::from_ssh_with_env(repo, target_file, label, ssh, runner, &|name| {
            std::env::var(name).ok()
        })
    }

    /// [`from_ssh`](Self::from_ssh) with an injectable environment lookup
    /// (tests; avoids mutating the process environment).
    pub fn from_ssh_with_env(
        repo: &Path,
        target_file: &str,
        label: &str,
        ssh: &Ssh,
        runner: Arc<dyn ProcessRunner>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<RemoteShell, CoreError> {
        let hint = format!("{label}.ssh.known_hosts in {target_file}");
        let pinned = read_repo_file(repo, &ssh.known_hosts, Some(&hint))?;
        let has_entry = String::from_utf8_lossy(&pinned)
            .lines()
            .map(str::trim)
            .any(|line| !line.is_empty() && !line.starts_with('#'));
        if !has_entry {
            return Err(CoreError::deploy_tree_invalid(
                &ssh.known_hosts,
                None,
                None,
                format!(
                    "has no host key entries (empty or comment-only); {label}.ssh.known_hosts must pin the host key"
                ),
            ));
        }

        let temp = tempfile::Builder::new()
            .prefix("ign-ssh-")
            .tempdir()
            .map_err(|e| {
                setup_failed(label, format!("cannot create a temp dir ({:?})", e.kind()))
            })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).map_err(
                |e| {
                    setup_failed(
                        label,
                        format!("cannot restrict the temp dir ({:?})", e.kind()),
                    )
                },
            )?;
        }
        let known_hosts = temp.path().join("known_hosts");
        write_private(&known_hosts, &pinned).map_err(|e| {
            setup_failed(
                label,
                format!("cannot write the known_hosts copy ({:?})", e.kind()),
            )
        })?;

        let identity = match (&ssh.key_file, &ssh.key_env) {
            (Some(file), _) => {
                let path = PathBuf::from(file);
                let regular = std::fs::metadata(&path)
                    .map(|m| m.is_file())
                    .unwrap_or(false);
                if !regular {
                    return Err(setup_failed(
                        label,
                        format!("{label}.ssh.key_file {file:?} is not a readable regular file"),
                    ));
                }
                Some(path)
            }
            (None, Some(name)) => {
                let text = env(name).unwrap_or_default();
                if text.trim().is_empty() {
                    return Err(setup_failed(
                        label,
                        format!(
                            "environment variable {name} is not set or is empty; it must hold the private key text"
                        ),
                    ));
                }
                let key = temp.path().join("key");
                write_private(&key, normalize_private_key(&text).as_bytes()).map_err(|e| {
                    setup_failed(label, format!("cannot write the key file ({:?})", e.kind()))
                })?;
                #[cfg(windows)]
                restrict_to_owner(&key, label)?;
                Some(key)
            }
            (None, None) => None,
        };

        let connect_timeout_s = ssh.connect_timeout_s.unwrap_or(DEFAULT_CONNECT_TIMEOUT_S);
        let op_timeout_s = ssh.op_timeout_s.unwrap_or(DEFAULT_OP_TIMEOUT_S);
        let options = hardened_options(&known_hosts, identity.as_deref(), connect_timeout_s)
            .map_err(|why| setup_failed(label, why))?;
        Ok(RemoteShell {
            runner,
            host: ssh.host.clone(),
            port: ssh.port.unwrap_or(22),
            user: ssh.user.clone(),
            temp: Some(temp),
            known_hosts,
            identity,
            options,
            op_timeout: Duration::from_secs(u64::from(op_timeout_s)),
            label: label.to_string(),
            destination_override: None,
        })
    }

    /// Test seam: a shell whose sftp talks to a local `sftp-server` binary
    /// (`-D <server>`) instead of a network destination, so the sftp
    /// protocol layer is exercised without an sshd.
    #[doc(hidden)]
    pub fn local_sftp_server(
        server: &Path,
        runner: Arc<dyn ProcessRunner>,
        op_timeout: Duration,
    ) -> RemoteShell {
        RemoteShell {
            runner,
            host: "localhost".to_string(),
            port: 22,
            user: "ign".to_string(),
            temp: None,
            known_hosts: PathBuf::new(),
            identity: None,
            options: Vec::new(),
            op_timeout,
            label: "nodes.test".to_string(),
            destination_override: Some(vec![OsString::from("-D"), server.as_os_str().to_owned()]),
        }
    }

    /// The node label used in errors (`nodes.master`).
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The per-call timeout.
    pub fn op_timeout(&self) -> Duration {
        self.op_timeout
    }

    /// This shell's private temp dir (known_hosts copy and key).
    pub fn temp_dir(&self) -> Option<&Path> {
        self.temp.as_ref().map(tempfile::TempDir::path)
    }

    /// The known_hosts copy ssh is pointed at.
    pub fn known_hosts_copy(&self) -> &Path {
        &self.known_hosts
    }

    /// The identity file passed with `-i`, when one is configured.
    pub fn identity(&self) -> Option<&Path> {
        self.identity.as_deref()
    }

    fn destination(&self) -> OsString {
        OsString::from(format!("{}@{}", self.user, self.host))
    }

    fn option_args(&self) -> Vec<OsString> {
        let mut args = Vec::with_capacity(self.options.len() * 2);
        for (key, value) in &self.options {
            args.push(OsString::from("-o"));
            args.push(OsString::from(format!("{key}={value}")));
        }
        args
    }

    /// `ssh` argv: `-T -p <port> [-i <key>] -o k=v… -- user@host [command]`.
    /// `remote_command` is sent to the remote shell as one argument; a
    /// [`RemoteCommand`] can only be built injection-safe.
    pub fn ssh_argv(&self, remote_command: Option<&RemoteCommand>) -> Vec<OsString> {
        let mut args = vec![
            OsString::from("-T"),
            OsString::from("-p"),
            OsString::from(self.port.to_string()),
        ];
        if let Some(identity) = &self.identity {
            args.push(OsString::from("-i"));
            args.push(identity.as_os_str().to_owned());
        }
        args.extend(self.option_args());
        args.push(OsString::from("--"));
        args.push(self.destination());
        if let Some(command) = remote_command {
            args.push(OsString::from(command.as_str()));
        }
        args
    }

    /// `sftp` argv: `-q -b - -P <port> [-i <key>] -o k=v… -- user@host`
    /// (the batch comes on stdin).
    pub fn sftp_argv(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = ["-q", "-b", "-"].into_iter().map(OsString::from).collect();
        if let Some(destination) = &self.destination_override {
            args.extend(destination.iter().cloned());
            return args;
        }
        args.push(OsString::from("-P"));
        args.push(OsString::from(self.port.to_string()));
        if let Some(identity) = &self.identity {
            args.push(OsString::from("-i"));
            args.push(identity.as_os_str().to_owned());
        }
        args.extend(self.option_args());
        args.push(OsString::from("--"));
        args.push(self.destination());
        args
    }

    /// Run `sftp` with `ops` as the batch on stdin, then close stdin. Every
    /// op is rendered first, so one op with an unsafe path refuses the whole
    /// batch before anything spawns.
    pub async fn sftp_batch(&self, ops: &[SftpOp]) -> Result<ProcessOutput, CoreError> {
        let mut lines = Vec::with_capacity(ops.len());
        for op in ops {
            lines.push(op.render().map_err(|why| self.refused_batch(why))?);
        }
        self.send_batch(&lines).await
    }

    /// Test-only: run raw batch lines, to exercise the line guard that backs
    /// up [`SftpOp`]. Not compiled outside tests, never `pub`.
    #[cfg(test)]
    async fn sftp_batch_raw(&self, lines: &[String]) -> Result<ProcessOutput, CoreError> {
        self.send_batch(lines).await
    }

    fn refused_batch(&self, detail: String) -> CoreError {
        CoreError::TransportFailed {
            operation: "sftp".to_string(),
            node: self.label.clone(),
            detail,
        }
    }

    /// The second layer: each line passes [`check_batch_line`], then the
    /// batch goes to sftp's stdin.
    async fn send_batch(&self, lines: &[String]) -> Result<ProcessOutput, CoreError> {
        for line in lines {
            check_batch_line(line).map_err(|why| self.refused_batch(why))?;
        }
        let mut batch = lines.join("\n");
        batch.push('\n');
        Ok(self
            .runner
            .run(
                OsStr::new("sftp"),
                &self.sftp_argv(),
                Some(batch.into_bytes()),
                self.op_timeout,
            )
            .await)
    }

    /// Map a finished invocation to `Ok` only when it exited 0 within its
    /// timeout with nothing on stderr. Anything else is
    /// [`CoreError::TransportFailed`] whose detail is the scrubbed stderr
    /// tail (never the stdout, never the key).
    pub fn require_success(
        &self,
        operation: &str,
        out: ProcessOutput,
    ) -> Result<ProcessOutput, CoreError> {
        let failure = |detail: String| CoreError::TransportFailed {
            operation: operation.to_string(),
            node: self.label.clone(),
            detail,
        };
        let tail = scrub_stderr(&out.stderr, self.temp_dir());
        if out.timed_out {
            return Err(failure(format!(
                "timed out after {}s and was killed",
                self.op_timeout.as_secs()
            )));
        }
        if out.code != 0 {
            // Every sftp failure exits 1 and the class is only in stderr
            // text (C7a-3): name it. A not-found answer for a path a listing
            // just returned means "unreadable" on a Windows host, where a
            // read the ACL denies reports not found.
            let class = if out.code == 1 {
                match classify_sftp_stderr(&String::from_utf8_lossy(&out.stderr)) {
                    SftpFailure::NotFound => " (not found or unreadable)",
                    SftpFailure::PermissionDenied => " (permission denied)",
                    SftpFailure::Failure => " (refused with no reason: a held or non-empty target)",
                    SftpFailure::Other => "",
                }
            } else {
                ""
            };
            return Err(failure(if tail.is_empty() {
                format!("exit code {}{class}", out.code)
            } else {
                format!("exit code {}{class}: {tail}", out.code)
            }));
        }
        if !out.stderr.is_empty() {
            return Err(failure(format!("unexpected stderr output: {tail}")));
        }
        Ok(out)
    }

    /// Run a fixed [`WinScript`] on a Windows host over ssh and return its
    /// result. The remote command is built by
    /// [`winps::encoded_command`] (the typed
    /// [`RemoteCommand::powershell_encoded`], held to the 8000 character
    /// budget before anything spawns); success needs exit 0 within the
    /// per-call timeout and exactly one `IGN-RESULT:` stdout line, and
    /// anything else, CLIXML on stderr included, is
    /// [`CoreError::TransportFailed`] (see [`winps`]).
    pub async fn exec_powershell(
        &self,
        script: &WinScript,
    ) -> Result<serde_json::Value, CoreError> {
        let failure = |detail: String| CoreError::TransportFailed {
            operation: "exec".to_string(),
            node: self.label.clone(),
            detail,
        };
        let command = winps::encoded_command(script, COMMAND_BUDGET).map_err(&failure)?;
        let out = self
            .runner
            .run(
                OsStr::new("ssh"),
                &self.ssh_argv(Some(&command)),
                None,
                self.op_timeout,
            )
            .await;
        winps::interpret_output(&out, self.op_timeout.as_secs(), self.temp_dir())
            .map_err(|detail| failure(format!("{}: {detail}", script.name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn safe_remote_path_hostile_table() {
        for bad in [
            "", "a\nb", "a\rb", "a\tb", "a\"b", "a\\b", "a*b", "a?b", "a[1]", "a{b}", "a]b", "a}b",
        ] {
            assert!(safe_remote_path(bad).is_err(), "{bad:?} must be refused");
        }
        assert_eq!(
            safe_remote_path(
                "/opt/ignition/data/config/resources/production/ignition/database-connection/My DB,1"
            ),
            Ok(())
        );
    }

    #[test]
    fn scrub_stderr_tail() {
        let many: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        let out = scrub_stderr(many.as_bytes(), None);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), STDERR_TAIL_LINES, "{out:?}");
        assert_eq!(lines[0], "line 16");
        assert_eq!(lines[STDERR_TAIL_LINES - 1], "line 20");

        let tmp = Path::new("/var/folders/xx/ign-ssh-AbC123");
        let text = format!(
            "Load key \"{0}/key\": invalid format\nWarning: {0}/known_hosts\n",
            tmp.display()
        );
        let out = scrub_stderr(text.as_bytes(), Some(tmp));
        assert!(!out.contains("ign-ssh-AbC123"), "{out}");
        assert!(out.contains("<tmp>/key"), "{out}");
        assert!(out.contains("<tmp>/known_hosts"), "{out}");

        let out = scrub_stderr(b"\x1b[31mred\x1b[0m\x07 bell\x00nul\r\nnext\n", None);
        assert!(out.chars().all(|c| c == '\n' || !c.is_control()), "{out:?}");
        assert!(out.contains("red") && out.contains("next"), "{out:?}");
        assert!(!out.contains("[31m"), "{out:?}");

        let big = "é".repeat(2000);
        let out = scrub_stderr(big.as_bytes(), None);
        assert!(out.len() <= 1024, "{}", out.len());
        assert!(!out.is_empty());
    }

    #[test]
    fn normalize_private_key_fixes_crlf_and_trailing_newline() {
        assert_eq!(normalize_private_key("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(normalize_private_key("a\nb"), "a\nb\n");
        assert_eq!(normalize_private_key("a\nb\n\n\n"), "a\nb\n");
        assert_eq!(normalize_private_key("a\rb"), "a\nb\n");
    }

    #[test]
    fn option_paths_are_quoted_escaped_and_refused() {
        assert_eq!(
            option_path(Path::new("/tmp/a b/x")),
            Ok("\"/tmp/a b/x\"".to_string())
        );
        assert_eq!(
            option_path(Path::new("/tmp/p%h/x")),
            Ok("/tmp/p%%h/x".to_string())
        );
        assert!(option_path(Path::new("/tmp/a\"b")).is_err());
        assert!(option_path(Path::new("/tmp/$HOME")).is_err());
        assert!(option_path(Path::new("/tmp/a\nb")).is_err());
    }

    #[test]
    fn hardened_options_order_is_fixed() {
        let keys = |identity: Option<&Path>| -> Vec<String> {
            hardened_options(Path::new("/t/kh"), identity, 10)
                .expect("options")
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        };
        let without = keys(None);
        assert_eq!(without.len(), 20);
        assert_eq!(without[0], "BatchMode");
        assert_eq!(without[2], "UserKnownHostsFile");
        assert_eq!(without[19], "LogLevel");
        assert!(!without.contains(&"IdentitiesOnly".to_string()));
        let with = keys(Some(Path::new("/t/key")));
        assert_eq!(with.len(), 21);
        assert_eq!(with[10], "IdentitiesOnly");
    }

    // ---- RemoteCommand (22-05 review, F5a) --------------------------------

    const HOSTILE_ARGS: [&str; 12] = [
        "plain",
        "with space",
        "it's",
        "'",
        "a;b",
        "$(touch IGN_PWNED)",
        "`touch IGN_PWNED`",
        "!rm -rf /",
        "a && b | c > d",
        "\"quoted\"",
        "back\\slash",
        "*.json ~ {a,b} $HOME",
    ];

    #[test]
    fn posix_command_single_quotes_every_argument() {
        let cmd = RemoteCommand::posix(&["echo", "it's", "a b"]).expect("posix");
        assert_eq!(cmd.as_str(), r"'echo' 'it'\''s' 'a b'");
        let cmd = RemoteCommand::posix(&["true"]).expect("posix");
        assert_eq!(cmd.as_str(), "'true'");
    }

    #[test]
    fn posix_command_refuses_nul_control_characters_and_empty_argv() {
        for bad in [
            "a\0b", "a\nb", "a\rb", "a\tb", "\x1b[31m", "a\x7fb", "\u{85}", "\u{9f}",
        ] {
            assert!(
                RemoteCommand::posix(&["echo", bad]).is_err(),
                "{bad:?} must be refused"
            );
            assert!(
                RemoteCommand::posix(&[bad]).is_err(),
                "{bad:?} must be refused as the program"
            );
        }
        assert!(RemoteCommand::posix(&[]).is_err(), "empty argv");
        assert!(
            RemoteCommand::posix(&[""]).is_ok(),
            "an empty arg is quoted"
        );
        assert_eq!(RemoteCommand::posix(&[""]).unwrap().as_str(), "''");
    }

    #[cfg(unix)]
    #[test]
    fn posix_command_round_trips_through_a_local_sh() {
        let dir = tempfile::tempdir().expect("dir");
        // printf prints each argument NUL-terminated, so the output is the
        // argv exactly as the shell tokenised it
        let mut argv: Vec<&str> = vec!["printf", "%s\\0"];
        argv.extend(HOSTILE_ARGS);
        let cmd = RemoteCommand::posix(&argv).expect("posix");
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd.as_str())
            .current_dir(dir.path())
            .output()
            .expect("sh");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let printed: Vec<&[u8]> = out
            .stdout
            .split(|b| *b == 0)
            .filter(|chunk| !chunk.is_empty())
            .collect();
        let expected: Vec<&[u8]> = HOSTILE_ARGS.iter().map(|a| a.as_bytes()).collect();
        assert_eq!(printed, expected, "the shell saw the argv unchanged");
        assert!(
            std::fs::read_dir(dir.path())
                .expect("read")
                .next()
                .is_none(),
            "no hostile argument ran as a command"
        );
    }

    #[test]
    fn powershell_command_decodes_back_to_the_script() {
        for script in [
            "Get-ChildItem 'C:\\Program Files'",
            "Write-Output \"caf\u{e9} \u{20ac} \u{1f600}\"; $x = $(whoami); `n",
            "'; Remove-Item -Recurse C:\\ ; '",
            "",
        ] {
            let cmd = RemoteCommand::powershell_encoded(script);
            let prefix = "powershell -NoProfile -NonInteractive -EncodedCommand ";
            let b64 = cmd
                .as_str()
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("prefix: {}", cmd.as_str()));
            assert!(
                b64.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='),
                "base64 is a safe charset: {b64}"
            );
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .expect("base64");
            assert_eq!(bytes.len() % 2, 0);
            let units: Vec<u16> = bytes
                .chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            assert_eq!(String::from_utf16(&units).expect("utf16"), script);
        }
        // a known vector: "Write-Output hi" in UTF-16LE base64
        assert_eq!(
            RemoteCommand::powershell_encoded("hi").as_str(),
            "powershell -NoProfile -NonInteractive -EncodedCommand aABpAA=="
        );
    }

    // ---- SftpOp (22-05 review, F5b) ---------------------------------------

    fn render_all(ops: &[SftpOp]) -> Vec<String> {
        ops.iter().map(|op| op.render().expect("render")).collect()
    }

    #[test]
    fn sftp_ops_render_exact_lines() {
        let ops = [
            SftpOp::Ls {
                path: "/d/a b".into(),
            },
            SftpOp::LsLong { path: "/d".into() },
            SftpOp::LsLongAll { path: "/d".into() },
            SftpOp::Pwd,
            SftpOp::Get {
                remote: "/r/f".into(),
                local: PathBuf::from("/l/f"),
            },
            SftpOp::GetRecursive {
                remote: "/r/dir".into(),
                local: PathBuf::from("/l/mirror/"),
            },
            SftpOp::Put {
                local: PathBuf::from("/l/f"),
                remote: "/r/f".into(),
            },
            SftpOp::Rename {
                from: "/r/a".into(),
                to: "/r/b".into(),
            },
            SftpOp::Rm {
                path: "/r/f".into(),
            },
            SftpOp::Mkdir {
                path: "/r/d".into(),
            },
            SftpOp::Rmdir {
                path: "/r/d".into(),
            },
        ];
        assert_eq!(
            render_all(&ops),
            [
                "@ls -1 \"/d/a b\"",
                "@ls -l \"/d\"",
                "@ls -la \"/d\"",
                "@pwd",
                "get \"/r/f\" \"/l/f\"",
                "get -R \"/r/dir\" \"/l/mirror/\"",
                "put \"/l/f\" \"/r/f\"",
                "rename \"/r/a\" \"/r/b\"",
                "rm \"/r/f\"",
                "mkdir \"/r/d\"",
                "rmdir \"/r/d\"",
            ]
        );
        for line in render_all(&ops) {
            assert!(
                line.starts_with(|c: char| c.is_ascii_lowercase() || c == '@'),
                "{line}"
            );
        }
    }

    #[test]
    fn sftp_ops_refuse_hostile_remote_and_local_paths() {
        let refused = [
            "a\nrm \"/x\"",
            "a\rb",
            "a\"; rm \"/x",
            "a\\b",
            "a*b",
            "a?b",
            "a[1]",
            "a{b}",
            "",
            "-R",
            "-rf",
            "a\0b",
            "a\x1bb",
        ];
        for bad in refused {
            let remote_ops = [
                SftpOp::Ls { path: bad.into() },
                SftpOp::LsLong { path: bad.into() },
                SftpOp::LsLongAll { path: bad.into() },
                SftpOp::Get {
                    remote: bad.into(),
                    local: PathBuf::from("/l"),
                },
                SftpOp::GetRecursive {
                    remote: bad.into(),
                    local: PathBuf::from("/l"),
                },
                SftpOp::Put {
                    local: PathBuf::from("/l"),
                    remote: bad.into(),
                },
                SftpOp::Rename {
                    from: bad.into(),
                    to: "/r/b".into(),
                },
                SftpOp::Rename {
                    from: "/r/a".into(),
                    to: bad.into(),
                },
                SftpOp::Rm { path: bad.into() },
                SftpOp::Mkdir { path: bad.into() },
                SftpOp::Rmdir { path: bad.into() },
            ];
            for op in &remote_ops {
                assert!(op.render().is_err(), "{op:?} must be refused");
            }
            let local_ops = [
                SftpOp::Get {
                    remote: "/r".into(),
                    local: PathBuf::from(bad),
                },
                SftpOp::GetRecursive {
                    remote: "/r".into(),
                    local: PathBuf::from(bad),
                },
                SftpOp::Put {
                    local: PathBuf::from(bad),
                    remote: "/r".into(),
                },
            ];
            for op in &local_ops {
                assert!(op.render().is_err(), "{op:?} must be refused");
            }
        }
    }

    #[test]
    fn sftp_ops_keep_other_hostile_text_inertly_inside_the_quotes() {
        // each of these is a path character sftp's double quotes make inert;
        // none can start a line, so none can be a command or a `!` shell escape
        for inert in [
            "!rm -rf /x",
            "/d/a;b",
            "/d/$(touch IGN_PWNED)",
            "/d/`touch IGN_PWNED`",
            "/d/it's",
            "/d/a&b|c>d",
            "/d/~tilde",
        ] {
            let line = SftpOp::Rm { path: inert.into() }.render().expect("render");
            assert_eq!(line, format!("rm \"{inert}\""));
            assert!(line.starts_with("rm \""), "{line}");
        }
    }

    #[test]
    fn local_path_check_table() {
        for bad in [
            "", "a\"b", "a\\b", "a*b", "a?b", "a[b", "a]b", "a{b", "a}b", "a\nb", "a\tb", "-x",
        ] {
            assert!(safe_local_path(bad).is_err(), "{bad:?} must be refused");
        }
        for ok in [
            "/tmp/ign-ssh-AbC/mirror/",
            "/tmp/a b,c/d",
            "rel/dir/file.json",
            "/tmp/it's",
        ] {
            assert_eq!(safe_local_path(ok), Ok(()), "{ok:?}");
        }
    }

    #[test]
    fn batch_line_guard_refuses_line_breaks_and_a_leading_bang() {
        for line in [
            "!rm -rf /",
            "@!rm -rf /",
            "-!rm -rf /",
            "  !id",
            "-@!id",
            "ls \"/d\"\nrm \"/x\"",
            "ls \"/d\"\rrm \"/x\"",
        ] {
            assert!(check_batch_line(line).is_err(), "{line:?} must be refused");
        }
        for line in ["@ls -1 \"/d\"", "get \"/r\" \"/l\"", "rm \"!x\""] {
            assert_eq!(check_batch_line(line), Ok(()), "{line:?}");
        }
    }

    #[derive(Default)]
    struct Recording {
        calls: std::sync::Mutex<Vec<Option<Vec<u8>>>>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for Recording {
        async fn run(
            &self,
            _program: &OsStr,
            _args: &[OsString],
            stdin: Option<Vec<u8>>,
            _timeout: Duration,
        ) -> ProcessOutput {
            self.calls.lock().expect("calls").push(stdin);
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: 0,
                timed_out: false,
            }
        }
    }

    #[tokio::test]
    async fn sftp_batch_sends_typed_lines_and_refuses_bad_ops_before_spawning() {
        let recording = Arc::new(Recording::default());
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            recording.clone(),
            Duration::from_secs(1),
        );
        shell
            .sftp_batch(&[
                SftpOp::Ls { path: "/d".into() },
                SftpOp::Rm {
                    path: "/d/x".into(),
                },
            ])
            .await
            .expect("batch");
        assert_eq!(
            recording.calls.lock().expect("calls")[0].as_deref(),
            Some(b"@ls -1 \"/d\"\nrm \"/d/x\"\n".as_slice())
        );

        // one bad op refuses the whole batch before anything spawns
        let err = shell
            .sftp_batch(&[
                SftpOp::Ls { path: "/d".into() },
                SftpOp::Rm {
                    path: "/d\nrm \"/x\"".into(),
                },
            ])
            .await
            .expect_err("refused");
        assert!(matches!(err, CoreError::TransportFailed { .. }));
        assert_eq!(recording.calls.lock().expect("calls").len(), 1);

        // the raw runner is the second layer: line break and leading `!`
        for line in ["ls \"/d\"\nrm \"/x\"", "!rm -rf /", "@!id"] {
            let err = shell
                .sftp_batch_raw(&[line.to_string()])
                .await
                .expect_err("raw line refused");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{line:?}");
        }
        assert_eq!(recording.calls.lock().expect("calls").len(), 1);
    }

    // ---- 22-13: Windows host behaviours from C7a ---------------------------

    #[test]
    fn rename_onto_itself_is_refused_because_it_deletes_the_file() {
        // C7a-4: on Win32-OpenSSH `rename x x` answers "No such file" and the
        // file is GONE. No spelling of the same path may reach the host.
        for (from, to) in [
            ("/C:/Users/u/f.json", "/C:/Users/u/f.json"),
            ("/C:/Users/u/f.json", "/c:/users/U/F.JSON"),
            ("/C:/Users/u//f.json", "/C:/Users/u/f.json"),
            ("/C:/Users/u/dir/", "/C:/Users/u/dir"),
            ("rel/f", "rel/f"),
        ] {
            let err = SftpOp::Rename {
                from: from.into(),
                to: to.into(),
            }
            .render()
            .expect_err(from);
            assert!(err.contains("onto itself"), "{from} -> {to}: {err}");
        }
        // different paths still render
        assert!(
            SftpOp::Rename {
                from: "/C:/Users/u/staging/f.json".into(),
                to: "/C:/Program Files/x/f.json".into(),
            }
            .render()
            .is_ok()
        );
        assert!(same_remote_path("/a/B", "/A/b"));
        assert!(!same_remote_path("/a/b", "/a/bc"));
    }

    #[test]
    fn sftp_stderr_is_classified_by_text() {
        use SftpFailure::*;
        for (text, want) in [
            ("Can't ls: \"/C:/x\" not found", NotFound),
            ("File \"/C:/x\" not found.", NotFound),
            ("remote delete /C:/x: No such file or directory", NotFound),
            (
                "remote rename \"a\" to \"b\": No such file or directory",
                NotFound,
            ),
            ("dest open \"/C:/x\": Permission denied", PermissionDenied),
            (
                "remote mkdir \"/C:/x\": Permission denied",
                PermissionDenied,
            ),
            ("remote mkdir \"/C:/x\": Failure", Failure),
            ("remote rename \"a\" to \"b\": Failure", Failure),
            ("remote readdir(\"/C:/x/\"): Bad message", Other),
            ("", Other),
        ] {
            assert_eq!(classify_sftp_stderr(text), want, "{text}");
        }
    }

    #[test]
    fn require_success_names_the_sftp_failure_class_for_exit_1_only() {
        let recording = Arc::new(Recording::default());
        let shell =
            RemoteShell::local_sftp_server(Path::new("/unused"), recording, Duration::from_secs(1));
        let out = |code: i32, stderr: &str| ProcessOutput {
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
            code,
            timed_out: false,
        };
        let detail = |o: ProcessOutput| match shell.require_success("read", o).expect_err("fail") {
            CoreError::TransportFailed { detail, .. } => detail,
            other => panic!("{other:?}"),
        };
        assert!(
            detail(out(1, "File \"/C:/x\" not found.\r\n")).contains("(not found or unreadable)")
        );
        assert!(detail(out(1, "dest open: Permission denied")).contains("(permission denied)"));
        assert!(detail(out(1, "remote rename: Failure")).contains("(refused with no reason"));
        // ssh's own auth failure exits 255 and is not an sftp class
        let ssh = detail(out(255, "Permission denied (publickey)."));
        assert!(!ssh.contains("(permission denied)"), "{ssh}");
    }

    struct ScriptedExec {
        out: Mutex<Option<ProcessOutput>>,
        argv: Mutex<Vec<(String, Vec<String>)>>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for ScriptedExec {
        async fn run(
            &self,
            program: &OsStr,
            args: &[OsString],
            _stdin: Option<Vec<u8>>,
            _timeout: Duration,
        ) -> ProcessOutput {
            self.argv.lock().expect("argv").push((
                program.to_string_lossy().into_owned(),
                args.iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
            ));
            self.out.lock().expect("out").take().expect("one call")
        }
    }

    fn exec_shell(out: ProcessOutput) -> (Arc<ScriptedExec>, RemoteShell) {
        let runner = Arc::new(ScriptedExec {
            out: Mutex::new(Some(out)),
            argv: Mutex::new(Vec::new()),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            runner.clone(),
            Duration::from_secs(3),
        );
        (runner, shell)
    }

    fn exec_script(params: serde_json::Value) -> WinScript {
        WinScript {
            name: "probe",
            body: "E @{ok=1}",
            params,
        }
    }

    #[tokio::test]
    async fn exec_powershell_runs_ssh_with_the_encoded_command_and_parses_the_sentinel() {
        let (runner, shell) = exec_shell(ProcessOutput {
            stdout: b"IGN-RESULT:{\"ok\":1}\r\n".to_vec(),
            stderr: Vec::new(),
            code: 0,
            timed_out: false,
        });
        let value = shell
            .exec_powershell(&exec_script(serde_json::Value::Null))
            .await
            .expect("exec");
        assert_eq!(value["ok"], 1);
        let argv = runner.argv.lock().expect("argv");
        assert_eq!(argv[0].0, "ssh");
        let last = argv[0].1.last().expect("remote command");
        assert!(
            last.starts_with("powershell -NoProfile -NonInteractive -EncodedCommand "),
            "{last}"
        );
        // the encoded command is ONE argv element, after `--` and the destination
        assert!(argv[0].1.iter().any(|a| a == "--"));
    }

    #[tokio::test]
    async fn exec_powershell_failures_are_transport_failed() {
        let cases = [
            (
                "non-zero exit with CLIXML",
                ProcessOutput {
                    stdout: Vec::new(),
                    stderr: b"#< CLIXML\r\n<Objs><S S=\"Error\">boom_x000D__x000A_</S></Objs>"
                        .to_vec(),
                    code: 1,
                    timed_out: false,
                },
                "boom",
            ),
            (
                "missing sentinel",
                ProcessOutput {
                    stdout: b"hello\r\n".to_vec(),
                    stderr: Vec::new(),
                    code: 0,
                    timed_out: false,
                },
                "IGN-RESULT:",
            ),
            (
                "timeout",
                ProcessOutput {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    code: -1,
                    timed_out: true,
                },
                "timed out after 3s",
            ),
        ];
        for (name, out, needle) in cases {
            let (_runner, shell) = exec_shell(out);
            let err = shell
                .exec_powershell(&exec_script(serde_json::Value::Null))
                .await
                .expect_err(name);
            let CoreError::TransportFailed {
                operation, detail, ..
            } = err
            else {
                panic!("{name}: not transport_failed");
            };
            assert_eq!(operation, "exec", "{name}");
            assert!(detail.contains(needle), "{name}: {detail}");
            assert!(detail.starts_with("probe: "), "{name}: {detail}");
        }
    }

    #[tokio::test]
    async fn exec_powershell_over_budget_never_spawns() {
        // an over-budget command is refused before the runner is called: the
        // scripted runner would panic on a call with no output queued
        let runner = Arc::new(ScriptedExec {
            out: Mutex::new(None),
            argv: Mutex::new(Vec::new()),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            runner.clone(),
            Duration::from_secs(3),
        );
        let big = "x".repeat(winps::COMMAND_BUDGET);
        let err = shell
            .exec_powershell(&exec_script(serde_json::json!({ "p": big })))
            .await
            .expect_err("over budget");
        assert!(matches!(err, CoreError::TransportFailed { .. }));
        assert!(
            runner.argv.lock().expect("argv").is_empty(),
            "nothing spawned"
        );
    }
}
