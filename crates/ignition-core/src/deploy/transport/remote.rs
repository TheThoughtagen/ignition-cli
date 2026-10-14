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
//!
//! The exact `-o` order is [`hardened_options`]; `ssh -G` on the generated
//! argv is pinned by `tests/deploy_transport.rs`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::process::{ProcessOutput, ProcessRunner};
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
    if path.is_empty() {
        return Err("remote path must not be empty".to_string());
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
            "remote path {path:?} contains {why} ({c:?}), which cannot be placed safely in an sftp batch line"
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
    /// `remote_command` is sent to the remote shell as one argument; the
    /// caller must have built it only from validated values.
    pub fn ssh_argv(&self, remote_command: Option<&str>) -> Vec<OsString> {
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
            args.push(OsString::from(command));
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

    /// Run `sftp` with `lines` as the batch on stdin, then close stdin.
    /// Every line must already have been built from [`safe_remote_path`]
    /// inputs; a line holding a line break is refused here as a second
    /// guard, because it would start another command.
    pub async fn sftp_batch(&self, lines: &[String]) -> Result<ProcessOutput, CoreError> {
        if let Some(bad) = lines.iter().find(|l| l.contains(['\n', '\r'])) {
            return Err(CoreError::TransportFailed {
                operation: "sftp".to_string(),
                node: self.label.clone(),
                detail: format!("refused a batch line holding a line break: {bad:?}"),
            });
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
            return Err(failure(if tail.is_empty() {
                format!("exit code {}", out.code)
            } else {
                format!("exit code {}: {tail}", out.code)
            }));
        }
        if !out.stderr.is_empty() {
            return Err(failure(format!("unexpected stderr output: {tail}")));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
