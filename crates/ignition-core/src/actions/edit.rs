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

use serde::Serialize;

use crate::actions::resources::export_zip_bytes;
use crate::client::GatewayApi;
use crate::client::resources::{
    MemberStatus, diff_members, member_hashes, member_path, resource_members,
};
use crate::client::scripts_codec::{decode_export_tree, encode_export_tree};
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

// ---- The edit pipeline (13-05) ----------------------------------------------

/// The pipeline verdict. `NoOp` means the editor changed NOTHING —
/// decided by CONTENT (byte-identical re-encode), never by the
/// editor's exit code. `Ready` carries the member-level blast
/// radius: `changed` lists every resource member the staged push
/// would write ([`diff_members`]' B-relative non-same paths, with
/// the ORIGINAL export as A and the re-encode as B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EditStatus {
    /// Nothing changed — the caller must NOT push and must NOT
    /// prompt (no guard, no confirmation, a silent clean exit).
    NoOp,
    /// An edit is staged; `changed` = what a push would write.
    Ready {
        /// Resource paths the staged import zip would change.
        changed: Vec<String>,
    },
}

/// The pipeline's terminal product: the staged edit the CALLER (the
/// 13-08 CLI, via the guard ladder) decides what to do with. Push is
/// deliberately OUT of core's signature — the pipeline ENDS at the
/// staged payload, keeping gate composition at the dispatch layer
/// (the 10-04 preview_then_confirm lesson: one gate site).
/// `import_zip` is `None` EXACTLY when `status` is `NoOp` — there
/// are no bytes to push, so the caller cannot push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StagedEdit {
    /// The project the edit targets (the push destination).
    pub project: String,
    /// The verdict + blast radius.
    pub status: EditStatus,
    /// The re-encoded import zip — `Some` only on `Ready`.
    pub import_zip: Option<Vec<u8>>,
}

/// `ign edit`'s core loop (SC-5's mechanics), in order:
///
/// 1. **fetch** — one project export (`export_zip_bytes`).
/// 2. **snapshot** — `member_hashes` of the fetch (descriptor-
///    normalized; the staleness baseline).
/// 3. **decode** — the WHOLE tree into a fresh private
///    [`EditTempDir`] (`decode_export_tree`; the encode-back needs
///    full context even though `resource_path` scopes the editor).
/// 4. **baseline** — re-encode a SECOND decode of the SAME export
///    untouched. This — not the raw original zip container — is the
///    no-op comparator (planner lock: "the ORIGINAL export zip's
///    re-encode"): our zip writer sorts and re-compresses, so
///    container bytes differ from the gateway's even with identical
///    member content; what is byte-stable is encode-vs-encode of
///    identical trees (13-03's member-level round-trip invariant,
///    made whole-tree by determinism). Computing it BEFORE the
///    editor runs means a codec failure on an unedited tree refuses
///    before burning the user's editing session.
/// 5. **target** — `resource_path` (a USER path) must be a resource
///    member; its decoded file is `member_path(user)` inside the
///    edit tree. Its sidecars (`<member>.<n>.py`, embedded scripts)
///    decode beside it and splice back through the manifest — the
///    editor opens the member file; untouched sidecars re-encode
///    byte-identically. `None` refuses with the member list:
///    editing "the whole tree" is `ign workspace checkout`'s job —
///    edit is ONE resource.
/// 6. **edit** — `Editor::open(target)`; the exit status is
///    advisory (Pitfall E1).
/// 7. **re-encode** — `encode_export_tree`, FAIL-CLOSED: a member
///    broken in the editor refuses via the codec's own
///    [`encode_member`] `InvalidInput` (verbatim), and the edit tree
///    is KEPT — the error message carries its path so the user can
///    recover the edit.
/// 8. **no-op** — the re-encoded bytes equal the baseline ⇒
///    [`EditStatus::NoOp`] with `import_zip: None`. Content
///    decided; no push, no prompt, no guard.
/// 9. **staleness** — a FRESH export + the target member's hash vs
///    the fetch snapshot; drift refuses with
///    `resource "<path>" changed on gateway since fetch` — NOT
///    `--yes`-able (forcing it would clobber a concurrent Designer
///    edit; re-run to fetch fresh).
/// 10. **stage** — [`EditStatus::Ready`] with the diff summary and
///     `import_zip = Some(re-encoded bytes)`. The pipeline NEVER
///     pushes; the caller does (with the gate of its own).
pub async fn edit_pipeline(
    api: &dyn GatewayApi,
    editor: &dyn Editor,
    project: &str,
    resource_path: Option<&str>,
) -> Result<StagedEdit, CoreError> {
    // 1+2. Fetch + snapshot.
    let zip = export_zip_bytes(api, project).await?;
    let snapshot = member_hashes(&zip)?;

    // 3. Decode the whole tree into the private edit dir.
    let temp = EditTempDir::new()?;
    let scripts = decode_export_tree(&zip, temp.path())?;
    tracing::debug!(scripts, "decoded the export tree for edit");

    // 4. Baseline re-encode of an untouched second decode (see the
    //    doc: this is the no-op comparator, computed pre-edit).
    let baseline = {
        let baseline_dir = EditTempDir::new()?;
        decode_export_tree(&zip, baseline_dir.path())?;
        encode_export_tree(baseline_dir.path())?
    };

    // 5. Resolve the editor target.
    let members = resource_members(&zip)?;
    let Some(user) = resource_path else {
        let list = if members.is_empty() {
            "the project has no resource members".to_string()
        } else {
            format!("valid members: {}", members.join(", "))
        };
        return Err(CoreError::InvalidInput {
            reason: format!(
                "edit needs one resource to edit — the whole tree is \
                 `ign workspace checkout`'s job ({list})"
            ),
        });
    };
    if !members.iter().any(|member| member == user) {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "\"{user}\" is not a resource member of {project} — valid members: {}",
                members.join(", ")
            ),
        });
    }
    let target = temp.path().join(member_path(user));
    if !target.exists() {
        // Defensive (the membership check above already gates this):
        // a resource member missing from its own decode tree is an
        // export-contract violation, not user error.
        return Err(CoreError::Internal(format!(
            "resource member \"{user}\" decoded to nothing — the export \
             zip is inconsistent with its member list"
        )));
    }

    // 6. Edit. Exit status is advisory; content decides.
    editor.open(&target).await?;

    // 7. Re-encode, fail-closed: the codec's own InvalidInput rides
    //    VERBATIM (the bare reason, not a re-wrapped display), and
    //    the edit tree is KEPT (its path rides the message) so the
    //    user's edit survives the failed run.
    let reencoded = match encode_export_tree(temp.path()) {
        Ok(bytes) => bytes,
        Err(err) => {
            let codec_reason = match err {
                CoreError::InvalidInput { reason } => reason,
                other => other.to_string(),
            };
            let kept = temp.keep();
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "{codec_reason}; the edit tree is preserved at {} — fix \
                     the member by hand and re-run to retry",
                    kept.display()
                ),
            });
        }
    };

    // 8. The no-op decision: CONTENT, never the exit code.
    if reencoded == baseline {
        return Ok(StagedEdit {
            project: project.to_string(),
            status: EditStatus::NoOp,
            import_zip: None,
        });
    }

    // 9. Staleness gate — fresh export, target-member hash vs the
    //    fetch snapshot. Reuses member_hashes unchanged (invents NO
    //    etag); NOT --yes-able (planner lock, Pitfall E2).
    let fresh = export_zip_bytes(api, project).await?;
    let fresh_hashes = member_hashes(&fresh)?;
    if fresh_hashes.get(user) != snapshot.get(user) {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "resource \"{user}\" changed on gateway since fetch — \
                 re-run to fetch fresh"
            ),
        });
    }

    // 10. Stage: Ready with the member-level blast radius.
    let diff = diff_members(&zip, &reencoded)?;
    let changed: Vec<String> = diff
        .entries
        .into_iter()
        .filter(|entry| entry.status != MemberStatus::Same)
        .map(|entry| entry.path)
        .collect();
    Ok(StagedEdit {
        project: project.to_string(),
        status: EditStatus::Ready { changed },
        import_zip: Some(reencoded),
    })
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
