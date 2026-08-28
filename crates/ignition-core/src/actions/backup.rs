//! Standalone gwbk backup actions (07-02, BKUP-01) — the Phase 4
//! client methods surfaced on ANY profiled gateway (not just rigs).
//!
//! Pure orchestration: the wire shipped in 04-04 (the streamed
//! download through `download_to_file`, the raw octet-stream restore
//! POST with four explicit-false scope params) — this layer owns only
//! the default output naming (the project-export `.part` rename
//! pattern: stream to a fallback name, rename to the
//! Content-Disposition basename when the gateway sends one) and the
//! usage-class file pre-checks the rig restore established (a
//! nonexistent/empty/directory `--file` refuses exit 2 BEFORE any
//! network work).
//!
//! The 8th `--yes`-guarded destructive verb lives at the CLI seam
//! (`backup restore`, main.rs — guard BEFORE resolution, the
//! sessions-terminate shape); the actions here stay unguarded
//! (caller-owns-guard). The post-restore restart-block window is a
//! README truth, not output data (Pitfall 6).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::backup::BackupType;
use crate::error::CoreError;

/// `ign backup download` output model — all keys always.
#[derive(Debug, Serialize)]
pub struct BackupDownloadResult {
    /// The file written (as resolved: the `-o` override, the
    /// gateway's Content-Disposition basename, or the
    /// `<stem>-backup.gwbk` fallback).
    pub file: String,
    /// The backup type requested (`roaming` | `all` — the wire
    /// value, agent-stable).
    pub r#type: String,
}

/// `ign backup restore` output model — the flat success shape.
#[derive(Debug, Serialize)]
pub struct BackupRestoreResult {
    /// Always `true` on this shape — the POST's 2xx IS the restore
    /// acceptance (the post-restore restart window is README
    /// honesty, not output).
    pub restored: bool,
}

/// A filesystem-safe fallback stem (the project-export defense —
/// profile names are config-controlled, still never trusted into a
/// path with separators intact).
fn safe_stem(stem: &str) -> String {
    stem.replace(['/', '\\'], "_")
}

/// Strip any path components from a Content-Disposition filename —
/// the gateway names a basename, defense-in-depth makes it true (the
/// project-export sanitizer's twin).
fn sanitize_basename(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.replace(['/', '\\'], "_"))
}

/// `ign backup download` — stream the gwbk to disk. Default naming
/// rides the project-export `.part` pattern: stream to
/// `<stem>-backup.gwbk.part`, rename to the disposition basename (or
/// the fallback) once the metadata arrives; a failed download leaves
/// no half-written impostor.
pub async fn backup_download(
    api: &dyn GatewayApi,
    out: Option<&Path>,
    host_stem: &str,
    backup_type: BackupType,
) -> Result<BackupDownloadResult, CoreError> {
    let wire_type = backup_type.wire().to_string();
    if let Some(out) = out {
        api.backup_download(out, backup_type).await?;
        return Ok(BackupDownloadResult {
            file: out.display().to_string(),
            r#type: wire_type,
        });
    }

    // Default naming: the export convention (disposition basename
    // wins; the sanitized fallback names the gateway).
    let fallback = format!("{}-backup.gwbk", safe_stem(host_stem));
    let part = PathBuf::from(format!("{fallback}.part"));
    let meta = match api.backup_download(&part, backup_type).await {
        Ok(meta) => meta,
        Err(err) => {
            let _ = std::fs::remove_file(&part); // best-effort
            return Err(err);
        }
    };
    let final_name = meta
        .filename
        .as_deref()
        .and_then(sanitize_basename)
        .unwrap_or(fallback);
    if let Err(err) = std::fs::rename(&part, &final_name) {
        let _ = std::fs::remove_file(&part); // best-effort
        return Err(CoreError::Internal(format!(
            "cannot finalize backup {final_name}: {err}"
        )));
    }
    Ok(BackupDownloadResult {
        file: final_name,
        r#type: wire_type,
    })
}

/// `ign backup restore` — thin orchestration over the 04-04 trait
/// method: usage-class file pre-checks (the `rig restore` shape:
/// exists + regular + non-empty, exit 2 BEFORE any network work),
/// then the raw octet-stream POST. The 2xx is ACCEPTANCE; the
/// gateway restarts after answering (README-documented).
pub async fn backup_restore(
    api: &dyn GatewayApi,
    gwbk: &Path,
) -> Result<BackupRestoreResult, CoreError> {
    let meta = std::fs::metadata(gwbk).map_err(|_| CoreError::InvalidInput {
        reason: format!("gwbk file {} not found", gwbk.display()),
    })?;
    if !meta.is_file() {
        return Err(CoreError::InvalidInput {
            reason: format!("gwbk file {} is not a regular file", gwbk.display()),
        });
    }
    if meta.len() == 0 {
        return Err(CoreError::InvalidInput {
            reason: format!("gwbk file {} is empty", gwbk.display()),
        });
    }

    api.backup_restore(gwbk).await?;
    Ok(BackupRestoreResult { restored: true })
}

#[cfg(test)]
mod tests {
    use super::{safe_stem, sanitize_basename};

    /// The basename sanitizer never lets a separator through (the
    /// project-export defense, backup edition).
    #[test]
    fn basename_sanitizer_strips_separators() {
        assert_eq!(
            sanitize_basename("backup.gwbk").as_deref(),
            Some("backup.gwbk")
        );
        assert_eq!(
            sanitize_basename("../../etc/passwd").as_deref(),
            Some(".._.._etc_passwd")
        );
        assert_eq!(sanitize_basename("a\\b.gwbk").as_deref(), Some("a_b.gwbk"));
        assert_eq!(sanitize_basename("   "), None, "blank names nothing");
    }

    /// The fallback stem sanitizes the same way.
    #[test]
    fn fallback_stem_sanitizes() {
        assert_eq!(safe_stem("gw/dev"), "gw_dev");
        assert_eq!(safe_stem("plain"), "plain");
    }
}
