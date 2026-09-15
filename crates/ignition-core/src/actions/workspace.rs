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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::actions::resources::export_zip_bytes;
use crate::client::GatewayApi;
use crate::client::resources::{
    FOLDER_DESCRIPTOR, fnv1a, member_hashes, normalize_descriptor, read_member, resource_members,
};
use crate::client::scripts_codec;
use crate::client::workspace::{MemberSource, build_mapping};
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

// ---- checkout (13-03 Task 2) -------------------------------------------------

/// The lines checkout's `.gitignore` owns at the tree root (planner
/// lock: the manifest is COMMITTED; the codec artifacts — the decode
/// manifest and the `*.py` sidecars — are not source).
const WORKSPACE_GITIGNORE_LINES: [&str; 2] = [scripts_codec::MANIFEST_NAME, "*.py"];

/// Root-level file names checkout itself owns — a mapped member may
/// never land on one of them (the checkout would shadow the member
/// or the member would shadow workspace machinery).
const RESERVED_ROOT_NAMES: [&str; 4] = [
    WORKSPACE_MANIFEST_NAME,
    scripts_codec::MANIFEST_NAME,
    ".gitignore",
    "project.json",
];

/// `ign workspace checkout`'s result model (serde for 13-07's
/// envelope — actions never print, ARCHITECTURE.md layering).
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutOutcome {
    /// The project checked out.
    pub project: String,
    /// The tree root the workspace landed in.
    pub target: PathBuf,
    /// How many resource members were written (the manifest's
    /// member count).
    pub member_count: usize,
    /// Whether the `--decode-scripts` codec leg ran (sidecars +
    /// `scripts-manifest.json` present).
    pub scripts_decoded: bool,
}

/// `ign workspace checkout PROJECT DIR [--decode-scripts]`'s core:
/// export the project zip → map members through
/// [`build_mapping`] ONCE → write every member's bytes at its
/// MAPPED local path (never `path.join` on the raw member name —
/// the mapping IS the fs truth) → record `.ign-workspace.json`
/// (gateway↔local pairs + zip-side descriptor-normalized hashes)
/// → write the idempotent `.gitignore`.
///
/// `--decode-scripts` rides the codec's own decode engine
/// ([`scripts_codec::decode_member`] — no new script extraction):
/// decoded scripts land as `<member>.<n>.py` sidecars beside their
/// member's mapped path, and `scripts-manifest.json` at the tree
/// root keys members by their MAPPED tree-relative paths — the
/// layout [`scripts_codec::encode_export_tree`] consumes, so an
/// UNEDITED tree re-encodes BYTE-EXACTLY (per member; the codec's
/// sacred invariant proven at tree scale — pinned in
/// `tests/workspace_checkout.rs`).
///
/// Refusals (all `invalid_input`, exit 2, before ANY write):
/// - a non-empty target that is not a valid workspace of the SAME
///   project (never clobbers an unrelated directory; a corrupt or
///   foreign manifest refuses naming what it found);
/// - mapping refusals propagate verbatim from [`build_mapping`]
///   (case-fold collisions name BOTH members — 13-02);
/// - a member mapping onto a reserved root name
///   ([`RESERVED_ROOT_NAMES`]).
///
/// `profile` is recorded into the manifest (planner-locked field;
/// 13-07 passes the resolved profile name down — the action layer
/// cannot read config itself, so the caller owns it).
///
/// Read-only on the wire: exactly ONE export GET, ZERO imports.
pub async fn workspace_checkout(
    api: &dyn GatewayApi,
    project: &str,
    target_dir: &Path,
    profile: &str,
    decode_scripts: bool,
) -> Result<CheckoutOutcome, CoreError> {
    // Export — the ONE transport (`project_export_to_file` via the
    // shared export-to-bytes seam; a nonexistent project surfaces
    // through export's existing 404 path, `not_found` exit 6).
    let zip = export_zip_bytes(api, project).await?;

    // Clobber refusal FIRST (before any write — the zip may already
    // be in memory, but the target is untouched until it is proven
    // safe).
    ensure_recheckout_safe(target_dir, project)?;

    // Map once — 13-02's refusals propagate verbatim (traversal
    // shapes, NUL, oversize segments, case-fold collisions naming
    // both members, duplicates).
    let members = resource_members(&zip)?;
    let mapping = build_mapping(&members)?;

    // Reserved root names: the checkout owns these files; a member
    // landing on one would shadow (or be shadowed by) workspace
    // machinery.
    for (user, local) in &mapping {
        let root_name = local.file_name().and_then(|name| name.to_str());
        let root_level = local
            .parent()
            .is_none_or(|parent| parent.as_os_str().is_empty());
        if root_level && root_name.is_some_and(|name| RESERVED_ROOT_NAMES.contains(&name)) {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "workspace member \"{user}\" maps onto reserved workspace file \
                     \"{}\" — the checkout owns this name; refusing",
                    local.display()
                ),
            });
        }
    }

    std::fs::create_dir_all(target_dir).map_err(|err| {
        CoreError::Internal(format!(
            "cannot create workspace root {}: {err}",
            target_dir.display()
        ))
    })?;

    let mapped: BTreeSet<&PathBuf> = mapping.values().collect();
    let mut codec_manifest = scripts_codec::Manifest {
        version: 1,
        members: BTreeMap::new(),
    };

    for (user, local) in &mapping {
        // The proven zip-member read (the same engine
        // `MemberSource::Zip` delegates to verbatim).
        let bytes = read_member(&zip, user)?;
        let dest = target_dir.join(local);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                CoreError::Internal(format!("cannot create {}: {err}", parent.display()))
            })?;
        }
        std::fs::write(&dest, &bytes).map_err(|err| {
            CoreError::Internal(format!("cannot write {}: {err}", dest.display()))
        })?;

        // The codec leg — the codec's OWN decode engine, unchanged.
        // `decode_member` uses the member path only for the sidecar
        // BASENAME (identical for user and raw member paths).
        if decode_scripts && let Some(decoded) = scripts_codec::decode_member(&bytes, user) {
            let mut entries = Vec::with_capacity(decoded.entries.len());
            for decoded_entry in decoded.entries {
                let sidecar_local = match local.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => {
                        parent.join(&decoded_entry.entry.sidecar)
                    }
                    _ => PathBuf::from(&decoded_entry.entry.sidecar),
                };
                if mapped.contains(&sidecar_local) {
                    return Err(CoreError::InvalidInput {
                        reason: format!(
                            "sidecar \"{}\" of member \"{user}\" collides \
                             with a real checkout member — refusing to shadow it",
                            sidecar_local.display()
                        ),
                    });
                }
                let sidecar_dest = target_dir.join(&sidecar_local);
                std::fs::write(&sidecar_dest, &decoded_entry.text).map_err(|err| {
                    CoreError::Internal(format!("cannot write {}: {err}", sidecar_dest.display()))
                })?;
                entries.push(decoded_entry.entry);
            }
            // Keyed by the MAPPED tree-relative path — the exact key
            // `encode_export_tree` resolves when it walks the tree.
            codec_manifest
                .members
                .insert(local.to_string_lossy().into_owned(), entries);
        }
    }

    if decode_scripts {
        // The codec manifest, in the codec's own format (pretty +
        // trailing newline — `decode_export_tree`'s byte shape).
        let mut manifest_bytes = serde_json::to_vec_pretty(&codec_manifest).map_err(|err| {
            CoreError::Internal(format!("cannot serialize the decode manifest: {err}"))
        })?;
        manifest_bytes.push(b'\n');
        let manifest_path = target_dir.join(scripts_codec::MANIFEST_NAME);
        std::fs::write(&manifest_path, manifest_bytes).map_err(|err| {
            CoreError::Internal(format!("cannot write {}: {err}", manifest_path.display()))
        })?;
    }

    // Record: zip-side descriptor-normalized hashes (identical
    // semantics to the Tree side for identical bytes — 13-02
    // equivalence pin). The manifest is RECORDED; status/push never
    // re-derive.
    let hashes = member_hashes(&zip)?;
    let mut manifest_members = BTreeMap::new();
    for (user, hash) in hashes {
        let local = &mapping[&user];
        manifest_members.insert(
            user,
            ManifestMember {
                local_path: local.to_string_lossy().into_owned(),
                hash,
            },
        );
    }
    let manifest = WorkspaceManifest {
        schema_version: WORKSPACE_MANIFEST_SCHEMA_VERSION,
        project: project.to_string(),
        profile: profile.to_string(),
        checked_out_at: rfc3339_now_utc(),
        members: manifest_members,
    };
    write_manifest(target_dir, &manifest)?;
    write_gitignore(target_dir)?;

    Ok(CheckoutOutcome {
        project: project.to_string(),
        target: target_dir.to_path_buf(),
        member_count: mapping.len(),
        scripts_decoded: decode_scripts,
    })
}

/// The clobber gate: an EXISTING target must be either empty or a
/// valid workspace of the SAME project (re-checkout over one's own
/// workspace = 13-06's refresh semantics, allowed). Anything else —
/// an unrelated non-empty directory, a file, a corrupt manifest —
/// refuses BEFORE any write.
fn ensure_recheckout_safe(target_dir: &Path, project: &str) -> Result<(), CoreError> {
    if !target_dir.exists() {
        return Ok(()); // fresh checkout
    }
    let entries = std::fs::read_dir(target_dir).map_err(|err| CoreError::InvalidInput {
        reason: format!(
            "target directory {} is not a usable checkout target: {err}",
            target_dir.display()
        ),
    })?;
    if entries.count() == 0 {
        return Ok(()); // empty dir — checkout owns it from here
    }
    match read_manifest(target_dir) {
        Ok(manifest) => {
            if manifest.project != project {
                return Err(CoreError::InvalidInput {
                    reason: format!(
                        "workspace at {} is checked out from project {:?} — refusing \
                         to check out {:?} over it",
                        target_dir.display(),
                        manifest.project,
                        project
                    ),
                });
            }
            Ok(()) // same-project re-checkout: allowed, refreshes
        }
        Err(err) => Err(CoreError::InvalidInput {
            reason: format!(
                "target directory {} is not empty and is not an ign workspace — \
                 refusing to clobber it ({err})",
                target_dir.display()
            ),
        }),
    }
}

/// Write (idempotently) the `.gitignore` at the tree root: the codec
/// manifest + the sidecar pattern. A file already listing both
/// entries is left untouched; missing entries are APPENDED (existing
/// content preserved — never clobber a user's own ignore rules).
fn write_gitignore(root: &Path) -> Result<(), CoreError> {
    let path = root.join(".gitignore");
    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .map(|content| content.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let mut changed = false;
    for entry in WORKSPACE_GITIGNORE_LINES {
        if !lines.iter().any(|line| line.trim() == entry) {
            lines.push(entry.to_string());
            changed = true;
        }
    }
    if !changed {
        return Ok(()); // idempotent
    }
    let mut body = lines.join("\n");
    body.push('\n');
    std::fs::write(&path, body)
        .map_err(|err| CoreError::Internal(format!("cannot write {}: {err}", path.display())))
}

// ---- status (13-06 Task 1) -----------------------------------------------------

/// `ign workspace status`'s result model (serde for 13-07's envelope —
/// actions never print). Envelope discipline: `rows` is ALL-rows-always
/// — every row that exists is present; there are no null placeholders
/// (the sessions-family semantics), and `clean` summarizes so agents
/// never have to walk the rows to know the verdict.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkspaceStatus {
    /// The project the workspace is checked out from (echoed from the
    /// manifest after the caller's `project` argument is verified
    /// against it).
    pub project: String,
    /// True iff every row is [`StatusKind::Clean`] (additions,
    /// deletions, untracked files, and drift all make this false).
    pub clean: bool,
    /// Every row: one per manifest member, then gateway-only members,
    /// then untracked files — sorted within each group (deterministic
    /// render order for 13-07).
    pub rows: Vec<StatusRow>,
}

/// One status row. `path` is the GATEWAY member path for
/// manifest/gateway rows (the manifest's keys) and the local
/// tree-relative path for [`StatusKind::Untracked`] rows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusRow {
    /// The member path the row is about.
    pub path: String,
    /// The verdict — PUSH-RELATIVE (see [`StatusKind`]).
    pub kind: StatusKind,
}

/// The three-way-compare verdict for one path.
///
/// ## Direction semantics — PUSH-RELATIVE (planner lock, load-bearing)
///
/// Every row describes what `workspace push` would do to the
/// GATEWAY. This is the projects.rs:535-555 lesson applied
/// explicitly: `diff` speaks B-relative while sync speaks
/// source→target, and agents misread mixed-direction labels. Here
/// there is exactly one direction, pinned in code and tests:
///
/// - [`StatusKind::LocalEdit`] — the local side changed since
///   checkout and the gateway still matches the recorded baseline:
///   push would WRITE this member to the gateway.
/// - [`StatusKind::GatewayDrift`] — the gateway moved on since
///   checkout and the local side still matches the baseline: push
///   would NOT touch this member (a pull/refresh would).
/// - [`StatusKind::Conflict`] — BOTH sides diverged from the
///   recorded baseline: push REFUSES outright (never `--yes`-able —
///   clobbering a concurrent Designer edit is beyond any flag,
///   Pitfall W2).
/// - [`StatusKind::Clean`] — both sides still match the baseline.
///
/// Set-difference verdicts (not from [`classify`]'s matrix):
///
/// - [`StatusKind::Deleted`] — a manifest member gone from one
///   side. `local: true` = deleted locally (push `--delete` would
///   remove it gateway-side; without `--delete` push reports it as
///   skipped). `local: false` = deleted gateway-side since checkout
///   (pull territory — push would not touch it).
/// - [`StatusKind::Added`] — a member present in the fresh gateway
///   export but not in the manifest (exported since checkout;
///   `local: false` — bringing it into the tree is checkout
///   `--refresh` territory, which is manual in v1).
/// - [`StatusKind::Untracked`] — a local file that is not a
///   checkout member and not workspace machinery. Push IGNORES
///   untracked files (never imports them): bringing a file into
///   the gateway is checkout/replace territory, not push's job.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    /// Both sides match the recorded checkout baseline.
    Clean,
    /// Local changed, gateway at baseline — push would write.
    LocalEdit,
    /// Gateway moved on, local at baseline — push would not touch.
    GatewayDrift,
    /// Both sides diverged — push refuses, never `--yes`-able.
    Conflict,
    /// Member in the fresh export but not in the manifest.
    Added {
        /// Always `false` in v1 (gateway-side additions; checkout
        /// `--refresh` is manual — the payload exists so a future
        /// local-add flow can reuse the kind additively).
        local: bool,
    },
    /// Manifest member gone from one side.
    Deleted {
        /// `true` = deleted locally (push `--delete` territory);
        /// `false` = deleted gateway-side (pull territory).
        local: bool,
    },
    /// Local file that is not a checkout member; push ignores it.
    Untracked,
}

/// THE three-way matrix — pure, total over all 8 `Option`
/// combinations, unit-tested exhaustively. `manifest` is the
/// recorded checkout hash, `local`/`gateway` are `None` when that
/// side lacks the member (local file deleted; member absent from
/// the fresh export). Returns only the four primary kinds — the
/// set-difference verdicts ([`StatusKind::Deleted`]/
/// [`StatusKind::Added`]/[`StatusKind::Untracked`]) are refined by
/// the caller, which knows which side the absence came from.
///
/// None-handling (pinned by test): an absent side never EQUALS a
/// present hash, so it counts as "moved" in the primary matrix —
/// the caller then refines `LocalEdit`-with-local-absent into
/// [`StatusKind::Deleted { local: true }`] and
/// `GatewayDrift`-with-gateway-absent into
/// [`StatusKind::Deleted { local: false }`]. The one special case:
/// a baseline member absent from BOTH sides classifies
/// [`StatusKind::Clean`] — both sides deleted it since checkout, so
/// there is nothing to reconcile (push has nothing to delete — the
/// member is already gone from the fresh export; a refresh
/// re-checkout drops the stale manifest entry).
pub fn classify(manifest: Option<u64>, local: Option<u64>, gateway: Option<u64>) -> StatusKind {
    let Some(m) = manifest else {
        // No recorded baseline — cannot happen for manifest-driven
        // rows in production (the caller only feeds manifest
        // members); totaled for the matrix with the honest verdict:
        // sides that agree need nothing, sides that disagree get the
        // refusal (no arbiter to pick a winner).
        return if local == gateway {
            StatusKind::Clean
        } else {
            StatusKind::Conflict
        };
    };
    if local.is_none() && gateway.is_none() {
        // Baseline member absent from BOTH sides: both deleted it
        // since checkout — nothing to reconcile (push has nothing to
        // delete; the member is already gone from the fresh export).
        return StatusKind::Clean;
    }
    let local_same = local == Some(m);
    let gateway_same = gateway == Some(m);
    match (local_same, gateway_same) {
        (true, true) => StatusKind::Clean,
        (false, true) => StatusKind::LocalEdit,
        (true, false) => StatusKind::GatewayDrift,
        (false, false) => StatusKind::Conflict,
    }
}

/// The tolerant, per-member Tree-equivalent hash — the SAME
/// primitives [`MemberSource::Tree::member_hashes`] uses
/// ([`normalize_descriptor`] for a `resource.json` basename,
/// [`fnv1a`] for the content) read at the RECORDED local path (the
/// manifest is the mapping truth — never re-derived). Status owns
/// the absence verdict, so a `NotFound` is `Ok(None)` (a local
/// deletion, not an error); any OTHER read failure refuses naming
/// the member (a permission problem is not a verdict).
fn local_member_hash(
    root: &Path,
    recorded: &ManifestMember,
    member: &str,
) -> Result<Option<u64>, CoreError> {
    match std::fs::read(root.join(&recorded.local_path)) {
        Ok(bytes) => {
            let is_descriptor = Path::new(&recorded.local_path).file_name()
                == Some(std::ffi::OsStr::new(FOLDER_DESCRIPTOR));
            let content = if is_descriptor {
                normalize_descriptor(&bytes).unwrap_or(bytes)
            } else {
                bytes
            };
            Ok(Some(fnv1a(&content)))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(CoreError::InvalidInput {
            reason: format!(
                "workspace member \"{member}\" cannot be read from the checkout \
                 tree (expected at \"{}\"): {err}",
                recorded.local_path
            ),
        }),
    }
}

/// Local files that are the workspace's OWN machinery or codec
/// artifacts — never reported as [`StatusKind::Untracked`]: the two
/// checkout-owned root files, the codec manifest, and `.py` files
/// (the decode sidecars — the workspace `.gitignore`'s own
/// convention). Anything else in the tree that is not a checkout
/// member is a genuine untracked file the human created.
fn workspace_owned_or_artifact(rel: &str) -> bool {
    rel == WORKSPACE_MANIFEST_NAME
        || rel == ".gitignore"
        || rel == scripts_codec::MANIFEST_NAME
        || rel.ends_with(".py")
}

/// Every regular file under `root`, relative + `/`-separated,
/// EXCLUDING workspace-owned files and recorded member paths — the
/// untracked candidate set. Sorted (BTreeSet) so the rows are
/// deterministic.
fn untracked_files(
    root: &Path,
    member_locals: &BTreeSet<String>,
) -> Result<Vec<String>, CoreError> {
    fn walk(dir: &Path, prefix: &str, found: &mut BTreeSet<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                walk(&entry.path(), &rel, found)?;
            } else if entry.path().is_file() {
                found.insert(rel);
            }
        }
        Ok(())
    }
    let mut found = BTreeSet::new();
    walk(root, "", &mut found).map_err(|err| {
        CoreError::Internal(format!("cannot walk workspace tree {root:?}: {err}"))
    })?;
    Ok(found
        .into_iter()
        .filter(|rel| !workspace_owned_or_artifact(rel) && !member_locals.contains(rel))
        .collect())
}

/// `ign workspace status`'s core: the manifest three-way compare.
/// Reads the RECORDED manifest (strict read ownership — a missing
/// manifest refuses with 13-03's stable prefix), verifies the
/// caller's `project` against it, exports the FRESH gateway zip
/// (ONE read GET — status is read-only on the wire), and classifies
/// every path through the ONE hash/normalize implementation:
/// gateway hashes ride [`MemberSource::Zip::member_hashes`]
/// (descriptor-normalized — `lastModification` volatility never
/// masquerades as drift); local hashes ride the Tree-equivalent
/// per-member hash at the recorded local paths ([`fnv1a`] +
/// [`normalize_descriptor`], never a second implementation). No
/// byte-compare anywhere: `resource.json` members compare
/// descriptor-normalized on both sides.
pub async fn workspace_status(
    root: &Path,
    api: &dyn GatewayApi,
    project: &str,
) -> Result<WorkspaceStatus, CoreError> {
    let manifest = read_manifest(root)?;
    if manifest.project != project {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "workspace at {} is checked out from project {:?} — refusing to \
                 status {:?}",
                root.display(),
                manifest.project,
                project
            ),
        });
    }

    // The fresh gateway side — the ONE transport, descriptor-
    // normalized hashes (identical semantics to checkout's recording).
    let zip = export_zip_bytes(api, project).await?;
    let gateway_hashes = MemberSource::Zip(zip).member_hashes()?;

    // Per-manifest-path matrix + absence refinement.
    let mut rows = Vec::new();
    for (member, recorded) in &manifest.members {
        let local = local_member_hash(root, recorded, member)?;
        let gateway = gateway_hashes.get(member).copied();
        let kind = match classify(Some(recorded.hash), local, gateway) {
            StatusKind::LocalEdit if local.is_none() => StatusKind::Deleted { local: true },
            StatusKind::GatewayDrift if gateway.is_none() => StatusKind::Deleted { local: false },
            other => other,
        };
        rows.push(StatusRow {
            path: member.clone(),
            kind,
        });
    }

    // Set differences: gateway members the manifest never recorded —
    // exported since checkout (pull/refresh territory, never push's).
    for member in gateway_hashes.keys() {
        if !manifest.members.contains_key(member) {
            rows.push(StatusRow {
                path: member.clone(),
                kind: StatusKind::Added { local: false },
            });
        }
    }

    // Untracked local files — reported, never pushed.
    let member_locals: BTreeSet<String> = manifest
        .members
        .values()
        .map(|recorded| recorded.local_path.clone())
        .collect();
    for rel in untracked_files(root, &member_locals)? {
        rows.push(StatusRow {
            path: rel,
            kind: StatusKind::Untracked,
        });
    }

    let clean = rows.iter().all(|row| matches!(row.kind, StatusKind::Clean));
    Ok(WorkspaceStatus {
        project: project.to_string(),
        clean,
        rows,
    })
}

/// Now, RFC3339 UTC (`…Z`, millisecond precision) — hand-rolled
/// (civil-from-days; the tags.rs hand-rolled-parser precedent) so
/// the manifest carries a human-readable checkout timestamp with
/// ZERO new dependencies.
fn rfc3339_now_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the unix epoch");
    unix_ms_to_rfc3339_utc(now.as_millis() as i64)
}

/// Epoch milliseconds → `YYYY-MM-DDTHH:MM:SS.mmmZ` (Howard
/// Hinnant's civil-from-days algorithm — the inverse of tags.rs's
/// days-from-civil parser).
fn unix_ms_to_rfc3339_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400); // seconds of day
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z",
        hh = sod / 3600,
        mm = (sod % 3600) / 60,
        ss = sod % 60,
    )
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

#[cfg(test)]
mod checkout_tests {
    use super::*;

    /// The RFC3339 formatter against well-known epoch values —
    /// including the famous 1234567890 (2009-02-13T23:31:30Z) —
    /// so the manifest's `checked_out_at` is trustworthy UTC.
    #[test]
    fn unix_ms_formats_rfc3339_utc() {
        assert_eq!(unix_ms_to_rfc3339_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            unix_ms_to_rfc3339_utc(1_000_000_000_000),
            "2001-09-09T01:46:40.000Z"
        );
        assert_eq!(
            unix_ms_to_rfc3339_utc(1_234_567_890_123),
            "2009-02-13T23:31:30.123Z"
        );
        // Leap-year day: 2024-02-29T12:00:00Z = 1709208000 s.
        assert_eq!(
            unix_ms_to_rfc3339_utc(1_709_208_000_000),
            "2024-02-29T12:00:00.000Z"
        );
    }

    /// `rfc3339_now_utc` produces a well-formed, parseable-shaped
    /// timestamp (the wall-clock pin lives here; exact values are
    /// pinned by `unix_ms_formats_rfc3339_utc`).
    #[test]
    fn now_utc_is_well_formed() {
        let stamp = rfc3339_now_utc();
        assert!(stamp.ends_with('Z'), "{stamp}");
        assert_eq!(stamp.len(), 24, "YYYY-MM-DDTHH:MM:SS.mmmZ: {stamp}");
        assert!(stamp.starts_with("20"), "{stamp}");
    }

    /// `write_gitignore` is idempotent (a second call is a no-op)
    /// and APPENDS to pre-existing content rather than clobbering.
    #[test]
    fn gitignore_is_idempotent_and_append_safe() {
        let root = tempfile::tempdir().expect("tempdir");

        write_gitignore(root.path()).expect("writes");
        let first = std::fs::read_to_string(root.path().join(".gitignore")).expect("read");
        assert!(first.contains("scripts-manifest.json") && first.contains("*.py"));

        write_gitignore(root.path()).expect("rewrites");
        let second = std::fs::read_to_string(root.path().join(".gitignore")).expect("read");
        assert_eq!(first, second, "second write is a no-op");

        // Pre-existing user content survives; missing entries append.
        let root2 = tempfile::tempdir().expect("tempdir");
        std::fs::write(root2.path().join(".gitignore"), "target/\n").expect("seed");
        write_gitignore(root2.path()).expect("appends");
        let merged = std::fs::read_to_string(root2.path().join(".gitignore")).expect("read");
        assert!(
            merged.starts_with("target/"),
            "user content first: {merged:?}"
        );
        assert!(
            merged.contains("*.py"),
            "codec pattern appended: {merged:?}"
        );
    }

    /// The clobber gate, pure-fs level: empty/absent targets pass; a
    /// valid same-project manifest passes; everything else refuses
    /// BEFORE any write.
    #[test]
    fn recheckout_gate_refuses_unrelated_non_empty_targets() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("ws");

        // Absent target: fine.
        assert!(ensure_recheckout_safe(&target, "p").is_ok());

        // Empty dir: fine.
        std::fs::create_dir_all(&target).expect("mkdir");
        assert!(ensure_recheckout_safe(&target, "p").is_ok());

        // Unrelated non-empty: refuses with the stable phrase, naming
        // the directory.
        std::fs::write(target.join("unrelated.txt"), b"x").expect("seed");
        let err = ensure_recheckout_safe(&target, "p").expect_err("refuses");
        let message = err.to_string();
        assert!(
            message.contains("not empty and is not an ign workspace"),
            "{message}"
        );
        assert!(message.contains("ws"), "names the dir: {message}");

        // Corrupt manifest in a non-empty dir: still refuses (the
        // corrupt reason folds into the clobber message).
        std::fs::write(target.join(WORKSPACE_MANIFEST_NAME), b"{bad").expect("seed");
        let err = ensure_recheckout_safe(&target, "p").expect_err("refuses");
        assert!(
            err.to_string()
                .contains("not empty and is not an ign workspace")
        );

        // Valid manifest, foreign project: refuses naming BOTH.
        let mut manifest = WorkspaceManifest {
            schema_version: WORKSPACE_MANIFEST_SCHEMA_VERSION,
            project: "other".to_string(),
            profile: "rig".to_string(),
            checked_out_at: "2026-09-15T00:00:00.000Z".to_string(),
            members: BTreeMap::new(),
        };
        write_manifest(&target, &manifest).expect("writes");
        let err = ensure_recheckout_safe(&target, "p").expect_err("refuses");
        let message = err.to_string();
        assert!(
            message.contains("\"other\"") && message.contains("\"p\""),
            "{message}"
        );

        // Same project: allowed (refresh semantics).
        manifest.project = "p".to_string();
        write_manifest(&target, &manifest).expect("rewrites");
        assert!(ensure_recheckout_safe(&target, "p").is_ok());
    }
}
