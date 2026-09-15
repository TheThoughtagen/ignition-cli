//! `ign edit` core — the Editor seam, the private edit tree, and the
//! edit pipeline (13-05).
//!
//! Three pieces live here:
//!
//! - [`Editor`] / [`TokioEditor`] — the swappable editor seam. The
//!   real impl resolves `VISUAL` → `EDITOR` (trimmed; empty string =
//!   unset; no platform fallback — `open -t -W` semantics vary, so a
//!   missing editor refuses with a clear "set $EDITOR" message) and
//!   spawns it with an ARG VECTOR (`<editor> <flags...> <target>`,
//!   the target path appended LAST) — never a shell string (the
//!   lint.rs delegation precedent; injection-safe by construction).
//!   The child's exit status is ADVISORY (Pitfall E1: vscode/emacs
//!   fork or daemonize, IDE shims linger) — whether an edit happened
//!   is decided by CONTENT, never by exit code.
//! - [`EditTempDir`] — one private (0700 on unix), unique-per-call
//!   directory per invocation with Drop-guard cleanup; [`keep`]
//!   (EditTempDir::keep) consumes it WITHOUT deleting — the
//!   fail-closed recovery path keeps the user's edit on disk.
//! - [`edit_pipeline`] / [`StagedEdit`] — the pipeline itself
//!   (fetch → decode → edit → content-decided no-op/encode →
//!   staleness gate → staged push payload), documented at the
//!   function.
//!
//! Editor resolution order and the arg-vector contract are
//! planner-locked: `VISUAL` beats `EDITOR`; IDE-style editors ride
//! inside the variable itself (`EDITOR="code --wait"` splits to the
//! right argv with the target last). The CLI documents this (13-08).

use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// The editor seam: open one file path in the user's editor.
///
/// Contract:
/// - the implementation spawns the editor with an ARG VECTOR (the
///   target path is an argument, never interpolated into a shell
///   string);
/// - a successful return means "the editor process finished" — it
///   says NOTHING about whether the content changed (the exit status
///   is advisory; the pipeline decides by content);
/// - a spawn failure (the editor binary is missing/unrunnable) is
///   the usage-class [`CoreError::InvalidInput`] naming the editor —
///   a missing binary is a user-env problem, not transport.
#[async_trait::async_trait]
pub trait Editor: Send + Sync {
    /// Open `path` in the editor and wait for the editor process to
    /// exit (however it exits).
    async fn open(&self, path: &Path) -> Result<(), CoreError>;
}

/// The real editor: resolve `VISUAL` → `EDITOR` from the process
/// environment and spawn it via [`tokio::process::Command`] with an
/// arg vector.
pub struct TokioEditor;

impl TokioEditor {
    /// Resolve the editor command string from the environment:
    /// `VISUAL` first, then `EDITOR`; trimmed; an empty value counts
    /// as unset. Absent both → the planner-locked refusal (no
    /// platform fallback — `open -t -W` semantics vary; the CLI
    /// documents setting `EDITOR` instead, 13-08).
    pub fn resolve_command() -> Result<String, CoreError> {
        let visual = std::env::var("VISUAL").ok();
        let editor = std::env::var("EDITOR").ok();
        Self::resolve_from(visual.as_deref(), editor.as_deref())
    }

    /// The pure resolution rule (env-free, unit-testable): `VISUAL`
    /// beats `EDITOR`; a trimmed-empty value counts as unset; both
    /// unset → the "set $EDITOR" refusal.
    pub fn resolve_from(visual: Option<&str>, editor: Option<&str>) -> Result<String, CoreError> {
        for candidate in [visual, editor].into_iter().flatten() {
            let trimmed = candidate.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        Err(CoreError::InvalidInput {
            reason: "no $EDITOR set — export VISUAL or EDITOR (IDE users: include \
                     the --wait flag, e.g. EDITOR='code --wait')"
                .to_string(),
        })
    }

    /// The arg vector for `command` opening `path`: the command
    /// splits on whitespace (so `code --wait` becomes
    /// `["code", "--wait"]`) and the target path is appended LAST.
    /// Never a shell string.
    pub fn arg_vector(command: &str, path: &Path) -> Vec<String> {
        let mut argv: Vec<String> = command.split_whitespace().map(str::to_string).collect();
        argv.push(path.to_string_lossy().into_owned());
        argv
    }
}

#[async_trait::async_trait]
impl Editor for TokioEditor {
    async fn open(&self, path: &Path) -> Result<(), CoreError> {
        let command = Self::resolve_command()?;
        let argv = Self::arg_vector(&command, path);
        let (program, flags) = argv.split_first().expect("arg_vector is never empty");
        run_editor_argv(program, flags, path).await
    }
}

/// THE single spawn site: run `program` with `flags` plus `path`
/// (appended last) through [`tokio::process::Command`] — ARG VECTOR
/// only, never a shell string (lint.rs:113-121 precedent;
/// injection-safe). A spawn failure is the usage-class refusal
/// naming the editor. A non-zero exit is SUCCESS here — the exit
/// status is advisory (Pitfall E1); whether anything changed is
/// decided by content downstream.
pub async fn run_editor_argv(
    program: &str,
    flags: &[String],
    path: &Path,
) -> Result<(), CoreError> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(flags).arg(path);
    let status = cmd.status().await.map_err(|err| CoreError::InvalidInput {
        reason: format!("editor {program:?} could not run: {err} — is it on PATH?"),
    })?;
    tracing::debug!(%status, "editor exited (exit is advisory — content decides)");
    Ok(())
}

/// One private edit tree per `ign edit` invocation: a
/// [`tempfile::TempDir`] created with 0700 permissions on unix
/// (unique per call), removed by its Drop guard — and
/// [`EditTempDir::keep`] consumes it WITHOUT deleting, the
/// fail-closed recovery path that leaves the user's edit on disk
/// with its path printed in the error.
pub struct EditTempDir(tempfile::TempDir);

impl EditTempDir {
    /// Create the private directory (0700 on unix).
    pub fn new() -> Result<Self, CoreError> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("ign-edit-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let dir = builder.tempdir().map_err(|err| {
            CoreError::Internal(format!("cannot create the private edit directory: {err}"))
        })?;
        Ok(Self(dir))
    }

    /// The tree root (the decoded export lives here).
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    /// Consume the guard WITHOUT deleting — returns the tree's path.
    /// The fail-closed recovery path: on a refused re-encode the
    /// caller keeps the tree and prints this path in the error, so
    /// the user's edit survives the failed run.
    pub fn keep(self) -> PathBuf {
        self.0.keep()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Editor resolution matrix (env-free) -------------------

    #[test]
    fn visual_beats_editor() {
        let resolved =
            TokioEditor::resolve_from(Some("vim"), Some("nano")).expect("visual resolves");
        assert_eq!(resolved, "vim");
    }

    #[test]
    fn editor_used_when_visual_unset() {
        let resolved = TokioEditor::resolve_from(None, Some("nano")).expect("editor resolves");
        assert_eq!(resolved, "nano");
    }

    #[test]
    fn all_empty_values_refuse() {
        let err = TokioEditor::resolve_from(Some(""), Some("   ")).expect_err("refuses");
        let CoreError::InvalidInput { reason } = err else {
            panic!("expected InvalidInput, got {err:?}");
        };
        assert!(reason.contains("no $EDITOR set"), "stable prefix: {reason}");
    }

    #[test]
    fn empty_visual_falls_through_to_editor() {
        let resolved = TokioEditor::resolve_from(Some("  "), Some("nano"))
            .expect("trimmed-empty VISUAL is unset → EDITOR wins");
        assert_eq!(resolved, "nano");
    }

    #[test]
    fn both_unset_refuses_with_the_set_editor_message() {
        let err = TokioEditor::resolve_from(None, None).expect_err("refuses");
        let CoreError::InvalidInput { reason } = err else {
            panic!("expected InvalidInput, got {err:?}");
        };
        assert!(
            reason.contains("no $EDITOR set"),
            "stable prefix missing: {reason}"
        );
        assert!(
            reason.contains("EDITOR='code --wait'"),
            "IDE hint: {reason}"
        );
    }

    // ---- Arg vector (never a shell string) ----------------------

    #[test]
    fn ide_flag_editor_splits_with_path_last() {
        let argv = TokioEditor::arg_vector("code --wait", Path::new("/tmp/x/view.json"));
        assert_eq!(argv, vec!["code", "--wait", "/tmp/x/view.json"]);
    }

    #[test]
    fn plain_editor_is_program_then_path() {
        let argv = TokioEditor::arg_vector("vim", Path::new("/tmp/x/view.json"));
        assert_eq!(argv, vec!["vim", "/tmp/x/view.json"]);
        assert_eq!(argv.last().expect("path last"), "/tmp/x/view.json");
    }

    // ---- EditTempDir lifecycle ----------------------------------

    #[cfg(unix)]
    #[test]
    fn temp_dir_mode_is_private_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = EditTempDir::new().expect("temp dir");
        let mode = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "the edit tree is private");
    }

    #[test]
    fn temp_dirs_are_unique_per_call() {
        let a = EditTempDir::new().expect("a");
        let b = EditTempDir::new().expect("b");
        assert_ne!(a.path(), b.path(), "unique per invocation");
    }

    #[test]
    fn keep_preserves_the_tree() {
        let dir = EditTempDir::new().expect("temp dir");
        let file = dir.path().join("member.json");
        std::fs::write(&file, b"{}").expect("seed file");
        let kept = dir.keep();
        assert!(kept.exists(), "keep() does not delete");
        assert!(file.exists(), "the edit survives");
        std::fs::remove_dir_all(&kept).expect("test cleanup");
    }
}
