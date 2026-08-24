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
//! way OUT and re-inserted on the way IN. `project.json` is never a
//! resource. Directory entries (when a writer emits them) are
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
/// `<collection>/<rest>` → `<collection>/resources/<rest>`.
/// A degenerate no-slash path maps to `<path>/resources`, which can
/// never match a member (resources always have a rest) — the honest
/// answer for such input is not-found downstream.
fn member_path(user_path: &str) -> String {
    match user_path.split_once('/') {
        Some((collection, rest)) => format!("{collection}/resources/{rest}"),
        None => format!("{user_path}/resources"),
    }
}

/// Map a zip member path back to the user-facing form — `None` for
/// every member that is not `<collection>/resources/<rest>` with a
/// nonempty rest (`project.json`, misplaced root files, directory
/// entries).
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

/// Full-zip rewrite: copy every member (decompressed → recompressed,
/// deflate, original order, directory entries preserved), applying
/// the surgery to the target. Returns the new zip plus whether the
/// target was seen (remove's not-found proof; replace appends when
/// unseen).
fn rewrite_zip(
    zip_bytes: &[u8],
    target: &str,
    surgery: Surgery<'_>,
) -> Result<(Vec<u8>, bool), CoreError> {
    let mut archive = open_archive(zip_bytes)?;
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
            _ => &bytes,
        };
        writer
            .write_all(content)
            .map_err(|err| CoreError::Internal(format!("cannot rewrite zip: {err}")))?;
    }

    // Replace-appends-when-absent: the put-upsert semantics (a new
    // resource joins the zip at the end, member order otherwise
    // preserved).
    if !seen && let Surgery::Replace(content) = surgery {
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
    /// both a full miss and the degenerate no-slash form; a non-zip
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

        let degenerate = read_member(&zip, "ignition").expect_err("no-slash path must fail");
        assert!(
            matches!(degenerate, CoreError::NotFound { endpoint: None }),
            "the degenerate form maps to a member that cannot exist: {degenerate}"
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

    /// THE upsert pin: replacing an ABSENT member appends it (put can
    /// create new resources) — existing members and their order
    /// untouched.
    #[test]
    fn replace_member_appends_when_absent() {
        let zip = sample_zip();
        let out = replace_member(&zip, "ignition/script-python/e2e/brand-new", b"print('x')")
            .expect("append rewrites");
        assert_eq!(
            read_member(&out, "ignition/script-python/e2e/brand-new").expect("appended reads"),
            b"print('x')".to_vec()
        );
        assert_eq!(
            resource_members(&out).expect("re-list"),
            vec![
                "ignition/script-python/e2e/scratch".to_string(),
                "com.example/views/Dashboard/view.json".to_string(),
                "ignition/script-python/e2e/brand-new".to_string(),
            ],
            "the new member rides last; the originals keep their order"
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
}
