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
    let mut value: serde_json::Value =
        serde_json::from_slice(existing).map_err(|err| {
            CoreError::Internal(format!("parent resource descriptor is not valid JSON: {err}"))
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
        CoreError::Internal(format!("cannot serialize merged resource descriptor: {err}"))
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
            if target.rsplit('/').next().is_some_and(|base| base != FOLDER_DESCRIPTOR) =>
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
            (_, Surgery::Replace(_)) if matches!(&descriptor_surgery, Some(DescriptorSurgery::Merge(path)) if path == &name) => {
                // The parent descriptor rides MERGED: the new basename
                // joins its files list (idempotent), everything else
                // about it kept verbatim.
                &merge_descriptor_member(&bytes, target.rsplit('/').next().expect("non-root — checked above"))?
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
            .start_file("ignition/resources/script-python/uat/resource.json", options)
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
        let out =
            replace_member(&zip, "ignition/script-python/uat/hello3.py", b"print('new')'")
                .expect("append rewrites");
        let descriptor: serde_json::Value =
            serde_json::from_slice(
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
        let descriptor: serde_json::Value =
            serde_json::from_slice(
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
            .start_file("ignition/resources/script-python/uat/resource.json", options)
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
            .start_file("ignition/resources/script-python/uat/resource.json", options)
            .expect("starts");
        writer
            .write_all(br#"{"scope":"G","version":1,"files":["scratch.py"],"attributes":{}}"#)
            .expect("writes");
        writer
            .start_file("ignition/resources/script-python/uat/scratch.py", options)
            .expect("starts");
        writer.write_all(b"print('x')").expect("writes");
        let zip = writer.finish().expect("finalize").into_inner();

        let out = remove_member(&zip, "ignition/script-python/uat/scratch.py")
            .expect("remove rewrites");
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
}
