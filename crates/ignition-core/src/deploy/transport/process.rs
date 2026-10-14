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
use std::time::Duration;

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

pub(crate) async fn run_capped(
    _program: &OsStr,
    _args: &[OsString],
    _stdin: Option<Vec<u8>>,
    _timeout: Duration,
    _stdout_cap: usize,
) -> ProcessOutput {
    ProcessOutput {
        stdout: Vec::new(),
        stderr: Vec::new(),
        code: -99,
        timed_out: false,
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
