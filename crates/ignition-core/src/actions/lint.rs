//! `ign lint` — external ignition-lint delegation (07-04, INTR-02).
//!
//! LOCAL delegation: no [`crate::client::GatewayApi`] surface, no
//! profile, no credential — the verb discovers `ignition-lint` on
//! PATH and spawns it with an ARG VECTOR (no shell interpolation,
//! the compose precedent). Exit semantics are the planner-locked
//! **doctor posture**: `ign lint` exits 0 whenever the child RAN —
//! findings, the child's exit code, and the parsed JSON report ride
//! as DATA; `--strict` flips to literal child-exit passthrough for
//! CI (the one sanctioned success-path exit exception, decided in
//! the binary after the envelope renders — README "Linting").
//! Findings never masquerade as `CoreError::Internal` (07-RESEARCH
//! Pitfall 6).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::error::CoreError;

/// The tool binary this verb delegates to.
const TOOL_NAME: &str = "ignition-lint";

/// How much of the child's stderr rides the result (a preview — the
/// full diagnostics already printed to the child's stderr handle we
/// capture; humans see the preview, agents get the shape).
const STDERR_PREVIEW_CAP: usize = 4000;

/// An executable regular file (unix checks any execute bit).
#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

/// An executable regular file (non-unix falls back to file-ness).
#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Split `PATH` and return the first `ignition-lint` that is an
/// executable regular file — std fs + env, no `which` dependency.
pub fn find_lint_tool() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(TOOL_NAME))
        .find(|candidate| is_executable_file(candidate))
}

/// `ign lint` output model — the doctor-posture shape, ALL keys
/// always: the child ran, its exit code rides as data alongside the
/// parsed report (null when stdout was not ignition-lint's JSON).
#[derive(Debug, Serialize)]
pub struct LintResult {
    /// Always `true` in a result (an absent tool refuses; a spawn
    /// failure errors — this shape only exists when the child ran).
    pub ran: bool,
    /// The resolved tool path (what ran).
    pub tool: String,
    /// The child's exit code (`null` only when a signal killed it).
    pub child_exit_code: Option<i32>,
    /// Issues counted from the parsed report (0 when unparseable).
    pub issues_found: usize,
    /// The parsed `{issues, summary}` JSON report, when stdout
    /// parsed; `null` otherwise (stdout still rides verbatim).
    pub report: Option<serde_json::Value>,
    /// The child's stdout, verbatim.
    pub stdout: String,
    /// The child's stderr, previewed (capped).
    pub stderr_preview: String,
    /// Whether --strict ran (display-only; the strict EXIT is
    /// decided by the binary after the envelope renders — never in
    /// the agent JSON).
    #[serde(skip)]
    pub strict: bool,
}

impl LintResult {
    /// The `--strict` exit passthrough: the child's code masked to
    /// the 0..128 range (`code & 0x7f` — the shell signal
    /// convention); a signal-killed child is not success (1).
    /// `None` in the default doctor posture.
    pub fn strict_exit_code(&self) -> Option<u8> {
        if !self.strict {
            return None;
        }
        match self.child_exit_code {
            Some(code) => Some((code & 0x7f) as u8),
            None => Some(1),
        }
    }
}

/// THE delegation: discover the tool (absent → the additive
/// `lint_tool_absent` exit 6 with the install hint), spawn it with
/// the arg vector `--report-format json --target <PATH>...` plus any
/// `--` passthrough extras, and map the outcome to the
/// doctor-posture result. A spawn failure (the binary vanished
/// between discovery and exec) is the honest usage-class refusal —
/// the tool was found but could not run.
pub async fn lint_run(
    paths: &[String],
    strict: bool,
    extra_args: &[String],
) -> Result<LintResult, CoreError> {
    let Some(tool) = find_lint_tool() else {
        return Err(CoreError::LintToolAbsent);
    };
    // ARG VECTOR (never a shell string — injection safety, the
    // compose seam precedent). `--target <path>` per path, extras
    // after, verbatim.
    let mut args: Vec<&str> = vec!["--report-format", "json"];
    for path in paths {
        args.push("--target");
        args.push(path);
    }
    args.extend(extra_args.iter().map(String::as_str));
    let output = Command::new(&tool)
        .args(&args)
        .output()
        .await
        .map_err(|err| CoreError::InvalidInput {
            reason: format!(
                "ignition-lint was found at {} but could not run: {err}",
                tool.display()
            ),
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr_full = String::from_utf8_lossy(&output.stderr).into_owned();
    let stderr_preview = if stderr_full.len() > STDERR_PREVIEW_CAP {
        let mut capped: String = stderr_full.chars().take(STDERR_PREVIEW_CAP).collect();
        capped.push_str("\n… (truncated)");
        capped
    } else {
        stderr_full
    };
    // ignition-lint's JSON report shape: {issues: [...], summary:
    // {...}} — parse when it parses; anything else rides stdout
    // verbatim with report null.
    let report: Option<serde_json::Value> = serde_json::from_str(&stdout).ok();
    let issues_found = report
        .as_ref()
        .and_then(|report| report.get("issues"))
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    Ok(LintResult {
        ran: true,
        tool: tool.display().to_string(),
        child_exit_code: output.status.code(),
        issues_found,
        report,
        stdout,
        stderr_preview,
        strict,
    })
}
