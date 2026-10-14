//! The process seam under the ssh transport (22-05).
//!
//! Every `ssh` and `sftp` invocation is an argv vector handed to a
//! [`ProcessRunner`]; there is no shell string anywhere. The production
//! [`TokioProcessRunner`] differs from the rig's `DockerCompose::spawn` on
//! purpose:
//!
//! - **stdin is piped and closed, never `Stdio::null()` and never
//!   inherited.** The child sees EOF as soon as the optional stdin bytes are
//!   written. A Windows `ssh.exe` client can hang when its stdin is NUL
//!   (openssh-bugs 2965), and an inherited stdin would let a stray prompt
//!   block on the operator's terminal or a CI pipe.
//! - **A hard timeout kills the child.** The call returns only after the
//!   child has been killed and reaped, so a timed-out invocation leaves no
//!   process behind.
//! - **Capture is bounded.** Stdout is capped at [`STDOUT_CAP_BYTES`]; going
//!   over kills the child and reports failure (code [`OVERFLOW_CODE`]), so a
//!   truncated listing is never mistaken for a complete one. Stderr keeps its
//!   last [`STDERR_CAP_BYTES`] bytes.
//! - **A spawn failure is data**, code 127 with the reason, never a panic.

use std::ffi::{OsStr, OsString};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Stdout above this many bytes is a failure (32 MiB). File content never
/// travels on stdout (it is downloaded to a temp dir), only listings.
pub const STDOUT_CAP_BYTES: usize = 32 * 1024 * 1024;
/// Only the last this many bytes of stderr are kept (64 KiB).
pub const STDERR_CAP_BYTES: usize = 64 * 1024;
/// Exit code reported when stdout went over its cap and the child was killed.
pub const OVERFLOW_CODE: i32 = -2;
/// Exit code reported when the program could not be spawned (shell-style 127).
pub const SPAWN_FAILED_CODE: i32 = 127;

/// One finished invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// Captured stdout (at most [`STDOUT_CAP_BYTES`]).
    pub stdout: Vec<u8>,
    /// Captured stderr (the last [`STDERR_CAP_BYTES`] bytes).
    pub stderr: Vec<u8>,
    /// Exit code; `-1` when killed by a signal or on timeout, `-2` on stdout
    /// overflow, `127` when the program could not be spawned.
    pub code: i32,
    /// The timeout elapsed and the child was killed.
    pub timed_out: bool,
}

/// Spawns a program with an argv vector. Fakes script this trait in tests.
#[async_trait::async_trait]
pub trait ProcessRunner: Send + Sync {
    /// Run `program args…` with `stdin` bytes (if any) written then closed,
    /// killing the child after `timeout`.
    async fn run(
        &self,
        program: &OsStr,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
        timeout: Duration,
    ) -> ProcessOutput;
}

/// The production runner.
pub struct TokioProcessRunner;

#[async_trait::async_trait]
impl ProcessRunner for TokioProcessRunner {
    async fn run(
        &self,
        program: &OsStr,
        args: &[OsString],
        stdin: Option<Vec<u8>>,
        timeout: Duration,
    ) -> ProcessOutput {
        run_capped(program, args, stdin, timeout, STDOUT_CAP_BYTES).await
    }
}

/// Stdout went over its cap.
struct Overflow;

/// Read `reader` to EOF, failing as soon as more than `cap` bytes arrive.
async fn read_capped<R>(reader: Option<R>, cap: usize) -> Result<Vec<u8>, Overflow>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(mut reader) = reader else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => return Ok(out),
            Ok(n) => {
                if out.len() + n > cap {
                    return Err(Overflow);
                }
                out.extend_from_slice(&chunk[..n]);
            }
        }
    }
}

/// Read `reader` to EOF keeping only the last `cap` bytes.
async fn read_tail<R>(reader: Option<R>, cap: usize) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(mut reader) = reader else {
        return Vec::new();
    };
    let mut out: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => return out,
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if out.len() > cap {
                    let excess = out.len() - cap;
                    out.drain(..excess);
                }
            }
        }
    }
}

fn failed(code: i32, timed_out: bool, note: String) -> ProcessOutput {
    ProcessOutput {
        stdout: Vec::new(),
        stderr: note.into_bytes(),
        code,
        timed_out,
    }
}

pub(crate) async fn run_capped(
    program: &OsStr,
    args: &[OsString],
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    stdout_cap: usize,
) -> ProcessOutput {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return failed(
                SPAWN_FAILED_CODE,
                false,
                format!(
                    "failed to spawn {}: {:?}",
                    program.to_string_lossy(),
                    err.kind()
                ),
            );
        }
    };
    let stdin_handle = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let io = async {
        let write = async {
            if let Some(mut handle) = stdin_handle {
                if let Some(bytes) = &stdin {
                    // A child that exits early closes the pipe; that is its
                    // exit status's business, not a write error to report.
                    let _ = handle.write_all(bytes).await;
                }
                let _ = handle.shutdown().await;
                // Dropping the handle closes the pipe: the child sees EOF.
            }
            Ok::<(), Overflow>(())
        };
        let err = async { Ok::<Vec<u8>, Overflow>(read_tail(stderr, STDERR_CAP_BYTES).await) };
        let ((), out, err) = tokio::try_join!(write, read_capped(stdout, stdout_cap), err)?;
        let status = child.wait().await;
        Ok::<_, Overflow>((out, err, status))
    };

    match tokio::time::timeout(timeout, io).await {
        Ok(Ok((stdout, stderr, status))) => ProcessOutput {
            stdout,
            stderr,
            code: status.ok().and_then(|s| s.code()).unwrap_or(-1),
            timed_out: false,
        },
        Ok(Err(Overflow)) => {
            // Kill and reap so nothing is left running.
            let _ = child.kill().await;
            failed(
                OVERFLOW_CODE,
                false,
                format!("stdout exceeded {stdout_cap} bytes; the child was killed"),
            )
        }
        Err(_elapsed) => {
            let _ = child.kill().await;
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: -1,
                timed_out: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_child() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pidfile = dir.path().join("pid");
        let script = format!("echo $$ > '{}'; exec sleep 30", pidfile.display());
        let started = std::time::Instant::now();
        let out = TokioProcessRunner
            .run(
                OsStr::new("sh"),
                &[os("-c"), os(&script)],
                None,
                Duration::from_millis(300),
            )
            .await;
        assert!(out.timed_out, "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid = std::fs::read_to_string(&pidfile)
            .expect("pid file")
            .trim()
            .to_string();
        let alive = std::process::Command::new("kill")
            .args(["-0", &pid])
            .stderr(std::process::Stdio::null())
            .status()
            .expect("kill")
            .success();
        assert!(!alive, "child {pid} survived the timeout");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn timeout_kills_child() {
        let started = std::time::Instant::now();
        let out = TokioProcessRunner
            .run(
                OsStr::new("ping"),
                &[os("-n"), os("30"), os("127.0.0.1")],
                None,
                Duration::from_millis(300),
            )
            .await;
        assert!(out.timed_out, "{out:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdin_is_closed() {
        let empty = TokioProcessRunner
            .run(OsStr::new("cat"), &[], None, Duration::from_secs(10))
            .await;
        assert_eq!((empty.code, empty.timed_out), (0, false), "{empty:?}");
        assert!(empty.stdout.is_empty());
        let fed = TokioProcessRunner
            .run(
                OsStr::new("cat"),
                &[],
                Some(b"abc".to_vec()),
                Duration::from_secs(10),
            )
            .await;
        assert_eq!(fed.stdout, b"abc");
        assert_eq!(fed.code, 0);
    }

    #[tokio::test]
    async fn spawn_failure_is_127() {
        let out = TokioProcessRunner
            .run(
                OsStr::new("ign-no-such-program-22-05"),
                &[],
                None,
                Duration::from_secs(5),
            )
            .await;
        assert_eq!(out.code, 127);
        assert!(!out.timed_out);
        let reason = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(reason.contains("failed to spawn"), "{reason}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdout_over_the_cap_kills_and_fails() {
        let out = run_capped(
            OsStr::new("sh"),
            &[os("-c"), os("yes | head -c 1000000; exec sleep 30")],
            None,
            Duration::from_secs(20),
            1024,
        )
        .await;
        assert_eq!(out.code, OVERFLOW_CODE, "{out:?}");
        assert!(!out.timed_out);
        assert!(out.stdout.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stderr_keeps_only_its_tail() {
        let out = TokioProcessRunner
            .run(
                OsStr::new("sh"),
                &[
                    os("-c"),
                    os("head -c 200000 /dev/zero | tr '\\0' x >&2; echo END >&2"),
                ],
                None,
                Duration::from_secs(20),
            )
            .await;
        assert_eq!(out.code, 0);
        assert!(out.stderr.len() <= STDERR_CAP_BYTES);
        assert!(out.stderr.ends_with(b"END\n"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nonzero_exit_code_is_reported() {
        let out = TokioProcessRunner
            .run(
                OsStr::new("sh"),
                &[os("-c"), os("echo oops >&2; exit 3")],
                None,
                Duration::from_secs(10),
            )
            .await;
        assert_eq!(out.code, 3);
        assert_eq!(out.stderr, b"oops\n");
    }
}
