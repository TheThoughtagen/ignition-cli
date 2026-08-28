//! Project-resource ZIP-member surgery (05-02) — the resource family
//! re-pointed onto project-export zips, closing the Phase 3
//! cross-phase defect: the `/projects/{p}/resources/**` REST routes
//! the family originally targeted DO NOT EXIST on real 8.3 gateways
//! (openapi-evidenced twice — 575 paths, zero matches — plus the EAM
//! probe and the gateway-scripting API audit; 05-RESEARCH). The
//! native steer's honest endpoint: export/import round-trip. These
//! helpers are the surgery half.
//!
//! PURE functions — no [`crate::client::GatewayApi`] surface, no I/O
//! beyond the `zip` crate itself — so every mapping is unit-testable
//! without a gateway. The orchestration (export → surgery → import)
//! lives in `actions::resources`.
//!
//! Zip layout of an 8.3 project export (05-RESEARCH, live-extracted):
//! `project.json` at the root plus `<collection>/resources/<rest>`
//! file members (collections are single-segment module ids —
//! `com.inductiveautomation.perspective`, `ignition`, …). The
//! user-facing path form — the Phase-3 UX-unchanged contract — is
//! `<collection>/<rest>`: the `resources/` segment is stripped on the
//! way OUT and re-inserted on the way IN. A no-slash user path (a
//! project-root file, e.g. `perspective-properties.json`) rides a
//! module named after the path itself: `<X>` ↔ `<X>/resources/<X>`
//! (06-08, live-proven — the only adoptable shape for root-level
//! files; see [`member_path`]). `project.json` is never a resource.
//! Directory entries (when a writer emits them) are
//! skipped on list and preserved verbatim on rewrite.
//!
//! [`ResourceEntry`] keeps the Phase-3 list shape (`path` typed,
//! passthrough extras) so the CLI's rendering contract is untouched;
//! surgery results carry no extras.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// One list item — the Phase-3 shape, unchanged: `path` typed (the
/// human renderer prints one per line), unknown keys round-trip.
/// Surgery-sourced entries carry no extras (the zip member list is
/// the whole truth).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceEntry {
    /// The resource path, e.g. `"ignition/script-python/e2e/scratch"`.
    #[serde(default)]
    pub path: Option<String>,
    /// `scope`, `version`, … — unknown keys round-trip (surgery
    /// entries leave this empty).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Map a user-facing resource path to its zip member path:
/// `<collection>/<rest>` → `<collection>/resources/<rest>`; a
/// no-slash path (a project-root file in user terms, e.g.
/// `perspective-properties.json`) round-trips through a module named
/// after the path itself — `<X>` → `<X>/resources/<X>`.
///
/// The root-level mapping is LIVE-PROVEN surgery shape (06-08, virgin
/// 8.3.3 rig): the file lands inside its own module's resources
/// container with the container descriptor naming it — the gateway
/// imports it exit-0 and re-exports it at exactly that member path.
/// The intuitive alternative — a file member at the zip root or one
/// literally named `<X>/resources` — is dead wire: root files are
/// silently not adopted (no parent descriptor can exist), and a file
/// named `resources` collides with the module's reserved resources
/// container (HTTP 500, "module folder must have folder flag set").
fn member_path(user_path: &str) -> String {
    match user_path.split_once('/') {
        Some((collection, rest)) => format!("{collection}/resources/{rest}"),
        None => format!("{user_path}/resources/{user_path}"),
    }
}

/// Map a zip member path back to the user-facing form — `None` for
/// every member that is not `<collection>/resources/<rest>` with a
/// nonempty rest (`project.json`, misplaced root files, directory
/// entries). The root-level inverse of [`member_path`]: a member
/// `<X>/resources/<X>` maps to the no-slash user path `<X>` (the
/// rest equals the collection), so put/get/list/delete round-trip
/// through one spelling. Note the deliberate alias: the explicit
/// user path `<X>/<X>` forwards to the same member and reads back
/// as `<X>` — one member, the no-slash spelling wins.
fn user_path(member: &str) -> Option<String> {
    let mut segments = member.split('/');
    let collection = segments.next()?;
    if collection.is_empty() || segments.next()? != "resources" {
        return None;
    }
    let rest = segments.collect::<Vec<_>>().join("/");
    if rest.is_empty() {
        return None;
    }
    if rest == collection {
        return Some(collection.to_string());
    }
    Some(format!("{collection}/{rest}"))
}

/// Open an export zip for reading — malformed bytes are a gateway
/// contract violation (the export endpoint answered non-zip), exit 1.
fn open_archive(zip_bytes: &[u8]) -> Result<zip::ZipArchive<std::io::Cursor<&[u8]>>, CoreError> {
    zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|err| CoreError::Internal(format!("project export is not a readable zip: {err}")))
}

/// The deterministic options every rewritten member rides: deflate
/// (both directions — Ignition exports and our imports), no
/// timestamps (`SimpleFileOptions` defaults are fixed), so identical
/// surgeries produce identical zips.
fn rewrite_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

/// The folder-descriptor filename every resource folder carries in a
/// gateway-produced export (live-extracted 8.3.3, 05-07 spike): the
/// `ign-cli` export's every route folder (`.../cli/tags/`, …) and a
/// fresh project's `ignition/global-props/` alike carry a
/// `resource.json` whose `files` array lists the folder's file
/// members. LIVE-PROVEN LANDING RULE: an overwrite-import LANDS a NEW
/// file member only when its immediate parent folder's descriptor
/// exists and lists the basename — a bare appended file is silently
/// ignored (the import still answers `{"success":true}` while nothing
/// lands; verified with AND without zip directory entries).
/// Intermediate plain folders above the resource folder carry
/// nothing (the webdev `cli/` precedent).
const FOLDER_DESCRIPTOR: &str = "resource.json";

/// The parent directory of a member path (`a/b/c` → `a/b`); `None`
/// for a root-level name.
fn parent_of(member: &str) -> Option<&str> {
    member.rsplit_once('/').map(|(parent, _)| parent)
}

/// Merge one basename into an EXISTING parent descriptor's `files`
/// array (idempotent): parse, append when absent, re-serialize
/// pretty. An unparseable descriptor is an export-contract violation
/// — refusing beats recreating the exact bug this plan closes
/// (`ok:true` while nothing lands).
fn merge_descriptor_member(existing: &[u8], basename: &str) -> Result<Vec<u8>, CoreError> {
    let mut value: serde_json::Value = serde_json::from_slice(existing).map_err(|err| {
        CoreError::Internal(format!(
            "parent resource descriptor is not valid JSON: {err}"
        ))
    })?;
    let object = value.as_object_mut().ok_or_else(|| {
        CoreError::Internal("parent resource descriptor is not a JSON object".to_string())
    })?;
    let files = object
        .entry("files")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if !files.is_array() {
        *files = serde_json::Value::Array(Vec::new());
    }
    let listed = files
        .as_array()
        .expect("just normalized to an array")
        .iter()
        .any(|name| name.as_str() == Some(basename));
    if !listed {
        files
            .as_array_mut()
            .expect("just normalized to an array")
            .push(serde_json::Value::String(basename.to_string()));
    }
    serde_json::to_vec_pretty(&value).map_err(|err| {
        CoreError::Internal(format!(
            "cannot serialize merged resource descriptor: {err}"
        ))
    })
}

/// Synthesize a NEW parent-folder descriptor in the live-proven
/// shape (the 05-07 variant-D wire answer): scope G, version 1,
/// unrestricted, overridable, `files` listing exactly the appended
/// basename, empty attributes.
fn synthesized_descriptor(basename: &str) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "scope": "G",
        "version": 1,
        "restricted": false,
        "overridable": true,
        "files": [basename],
        "attributes": {},
    }))
    .expect("the descriptor shape always serializes")
}

/// THE list primitive: user-facing paths of every resource member in
/// the export zip, in member order. `project.json`, directory
/// entries, and non-`resources`-shaped members are skipped.
pub fn resource_members(zip_bytes: &[u8]) -> Result<Vec<String>, CoreError> {
    let mut archive = open_archive(zip_bytes)?;
    let mut members = Vec::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk project export zip: {err}")))?;
        if file.is_dir() {
            continue;
        }
        if let Some(user) = user_path(file.name()) {
            members.push(user);
        }
    }
    Ok(members)
}

/// THE read primitive: one member's bytes, verbatim. A missing
/// member is the existing not-found error shape (exit 6) — the REST
/// family's 404 semantics carried over the surgery transport.
pub fn read_member(zip_bytes: &[u8], member: &str) -> Result<Vec<u8>, CoreError> {
    let target = member_path(member);
    let mut archive = open_archive(zip_bytes)?;
    let mut file = archive.by_name(&target).map_err(|err| match err {
        zip::result::ZipError::FileNotFound => CoreError::NotFound { endpoint: None },
        err => CoreError::Internal(format!("cannot read zip member {target:?}: {err}")),
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|err| {
        CoreError::Internal(format!("cannot decompress zip member {target:?}: {err}"))
    })?;
    Ok(bytes)
}

/// What [`rewrite_zip`] does to one target member.
enum Surgery<'a> {
    /// Replace the member's content — or APPEND it when absent (put
    /// can create new resources).
    Replace(&'a [u8]),
    /// Drop the member — absent is an error the caller raises.
    Remove,
}

/// The put-new descriptor action resolved before the copy loop
/// (05-07): append-when-absent needs the parent folder's descriptor
/// to list the new basename.
enum DescriptorSurgery {
    /// The archive already carries the parent descriptor — merge the
    /// basename into its `files` (edited in place, position kept —
    /// the live-proven variant-E ordering).
    Merge(String),
    /// No parent descriptor exists — synthesize one just before the
    /// appended member (the live-proven variant-D ordering:
    /// descriptor before file).
    Synthesize(String),
}

/// Full-zip rewrite: copy every member (decompressed → recompressed,
/// deflate, original order, directory entries preserved), applying
/// the surgery to the target. Returns the new zip plus whether the
/// target was seen (remove's not-found proof; replace appends when
/// unseen).
///
/// Put-new (05-07): when a Replace target is ABSENT, the append also
/// lands the parent-folder descriptor — merged when the archive
/// already carries one, synthesized otherwise. A target that IS a
/// descriptor (basename `resource.json`) authors it explicitly and
/// gets no second one. [`Surgery::Remove`] never touches descriptors:
/// the gateway reconciles a stale `files` list itself (live-proven —
/// the deleted file's descriptor comes back with the entry pruned).
fn rewrite_zip(
    zip_bytes: &[u8],
    target: &str,
    surgery: Surgery<'_>,
) -> Result<(Vec<u8>, bool), CoreError> {
    let mut archive = open_archive(zip_bytes)?;
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let target_present = names.iter().any(|name| name == target);

    // Resolve the descriptor surgery BEFORE copying (the merge must
    // edit the descriptor member as it streams past).
    let descriptor_surgery = match (&surgery, target_present) {
        (Surgery::Replace(_), false)
            if target
                .rsplit('/')
                .next()
                .is_some_and(|base| base != FOLDER_DESCRIPTOR) =>
        {
            parent_of(target).map(|parent| {
                let descriptor_path = format!("{parent}/{FOLDER_DESCRIPTOR}");
                if names.iter().any(|name| name == &descriptor_path) {
                    DescriptorSurgery::Merge(descriptor_path)
                } else {
                    DescriptorSurgery::Synthesize(descriptor_path)
                }
            })
        }
        _ => None,
    };

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = rewrite_options();
    let mut seen = false;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk project export zip: {err}")))?;
        let name = file.name().to_string();
        let is_target = name == target;
        seen |= is_target;
        if is_target && matches!(surgery, Surgery::Remove) {
            continue; // dropped — the rest of the zip carries on
        }
        if file.is_dir() {
            writer
                .add_directory(name, options)
                .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|err| {
            CoreError::Internal(format!("cannot decompress zip member {name:?}: {err}"))
        })?;
        writer
            .start_file(name.clone(), options)
            .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
        let content = match (is_target, &surgery) {
            (true, Surgery::Replace(content)) => *content,
            (_, Surgery::Replace(_)) if matches!(&descriptor_surgery, Some(DescriptorSurgery::Merge(path)) if path == &name) =>
            {
                // The parent descriptor rides MERGED: the new basename
                // joins its files list (idempotent), everything else
                // about it kept verbatim.
                &merge_descriptor_member(
                    &bytes,
                    target.rsplit('/').next().expect("non-root — checked above"),
                )?
            }
            _ => &bytes,
        };
        writer
            .write_all(content)
            .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
    }

    // Replace-appends-when-absent: the put-upsert semantics (a new
    // resource joins the zip at the end, member order otherwise
    // preserved) — the parent descriptor lands FIRST (the
    // live-proven ordering: descriptor before file).
    if !seen && let Surgery::Replace(content) = surgery {
        if let Some(DescriptorSurgery::Synthesize(descriptor_path)) = &descriptor_surgery {
            let basename = target.rsplit('/').next().expect("non-root — checked above");
            let descriptor = synthesized_descriptor(basename);
            writer
                .start_file(descriptor_path.clone(), options)
                .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
            writer
                .write_all(&descriptor)
                .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
        }
        writer
            .start_file(target, options)
            .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
        writer
            .write_all(content)
            .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
    }

    let cursor = writer
        .finish()
        .map_err(|err| CoreError::Internal(format!("cannot finalize rewritten zip: {err}")))?;
    Ok((cursor.into_inner(), seen))
}

/// THE put primitive: replace the member's content — or append it
/// when absent (upsert: created if missing). Every other member,
/// their order, and directory entries ride across untouched.
pub fn replace_member(
    zip_bytes: &[u8],
    member: &str,
    content: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let (zip, _) = rewrite_zip(zip_bytes, &member_path(member), Surgery::Replace(content))?;
    Ok(zip)
}

/// THE delete primitive: the zip minus the member. A missing member
/// is the existing not-found error shape (exit 6).
pub fn remove_member(zip_bytes: &[u8], member: &str) -> Result<Vec<u8>, CoreError> {
    let (zip, seen) = rewrite_zip(zip_bytes, &member_path(member), Surgery::Remove)?;
    if !seen {
        return Err(CoreError::NotFound { endpoint: None });
    }
    Ok(zip)
}

// ---- Pure diff engine (07-01, SYNC-01) -----------------------------------
//
// Cross-gateway project diff, member-level with resource.json
// NORMALIZATION — the live-evidenced volatility guard (07-RESEARCH
// Pitfall 1): every gateway-written descriptor carries
// `attributes.lastModification` (+`…Signature`), so a byte-compare
// flags identical content exported from two gateways as CHANGED.
// Normalization strips exactly those two attribute fields, keeps
// everything semantic (`scope`/`version`/`files`, the REST of
// `attributes`), and re-serializes into a canonical form this module
// OWNS — see [`normalize_descriptor`]. All pure, zero new
// dependencies, unit-testable without a gateway (the 05-02 pattern).

/// 64-bit FNV-1a — the member digest. No sha2 dependency: collision
/// risk (2^-64 per pair on 64-bit hashes) is acceptable for diff UX;
/// this is change detection, not security.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Rebuild a JSON value with every object's entries sorted by key —
/// the canonicalizer [`normalize_descriptor`] owns. serde_json's
/// default map IS sorted (BTreeMap) today, but a workspace-wide
/// `preserve_order` feature flip would make it insertion-ordered:
/// sorting explicitly means canonical output never depends on the
/// ambient map behavior (the cross-version guard — pinned by the
/// key-order-independence unit test).
fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(entries.into_iter().collect::<serde_json::Map<_, _>>())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize).collect())
        }
        other => other,
    }
}

/// Normalize one `resource.json` descriptor for comparison: parse,
/// strip the two live-evidenced volatility fields
/// (`attributes.lastModification` and
/// `attributes.lastModificationSignature` — keep every other
/// attribute and all semantic keys), recursively sort object keys,
/// re-serialize compact. `None` for non-JSON or a non-object root —
/// the caller hashes the raw bytes instead (the descriptor is exotic
/// or corrupt; content honesty over a false equality).
pub fn normalize_descriptor(json: &[u8]) -> Option<Vec<u8>> {
    let mut value: serde_json::Value = serde_json::from_slice(json).ok()?;
    if !value.is_object() {
        return None; // non-object roots hash raw (the caller's rule)
    }
    if let Some(attributes) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("attributes"))
        .and_then(|attributes| attributes.as_object_mut())
    {
        attributes.remove("lastModification");
        attributes.remove("lastModificationSignature");
    }
    serde_json::to_vec(&canonicalize(value)).ok()
}

/// User path → FNV-1a digest for every resource member in the export
/// zip. Members whose basename is `resource.json` hash their
/// NORMALIZED form ([`normalize_descriptor`]); everything else hashes
/// raw bytes. `project.json`, directory entries, and
/// non-`resources`-shaped members carry no user path and are skipped
/// (the same walk [`resource_members`] rides) — which is exactly how
/// [`diff_members`] excludes the root project.json from resource
/// entries.
pub fn member_hashes(zip_bytes: &[u8]) -> Result<BTreeMap<String, u64>, CoreError> {
    let mut archive = open_archive(zip_bytes)?;
    let mut hashes = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| CoreError::Internal(format!("cannot walk project export zip: {err}")))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let Some(user) = user_path(&name) else {
            continue;
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|err| {
            CoreError::Internal(format!("cannot decompress zip member {name:?}: {err}"))
        })?;
        let content = if name.rsplit('/').next() == Some(FOLDER_DESCRIPTOR) {
            normalize_descriptor(&bytes).unwrap_or(bytes)
        } else {
            bytes
        };
        hashes.insert(user, fnv1a(&content));
    }
    Ok(hashes)
}

/// One member's diff status — B-relative-to-A semantics (the LOCKED
/// direction): `added` = in B only, `removed` = in A only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberStatus {
    /// Present in B, absent in A.
    Added,
    /// Present in A, absent in B.
    Removed,
    /// Present in both, normalized hashes differ.
    Changed,
    /// Present in both, normalized hashes equal.
    Same,
}

/// One row of the diff: the user path + its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemberDiffEntry {
    /// The user-facing resource path.
    pub path: String,
    /// B-relative-to-A status.
    pub status: MemberStatus,
}

/// The four counts — the summary line and the JSON `summary` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub struct DiffSummary {
    /// Members equal (after normalization) in both.
    pub same: usize,
    /// Members only in B.
    pub added: usize,
    /// Members only in A.
    pub removed: usize,
    /// Members differing (after normalization) between A and B.
    pub changed: usize,
}

/// The member-level diff result: counts + one entry per resource
/// member, sorted by path. The root `project.json` is EXCLUDED (it is
/// not a resource; [`project_meta_delta`] surfaces it separately).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemberDiff {
    /// The four counts.
    pub summary: DiffSummary,
    /// Every resource member with its status, path-sorted.
    pub entries: Vec<MemberDiffEntry>,
}

/// THE compare primitive: B-relative-to-A member statuses over two
/// export zips — `added` = path in B not A, `removed` = in A not B,
/// `changed` = both with differing normalized hashes, `same` = both
/// with equal ones. Entries ride path-sorted (the BTreeMap union
/// iterates sorted). The root `project.json` never appears (the
/// [`member_hashes`] walk skips it).
pub fn diff_members(zip_a: &[u8], zip_b: &[u8]) -> Result<MemberDiff, CoreError> {
    let hashes_a = member_hashes(zip_a)?;
    let hashes_b = member_hashes(zip_b)?;
    let paths: std::collections::BTreeSet<&String> =
        hashes_a.keys().chain(hashes_b.keys()).collect();
    let mut summary = DiffSummary::default();
    let mut entries = Vec::with_capacity(paths.len());
    for path in paths {
        let status = match (hashes_a.get(path), hashes_b.get(path)) {
            (Some(hash_a), Some(hash_b)) => {
                if hash_a == hash_b {
                    MemberStatus::Same
                } else {
                    MemberStatus::Changed
                }
            }
            (None, Some(_)) => MemberStatus::Added,
            (Some(_), None) => MemberStatus::Removed,
            (None, None) => unreachable!("the union only carries present keys"),
        };
        match status {
            MemberStatus::Same => summary.same += 1,
            MemberStatus::Added => summary.added += 1,
            MemberStatus::Removed => summary.removed += 1,
            MemberStatus::Changed => summary.changed += 1,
        }
        entries.push(MemberDiffEntry {
            path: (*path).clone(),
            status,
        });
    }
    Ok(MemberDiff { summary, entries })
}

/// The root `project.json` member, parsed — `Ok(None)` when the member
/// is absent; a parse failure is ALSO `Ok(None)` (the caller treats a
/// missing/unparseable project.json as "no meta to compare" — the
/// diff is about resources).
fn root_project_json(zip_bytes: &[u8]) -> Result<Option<serde_json::Value>, CoreError> {
    let mut archive = open_archive(zip_bytes)?;
    let Ok(mut file) = archive.by_name("project.json") else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|err| {
        CoreError::Internal(format!(
            "cannot decompress zip member \"project.json\": {err}"
        ))
    })?;
    Ok(serde_json::from_slice(&bytes).ok())
}

/// A missing/null JSON value rendered as text for the delta triples.
fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Compare the root `project.json`'s SEMANTIC fields — `title`,
/// `enabled`, `parent` only — returning one `(field, a_value,
/// b_value)` triple per differing field (stringified values; absent
/// renders as `null`). Missing member or parse failure → empty vec:
/// the diff is about resources, project meta rides separately.
pub fn project_meta_delta(
    zip_a: &[u8],
    zip_b: &[u8],
) -> Result<Vec<(String, String, String)>, CoreError> {
    let a = root_project_json(zip_a)?.unwrap_or(serde_json::Value::Null);
    let b = root_project_json(zip_b)?.unwrap_or(serde_json::Value::Null);
    let mut deltas = Vec::new();
    for field in ["title", "enabled", "parent"] {
        let (value_a, value_b) = (&a[field], &b[field]);
        if value_a != value_b {
            deltas.push((field.to_string(), value_text(value_a), value_text(value_b)));
        }
    }
    Ok(deltas)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::{ResourceEntry, read_member, remove_member, replace_member, resource_members};
    use crate::error::CoreError;

    /// Build a small in-test export zip: `project.json` + one member
    /// per `(name, bytes)` pair, in order (the zip crate writer is
    /// the same engine the surgery rides, so fixtures are honest).
    fn fixture_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file("project.json", options)
            .expect("project.json starts");
        writer
            .write_all(br#"{"title":"T"}"#)
            .expect("project.json writes");
        for (name, bytes) in members {
            writer.start_file(*name, options).expect("member starts");
            writer.write_all(bytes).expect("member writes");
        }
        writer.finish().expect("zip finalizes").into_inner()
    }

    /// The two real-world members the round-trip units ride: a core
    /// script and a Perspective view file.
    const SCRIPT_MEMBER: &str = "ignition/resources/script-python/e2e/scratch";
    const VIEW_MEMBER: &str = "com.example/resources/views/Dashboard/view.json";

    fn sample_zip() -> Vec<u8> {
        fixture_zip(&[
            (
                SCRIPT_MEMBER,
                br#"{"scope":"G","code":"print('old')"}"#.as_slice(),
            ),
            (VIEW_MEMBER, br#"{"scope":"A"}"#.as_slice()),
        ])
    }

    /// THE mapping pin: list strips the `resources/` segment
    /// (user-facing form, UX-unchanged), skips `project.json`, and
    /// preserves member order.
    #[test]
    fn resource_members_maps_user_paths_and_skips_project_json() {
        let zip = sample_zip();
        assert_eq!(
            resource_members(&zip).expect("list parses"),
            vec![
                "ignition/script-python/e2e/scratch".to_string(),
                "com.example/views/Dashboard/view.json".to_string(),
            ],
            "resources/ stripped, project.json skipped, order preserved"
        );

        // A zip with ONLY project.json lists empty (a fresh project).
        assert!(
            resource_members(&fixture_zip(&[]))
                .expect("bare zip parses")
                .is_empty()
        );
    }

    /// Read returns the member bytes verbatim (user path in).
    #[test]
    fn read_member_returns_verbatim_bytes() {
        let zip = sample_zip();
        assert_eq!(
            read_member(&zip, "ignition/script-python/e2e/scratch").expect("member reads"),
            br#"{"scope":"G","code":"print('old')"}"#.to_vec()
        );
        assert_eq!(
            read_member(&zip, "com.example/views/Dashboard/view.json").expect("member reads"),
            br#"{"scope":"A"}"#.to_vec()
        );
    }

    /// Missing member → the existing not-found shape (exit 6) — for
    /// both a full miss and the no-slash root-level form (whose
    /// member can exist only after a root-level put); a non-zip
    /// input is an internal error (the export contract was violated).
    #[test]
    fn missing_member_and_garbage_zip_error_shapes() {
        let zip = sample_zip();
        let err =
            read_member(&zip, "ignition/script-python/nope").expect_err("missing member must fail");
        assert!(
            matches!(err, CoreError::NotFound { endpoint: None }),
            "wrong class: {err}"
        );
        assert_eq!(err.exit_code(), 6);
        assert_eq!(err.code(), "not_found");

        let root_level =
            read_member(&zip, "ignition").expect_err("absent root-level member must fail");
        assert!(
            matches!(root_level, CoreError::NotFound { endpoint: None }),
            "the root-level member form is absent until a put creates it: {root_level}"
        );

        let garbage = resource_members(b"not a zip at all").expect_err("garbage must fail");
        assert!(
            matches!(garbage, CoreError::Internal(_)),
            "garbage: {garbage}"
        );
        assert_eq!(garbage.exit_code(), 1);
    }

    /// THE replace pin: content swapped, every other member intact,
    /// member order preserved — and the result is itself a valid zip
    /// the helpers can walk again (the surgery round-trip).
    #[test]
    fn replace_member_swaps_content_preserving_everything_else() {
        let zip = sample_zip();
        let new_body = br#"{"scope":"G","code":"print('new')"}"#.as_slice();
        let out = replace_member(&zip, "ignition/script-python/e2e/scratch", new_body)
            .expect("replace rewrites");
        assert_eq!(
            read_member(&out, "ignition/script-python/e2e/scratch").expect("re-read"),
            new_body.to_vec(),
            "the target carries the new content"
        );
        assert_eq!(
            read_member(&out, "com.example/views/Dashboard/view.json").expect("re-read"),
            br#"{"scope":"A"}"#.to_vec(),
            "the neighbor is untouched"
        );
        assert_eq!(
            resource_members(&out).expect("re-list"),
            vec![
                "ignition/script-python/e2e/scratch".to_string(),
                "com.example/views/Dashboard/view.json".to_string(),
            ],
            "order and membership preserved"
        );
    }

    /// THE upsert pin (05-07 re-pinned): replacing an ABSENT member
    /// appends it (put can create new resources) AND lands the
    /// parent-folder `resource.json` descriptor the live-proven
    /// landing rule requires — the descriptor rides FIRST (the
    /// gateway-accepted ordering), existing members keep their order.
    #[test]
    fn replace_member_appends_when_absent() {
        let zip = sample_zip();
        let out = replace_member(&zip, "ignition/script-python/e2e/brand-new", b"print('x')")
            .expect("append rewrites");
        assert_eq!(
            read_member(&out, "ignition/script-python/e2e/brand-new").expect("appended reads"),
            b"print('x')".to_vec()
        );
        // THE landing shape: the parent folder now carries a
        // descriptor listing the new basename.
        let descriptor =
            read_member(&out, "ignition/script-python/e2e/resource.json").expect("descriptor");
        let parsed: serde_json::Value = serde_json::from_slice(&descriptor).expect("json");
        assert_eq!(parsed["files"], serde_json::json!(["brand-new"]));
        assert_eq!(parsed["scope"], serde_json::json!("G"));
        // Member order: originals first, then the synthesized
        // descriptor, then the appended member.
        assert_eq!(
            resource_members(&out).expect("re-list"),
            vec![
                "ignition/script-python/e2e/scratch".to_string(),
                "com.example/views/Dashboard/view.json".to_string(),
                "ignition/script-python/e2e/resource.json".to_string(),
                "ignition/script-python/e2e/brand-new".to_string(),
            ],
            "the descriptor rides LAST-but-one; the originals keep their order"
        );
    }

    /// THE merge pin (05-07, variant-E wire truth): appending into a
    /// folder that ALREADY carries a descriptor merges the basename
    /// into its `files` (idempotently — an already-listed name does
    /// not duplicate), every other descriptor key kept verbatim, the
    /// descriptor's member position preserved.
    #[test]
    fn replace_member_appends_merging_existing_descriptor() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("project.json", options).expect("starts");
        writer.write_all(br#"{"title":"T"}"#).expect("writes");
        writer
            .start_file(
                "ignition/resources/script-python/uat/resource.json",
                options,
            )
            .expect("starts");
        writer
            .write_all(br#"{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["hello2.py"],"attributes":{"keep":"me"}}"#)
            .expect("writes");
        writer
            .start_file("ignition/resources/script-python/uat/hello2.py", options)
            .expect("starts");
        writer.write_all(b"print('old')").expect("writes");
        let zip = writer.finish().expect("finalize").into_inner();

        // Append a sibling into the SAME resource folder.
        let out = replace_member(
            &zip,
            "ignition/script-python/uat/hello3.py",
            b"print('new')'",
        )
        .expect("append rewrites");
        let descriptor: serde_json::Value = serde_json::from_slice(
            &read_member(&out, "ignition/script-python/uat/resource.json").expect("descriptor"),
        )
        .expect("descriptor json");
        assert_eq!(
            descriptor["files"],
            serde_json::json!(["hello2.py", "hello3.py"]),
            "the new basename joins the files list"
        );
        assert_eq!(
            descriptor["attributes"]["keep"],
            serde_json::json!("me"),
            "unknown descriptor keys ride verbatim"
        );
        assert_eq!(
            read_member(&out, "ignition/script-python/uat/hello3.py").expect("appended reads"),
            b"print('new')'".to_vec()
        );

        // Idempotent re-merge: replacing the SAME absent-member path
        // again does not duplicate the files entry (and a SECOND new
        // sibling appends after the first).
        let again = replace_member(&out, "ignition/script-python/uat/hello4.py", b"x")
            .expect("second append");
        let descriptor: serde_json::Value = serde_json::from_slice(
            &read_member(&again, "ignition/script-python/uat/resource.json").expect("d"),
        )
        .expect("json");
        assert_eq!(
            descriptor["files"],
            serde_json::json!(["hello2.py", "hello3.py", "hello4.py"])
        );
    }

    /// An appended member whose basename IS the descriptor authors it
    /// explicitly — NO second (parent-of-parent) descriptor is
    /// synthesized, and the member rides at exactly its path.
    #[test]
    fn replace_member_appending_a_descriptor_authors_it_explicitly() {
        let zip = sample_zip();
        let descriptor_body = br#"{"scope":"A","version":1,"files":["view.json"]}"#;
        let out = replace_member(
            &zip,
            "com.example/views/Dashboard/resource.json",
            descriptor_body,
        )
        .expect("append rewrites");
        assert_eq!(
            read_member(&out, "com.example/views/Dashboard/resource.json").expect("reads"),
            descriptor_body.to_vec(),
            "the authored descriptor rides verbatim"
        );
        // No descriptor-of-the-descriptor: the parent folder gained
        // nothing but the member itself.
        assert!(
            read_member(&out, "com.example/views/resource.json").is_err(),
            "no second descriptor is synthesized for an authored descriptor member"
        );
    }

    /// An unparseable EXISTING parent descriptor on the append path
    /// refuses (internal) rather than shipping an import the gateway
    /// would silently ignore — the exact bug class 05-07 closes.
    #[test]
    fn replace_member_append_over_corrupt_descriptor_refuses() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("project.json", options).expect("starts");
        writer.write_all(br#"{"title":"T"}"#).expect("writes");
        writer
            .start_file(
                "ignition/resources/script-python/uat/resource.json",
                options,
            )
            .expect("starts");
        writer.write_all(b"<<<not json>>>").expect("writes");
        let zip = writer.finish().expect("finalize").into_inner();

        let err = replace_member(&zip, "ignition/script-python/uat/new.py", b"x")
            .expect_err("corrupt descriptor must refuse the append");
        assert!(matches!(err, CoreError::Internal(_)), "{err}");
        assert_eq!(err.exit_code(), 1);
    }

    /// THE delete pin (05-07, live-proven): remove leaves the parent
    /// descriptor UNTOUCHED — the gateway itself reconciles the stale
    /// `files` entry (the wire truth from the variant-G2 probe).
    #[test]
    fn remove_member_leaves_descriptor_to_gateway_reconciliation() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("project.json", options).expect("starts");
        writer.write_all(br#"{"title":"T"}"#).expect("writes");
        writer
            .start_file(
                "ignition/resources/script-python/uat/resource.json",
                options,
            )
            .expect("starts");
        writer
            .write_all(br#"{"scope":"G","version":1,"files":["scratch.py"],"attributes":{}}"#)
            .expect("writes");
        writer
            .start_file("ignition/resources/script-python/uat/scratch.py", options)
            .expect("starts");
        writer.write_all(b"print('x')").expect("writes");
        let zip = writer.finish().expect("finalize").into_inner();

        let out =
            remove_member(&zip, "ignition/script-python/uat/scratch.py").expect("remove rewrites");
        assert_eq!(
            read_member(&out, "ignition/script-python/uat/resource.json").expect("descriptor"),
            br#"{"scope":"G","version":1,"files":["scratch.py"],"attributes":{}}"#.to_vec(),
            "the descriptor rides verbatim — the gateway prunes the stale entry"
        );
    }

    /// THE remove pin: the member is gone (a follow-up read is
    /// not-found), neighbors survive; removing a missing member is
    /// the not-found error.
    #[test]
    fn remove_member_drops_exactly_the_target() {
        let zip = sample_zip();
        let out =
            remove_member(&zip, "com.example/views/Dashboard/view.json").expect("remove rewrites");
        let err = read_member(&out, "com.example/views/Dashboard/view.json")
            .expect_err("removed member must be gone");
        assert!(matches!(err, CoreError::NotFound { .. }), "gone: {err}");
        assert_eq!(
            resource_members(&out).expect("re-list"),
            vec!["ignition/script-python/e2e/scratch".to_string()]
        );

        let missing = remove_member(&zip, "com.example/views/Never").expect_err("must fail");
        assert!(matches!(missing, CoreError::NotFound { .. }), "{missing}");
        assert_eq!(missing.exit_code(), 6);
    }

    /// Surgery preserves directory entries across rewrites (a writer
    /// that emits them gets them back — position kept). The member
    /// `ignition/resources/a` is addressed by its USER path
    /// (`ignition/a` — the `resources/` segment is the mapping's).
    #[test]
    fn rewrite_preserves_directory_entries() {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer
            .add_directory("ignition/resources/", options)
            .expect("dir starts");
        writer
            .start_file("ignition/resources/a", options)
            .expect("file starts");
        writer.write_all(b"a").expect("file writes");
        let zip = writer.finish().expect("finalize").into_inner();

        let out = remove_member(&zip, "ignition/a").expect("remove");
        // The directory entry survives as an entry (len counts it) and
        // does NOT appear as a resource.
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&out)).expect("re-open");
        assert_eq!(archive.len(), 1, "the directory entry survived the rewrite");
        assert!(archive.by_index(0).expect("entry").is_dir());
        assert!(resource_members(&out).expect("list").is_empty());
    }

    /// The Phase-3 list item shape survives the re-point verbatim
    /// (surgery entries carry no extras; the passthrough contract
    /// stays for wire-sourced shapes).
    #[test]
    fn resource_entry_shape_unchanged() {
        let entry: ResourceEntry = serde_json::from_value(serde_json::json!({
            "path": "ignition/script-python/e2e/scratch",
            "scope": "G"
        }))
        .expect("plausible item must parse");
        assert_eq!(
            entry.path.as_deref(),
            Some("ignition/script-python/e2e/scratch")
        );
        assert_eq!(entry.extra.get("scope"), Some(&serde_json::json!("G")));

        let bare: ResourceEntry =
            serde_json::from_value(serde_json::json!({"path": "x"})).expect("extras-free parses");
        assert!(bare.extra.is_empty());
    }

    // ---- Pure diff engine units (07-01) ----

    use super::{
        DiffSummary, MemberDiff, MemberDiffEntry, MemberStatus, diff_members, member_hashes,
        normalize_descriptor, project_meta_delta,
    };

    /// The diff-side fixture builder: a custom `project.json` (the
    /// meta-delta tests need differing titles) + one member per pair.
    fn diff_zip(project_json: &[u8], members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file("project.json", options)
            .expect("project.json starts");
        writer.write_all(project_json).expect("project.json writes");
        for (name, bytes) in members {
            writer.start_file(*name, options).expect("member starts");
            writer.write_all(bytes).expect("member writes");
        }
        writer.finish().expect("zip finalizes").into_inner()
    }

    /// A live-shaped folder descriptor: scope/version/files plus the
    /// two volatility attributes every gateway-written resource.json
    /// carries (07-RESEARCH Pitfall 1).
    fn descriptor(plain_body: &str, modified_a: &str) -> Vec<u8> {
        format!(
            r#"{{"scope":"G","version":1,"files":["script.py"],"attributes":{{"lastModification":{{"actor":"admin","timestamp":"{modified_a}","signature":"sig-{modified_a}"}},"lastModificationSignature":"sig-{modified_a}","notes":"{plain_body}"}}}}"#
        )
        .into_bytes()
    }

    const DESC_MEMBER: &str = "ignition/resources/script-python/uat/resource.json";
    const SCRIPT_MEMBER_2: &str = "ignition/resources/script-python/uat/script.py";
    const VIEW_MEMBER_2: &str = "com.example/resources/views/Dash/view.json";

    /// (a) THE volatility pin: identical descriptor CONTENT with
    /// DIFFERING `lastModification` attributes reports `same` — the
    /// normalized compare, not a byte compare.
    #[test]
    fn diff_same_content_differing_modification_attributes_is_same() {
        let a = diff_zip(
            br#"{"title":"T","enabled":true}"#,
            &[(
                DESC_MEMBER,
                descriptor("kept", "2026-08-28T10:00:00Z").as_slice(),
            )],
        );
        let b = diff_zip(
            br#"{"title":"T","enabled":true}"#,
            &[(
                DESC_MEMBER,
                descriptor("kept", "2026-08-28T11:30:00Z").as_slice(),
            )],
        );
        let diff = diff_members(&a, &b).expect("diff parses");
        assert_eq!(
            diff,
            MemberDiff {
                summary: DiffSummary {
                    same: 1,
                    added: 0,
                    removed: 0,
                    changed: 0
                },
                entries: vec![MemberDiffEntry {
                    path: "ignition/script-python/uat/resource.json".to_string(),
                    status: MemberStatus::Same,
                }],
            }
        );
    }

    /// (b)+(c)+(d) THE direction pin: a member only in B is `added`,
    /// one only in A is `removed`, differing script BYTES are
    /// `changed` — B-relative-to-A, entries path-sorted.
    #[test]
    fn diff_direction_semantics_added_removed_changed() {
        let a = diff_zip(
            br#"{"title":"T"}"#,
            &[
                (SCRIPT_MEMBER_2, b"print('old')"),
                (VIEW_MEMBER_2, br#"{"scope":"A"}"#.as_slice()),
            ],
        );
        let b = diff_zip(
            br#"{"title":"T"}"#,
            &[
                (SCRIPT_MEMBER_2, b"print('new')"),
                (
                    "com.example/resources/views/Fresh/view.json",
                    br#"{"scope":"G"}"#.as_slice(),
                ),
            ],
        );
        let diff = diff_members(&a, &b).expect("diff parses");
        assert_eq!(
            diff.entries,
            vec![
                MemberDiffEntry {
                    path: "com.example/views/Dash/view.json".to_string(),
                    status: MemberStatus::Removed,
                },
                MemberDiffEntry {
                    path: "com.example/views/Fresh/view.json".to_string(),
                    status: MemberStatus::Added,
                },
                MemberDiffEntry {
                    path: "ignition/script-python/uat/script.py".to_string(),
                    status: MemberStatus::Changed,
                },
            ],
            "B-relative-to-A, path-sorted"
        );
        assert_eq!(
            diff.summary,
            DiffSummary {
                same: 0,
                added: 1,
                removed: 1,
                changed: 1
            }
        );
    }

    /// (e) Empty-vs-populated: everything in A reports `removed`
    /// (and symmetrically everything in B would be `added`).
    #[test]
    fn diff_empty_vs_populated_is_all_removed() {
        let a = diff_zip(br#"{"title":"T"}"#, &[(SCRIPT_MEMBER_2, b"print('old')")]);
        let b = diff_zip(br#"{"title":"T"}"#, &[]);
        let diff = diff_members(&a, &b).expect("diff parses");
        assert_eq!(diff.summary.removed, 1);
        assert_eq!(
            diff.summary.added + diff.summary.changed + diff.summary.same,
            0
        );
        assert_eq!(diff.entries[0].status, MemberStatus::Removed);

        // The mirror: populated-vs-empty is all `added`.
        let mirror = diff_members(&b, &a).expect("diff parses");
        assert_eq!(mirror.summary.added, 1);
        assert_eq!(mirror.entries[0].status, MemberStatus::Added);
    }

    /// (f) THE exclusion pin: a differing `project.json` title rides
    /// the META delta as one triple and NEVER appears in the member
    /// entries — `diff_members` excludes the root project.json (it is
    /// not a resource).
    #[test]
    fn diff_excludes_root_project_json_and_surfaces_meta_delta() {
        let a = diff_zip(
            br#"{"title":"Old","enabled":true}"#,
            &[(SCRIPT_MEMBER_2, b"x")],
        );
        let b = diff_zip(
            br#"{"title":"New","enabled":true}"#,
            &[(SCRIPT_MEMBER_2, b"x")],
        );
        let diff = diff_members(&a, &b).expect("diff parses");
        assert_eq!(diff.summary.same, 1, "the only resource member is same");
        assert!(
            !diff
                .entries
                .iter()
                .any(|entry| entry.path == "project.json"),
            "the root project.json is never a resource entry"
        );
        assert_eq!(
            project_meta_delta(&a, &b).expect("meta delta parses"),
            vec![("title".to_string(), "Old".to_string(), "New".to_string())],
            "the title difference rides the meta delta exactly"
        );

        // Semantic fields beyond the trio never ride: a differing
        // description is NOT a delta (scope discipline).
        let c = diff_zip(
            br#"{"title":"New","enabled":true,"description":"differs"}"#,
            &[(SCRIPT_MEMBER_2, b"x")],
        );
        assert!(
            project_meta_delta(&b, &c)
                .expect("meta delta parses")
                .is_empty(),
            "only title/enabled/parent compare"
        );
    }

    /// (g) THE key-order-independence pin (the cross-version
    /// canonicalization guard): the SAME logical descriptor
    /// serialized with two DIFFERENT key orders — top-level AND
    /// nested-object keys shuffled — normalizes to byte-identical
    /// output, so the members hash equal and report `same`. Canonical
    /// output must never depend on input key order or serde_json's
    /// ambient map behavior (a future `preserve_order` flip).
    #[test]
    fn normalize_descriptor_is_key_order_independent() {
        let order_a = br#"{"scope":"G","version":1,"files":["a.py","b.py"],"attributes":{"lastModification":{"actor":"x","timestamp":"t"},"keep":{"z":1,"a":2}}}"#;
        let order_b = br#"{"attributes":{"keep":{"a":2,"z":1},"lastModification":{"timestamp":"t","actor":"x"}},"files":["a.py","b.py"],"version":1,"scope":"G"}"#;
        let normalized_a = normalize_descriptor(order_a).expect("a normalizes");
        let normalized_b = normalize_descriptor(order_b).expect("b normalizes");
        assert_eq!(
            normalized_a, normalized_b,
            "canonical output depends only on content, never key order"
        );
        // …and the member-level consequence: the two zips diff `same`.
        let zip_a = diff_zip(br#"{"title":"T"}"#, &[(DESC_MEMBER, &normalized_a)]);
        let zip_b = diff_zip(br#"{"title":"T"}"#, &[(DESC_MEMBER, &normalized_b)]);
        let diff = diff_members(&zip_a, &zip_b).expect("diff parses");
        assert_eq!(diff.summary.same, 1);
        assert_eq!(diff.summary.changed, 0);

        // The strip is EXACTLY the two volatility fields — other
        // attribute content differing still means `changed`.
        let keep_a = br#"{"scope":"G","attributes":{"notes":"one"}}"#;
        let keep_b = br#"{"scope":"G","attributes":{"notes":"two"}}"#;
        assert_ne!(
            normalize_descriptor(keep_a).expect("normalizes"),
            normalize_descriptor(keep_b).expect("normalizes"),
            "non-volatility attribute differences survive normalization"
        );
    }

    /// Non-JSON / non-object descriptors return `None` — the caller
    /// hashes the RAW bytes (exotic descriptors compare bytewise,
    /// honest over a false equality).
    #[test]
    fn normalize_descriptor_refuses_non_json_and_non_object() {
        assert!(normalize_descriptor(b"print('not json')").is_none());
        assert!(normalize_descriptor(br#"[1,2,3]"#).is_none());
        assert!(normalize_descriptor(b"").is_none());
        // A plain object without attributes normalizes fine (the
        // strip is conditional).
        assert!(normalize_descriptor(br#"{"scope":"G"}"#).is_some());
    }

    /// `member_hashes` maps USER paths (project.json skipped,
    /// directory entries skipped) and normalizes descriptor members —
    /// the map's key set is exactly `resource_members`' list.
    #[test]
    fn member_hashes_key_set_matches_resource_members() {
        let a = diff_zip(
            br#"{"title":"T"}"#,
            &[
                (DESC_MEMBER, descriptor("x", "t1").as_slice()),
                (SCRIPT_MEMBER_2, b"print('x')"),
                (VIEW_MEMBER_2, br#"{"scope":"A"}"#.as_slice()),
            ],
        );
        let hashes = member_hashes(&a).expect("hashes parse");
        assert_eq!(
            hashes.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![
                "com.example/views/Dash/view.json",
                "ignition/script-python/uat/resource.json",
                "ignition/script-python/uat/script.py",
            ],
            "user paths, project.json skipped, path-sorted"
        );
        assert!(hashes.values().all(|hash| *hash != 0));
    }

    /// The serialized agent shapes: statuses render as lowercase
    /// strings and the summary carries all four keys always.
    #[test]
    fn diff_shapes_serialize_stably() {
        let entry = MemberDiffEntry {
            path: "x/y".to_string(),
            status: MemberStatus::Added,
        };
        assert_eq!(
            serde_json::to_value(&entry).expect("serializes"),
            serde_json::json!({"path": "x/y", "status": "added"})
        );
        let summary = serde_json::to_value(DiffSummary {
            same: 1,
            added: 2,
            removed: 3,
            changed: 4,
        })
        .expect("serializes");
        assert_eq!(
            summary,
            serde_json::json!({"same": 1, "added": 2, "removed": 3, "changed": 4})
        );
        for (status, word) in [
            (MemberStatus::Added, "added"),
            (MemberStatus::Removed, "removed"),
            (MemberStatus::Changed, "changed"),
            (MemberStatus::Same, "same"),
        ] {
            assert_eq!(
                serde_json::to_value(status).expect("serializes"),
                serde_json::Value::String(word.to_string())
            );
        }
    }
}
