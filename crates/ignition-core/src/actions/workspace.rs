//! Workspace actions (13-03) — the `.ign-workspace.json` manifest
//! (the three-way-compare state for `ign workspace status`/`push`,
//! 13-06) and the checkout action that turns a gateway project
//! export into a mapped, hash-recorded local directory tree.
//!
//! ## The manifest is the workspace's identity — RECORDED, never
//! recomputed
//!
//! [`crate::client::workspace::build_mapping`] runs ONCE at checkout
//! (Pitfall W1); the manifest stores the gateway↔local path pairs
//! plus the checkout-time descriptor-normalized FNV-1a hash of every
//! member. status/push READ this file — re-deriving the mapping or
//! the hashes downstream is the documented anti-pattern. The
//! manifest is COMMITTED (git): checkout also writes a `.gitignore`
//! listing `scripts-manifest.json` and the `*.py` sidecar pattern
//! (codec artifacts, not source — the codec tree convention).
//!
//! Envelope discipline: serde field order is struct order (stable),
//! and any FUTURE optional field rides
//! `#[serde(skip_serializing_if = "Option::is_none")]` (the
//! `loss_report` precedent) so pre-existing envelopes stay
//! byte-identical. Read ownership is strict: a missing, foreign, or
//! unparseable manifest REFUSES with a stable message — the prefixes
//! below are 13-07's golden anchors, never reword casually.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// The workspace manifest's filename at the tree root — dot-prefixed
/// so it can never collide with a member-derived path, and DISTINCT
/// from the codec's `scripts-manifest.json` (`encode_export_tree`
/// strips only its own manifest; verified scripts_codec.rs).
pub const WORKSPACE_MANIFEST_NAME: &str = ".ign-workspace.json";

/// The only manifest schema this code reads. A mismatch refuses —
/// never guess at a foreign schema.
pub const WORKSPACE_MANIFEST_SCHEMA_VERSION: u8 = 1;

/// `.ign-workspace.json` — the recorded checkout state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    /// Manifest format version ([`WORKSPACE_MANIFEST_SCHEMA_VERSION`]).
    pub schema_version: u8,
    /// The gateway project the workspace checks out.
    pub project: String,
    /// The profile name checkout resolved against (13-07 passes it
    /// down; recorded for status/push context, never re-derived).
    pub profile: String,
    /// Checkout time, RFC3339 UTC (`…Z`).
    pub checked_out_at: String,
    /// Gateway member path → its recorded checkout facts. BTreeMap:
    /// deterministic serialization.
    pub members: BTreeMap<String, ManifestMember>,
}

/// One member's recorded checkout facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestMember {
    /// The member's local path relative to the workspace root — the
    /// [`crate::client::workspace::build_mapping`] output, recorded
    /// verbatim (the mapping IS the fs truth; downstream never
    /// re-derives).
    pub local_path: String,
    /// The checkout-time content hash — the zip-side,
    /// descriptor-normalized FNV-1a value (`member_hashes`
    /// semantics: a `resource.json` member hashes its normalized
    /// descriptor, so gateway-side `lastModification` volatility
    /// never masquerades as drift). Identical semantics to the
    /// Tree-side hash for identical bytes (13-02 equivalence pin).
    pub hash: u64,
}

/// Read and validate the workspace manifest at `root`/
/// [`WORKSPACE_MANIFEST_NAME`]. Strict read ownership — every
/// failure is `invalid_input` (exit 2) with a STABLE message prefix
/// (13-07's golden anchors):
///
/// - missing file → `not an ign workspace — run \`ign workspace
///   checkout\` first` + the expected path;
/// - wrong `schema_version` → names found vs expected;
/// - unparseable JSON → names the path (never a panic, never a
///   silent default).
pub fn read_manifest(root: &Path) -> Result<WorkspaceManifest, CoreError> {
    let path = root.join(WORKSPACE_MANIFEST_NAME);
    let bytes = std::fs::read(&path).map_err(|_| CoreError::InvalidInput {
        reason: format!(
            "not an ign workspace — run `ign workspace checkout` first \
             (expected manifest at {})",
            path.display()
        ),
    })?;
    let manifest: WorkspaceManifest =
        serde_json::from_slice(&bytes).map_err(|err| CoreError::InvalidInput {
            reason: format!("{} is not valid JSON: {err}", path.display()),
        })?;
    if manifest.schema_version != WORKSPACE_MANIFEST_SCHEMA_VERSION {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "unsupported workspace manifest schema_version {} (expected \
                 {}) at {}",
                manifest.schema_version,
                WORKSPACE_MANIFEST_SCHEMA_VERSION,
                path.display()
            ),
        });
    }
    Ok(manifest)
}

/// Write the workspace manifest at `root`/`​.ign-workspace.json`:
/// pretty JSON (stable field order via struct order), trailing
/// newline, parents created, `0640` perms (unix — it is committed,
/// not secret, but not world-readable either).
pub fn write_manifest(root: &Path, manifest: &WorkspaceManifest) -> Result<(), CoreError> {
    let path = root.join(WORKSPACE_MANIFEST_NAME);
    std::fs::create_dir_all(root).map_err(|err| {
        CoreError::Internal(format!(
            "cannot create workspace root {}: {err}",
            root.display()
        ))
    })?;
    let mut body = serde_json::to_vec_pretty(manifest).map_err(|err| {
        CoreError::Internal(format!("cannot serialize workspace manifest: {err}"))
    })?;
    body.push(b'\n');
    std::fs::write(&path, body)
        .map_err(|err| CoreError::Internal(format!("cannot write {}: {err}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).map_err(|err| {
            CoreError::Internal(format!(
                "cannot set permissions on {}: {err}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    fn sample() -> WorkspaceManifest {
        let mut members = BTreeMap::new();
        members.insert(
            "com.example/views/Dashboard/view.json".to_string(),
            ManifestMember {
                local_path: "com.example/views/Dashboard/view.json".to_string(),
                hash: 0xDEAD_BEEF,
            },
        );
        members.insert(
            "ignition/script-python/e2e/scratch".to_string(),
            ManifestMember {
                local_path: "ignition/script-python/e2e/scratch".to_string(),
                hash: 42,
            },
        );
        WorkspaceManifest {
            schema_version: WORKSPACE_MANIFEST_SCHEMA_VERSION,
            project: "My Proj".to_string(),
            profile: "rig".to_string(),
            checked_out_at: "2026-09-15T02:25:19.000Z".to_string(),
            members,
        }
    }

    /// Round-trip: write → read reproduces the manifest exactly
    /// (BTreeMap + struct order keep serialization deterministic).
    #[test]
    fn manifest_round_trips_write_read() {
        let root = tempfile::tempdir().expect("tempdir");
        let manifest = sample();
        write_manifest(root.path(), &manifest).expect("writes");
        let read = read_manifest(root.path()).expect("reads back");
        assert_eq!(read, manifest, "write→read is lossless");

        // Deterministic serialization: two writes are byte-identical.
        let first = std::fs::read(root.path().join(WORKSPACE_MANIFEST_NAME)).expect("read 1");
        write_manifest(root.path(), &manifest).expect("rewrites");
        let second = std::fs::read(root.path().join(WORKSPACE_MANIFEST_NAME)).expect("read 2");
        assert_eq!(first, second, "serialization is deterministic");
        assert!(
            first.ends_with(b"\n"),
            "the manifest file ends with a trailing newline"
        );
    }

    /// THE missing-manifest golden anchor (13-07 pins this prefix):
    /// a directory without `.ign-workspace.json` is not a workspace.
    #[test]
    fn missing_manifest_refuses_with_stable_prefix() {
        let root = tempfile::tempdir().expect("tempdir");
        let err = read_manifest(root.path()).expect_err("missing refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        assert_eq!(err.exit_code(), 2);
        let message = err.to_string();
        assert!(
            message.contains("not an ign workspace — run `ign workspace checkout` first"),
            "stable prefix missing: {message}"
        );
        assert!(
            message.contains(WORKSPACE_MANIFEST_NAME),
            "names the expected path: {message}"
        );
    }

    /// A foreign schema version refuses, naming found vs expected.
    #[test]
    fn schema_version_mismatch_refuses() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut manifest = sample();
        manifest.schema_version = 99;
        write_manifest(root.path(), &manifest).expect("writes foreign version");
        let err = read_manifest(root.path()).expect_err("mismatch refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        let message = err.to_string();
        assert!(
            message.contains("schema_version 99") && message.contains("expected 1"),
            "names found vs expected: {message}"
        );
    }

    /// Unparseable JSON refuses (never a panic, never a silent
    /// default) and names the path.
    #[test]
    fn corrupt_manifest_refuses() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join(WORKSPACE_MANIFEST_NAME), b"{not json").expect("writes");
        let err = read_manifest(root.path()).expect_err("corrupt refuses");
        assert!(matches!(err, CoreError::InvalidInput { .. }), "{err}");
        let message = err.to_string();
        assert!(
            message.contains("is not valid JSON"),
            "stable corruption prefix missing: {message}"
        );
    }

    /// The manifest file lands at `0640` (unix) — committed, not
    /// world-readable.
    #[cfg(unix)]
    #[test]
    fn manifest_writes_0640() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().expect("tempdir");
        write_manifest(root.path(), &sample()).expect("writes");
        let mode = root
            .path()
            .join(WORKSPACE_MANIFEST_NAME)
            .metadata()
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o640, "manifest mode is 0640");
    }
}
