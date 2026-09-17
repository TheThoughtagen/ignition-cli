//! The workspace path-mapping property suite (13-02) — SC-2's machine
//! proof that the user-path → fs-path mapping is INJECTIVE and
//! hostile-name-safe, not an example table: randomized corpora over
//! case pairs, traversal shapes, control bytes, percent traps,
//! unicode NFC/NFD decompositions, deep nesting, and the length cap,
//! with proptest shrinking on failure naming the exact offender.
//!
//! Properties:
//! - P1 round-trip/bijection — escape then unescape is the identity
//!   over every mappable member path (hostile corpus included).
//! - P2 injectivity — two DISTINCT member paths never render the
//!   same local path byte-wise (case-variant siblings included; the
//!   case-FOLD class is the set-level refusal, P6).
//! - P3 refusal — NUL anywhere, `.`/`..` segments, empty segments
//!   (`//`, leading/trailing slash) are `CoreError::InvalidInput`
//!   naming the offending member path (fail-closed, never sanitized,
//!   never last-write-wins).
//! - P4 idempotence — the safe domain (`[A-Za-z0-9._-]`) maps to
//!   itself byte-identically (the diffability guarantee).

use std::path::Path;

use ignition_core::client::workspace::{
    build_mapping, local_path_for, segment_escape, segment_unescape,
};
use ignition_core::error::CoreError;
use proptest::prelude::*;
use proptest::string::string_regex;

// ---- Corpus generators (the hostile corpora SC-2 mandates) ----

/// A hostile-but-structural segment: any unicode EXCEPT NUL and the
/// `/` separator (a slash splits, never injects), 1–24 scalars —
/// so it includes `%`, spaces, control bytes, emoji, and
/// combining marks. Traversal-shaped segments (`.`/`..`) are excluded
/// here — they belong to the P3 refusal corpus, not the mappable one.
fn hostile_segment() -> impl Strategy<Value = String> {
    string_regex(r"[^\x00/]{1,24}")
        .expect("hostile segment regex compiles")
        .prop_filter("`.`/`..` refuse (P3 corpus)", |s| s != "." && s != "..")
}

/// Mappable member paths: shallow ordinary, plus deep nesting (the
/// checkout tree is directory-shaped — 8–20 segments exercise the
/// join/unescape loop at depth).
fn valid_member_path() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => proptest::collection::vec(hostile_segment(), 1..=4)
            .prop_map(|segments| segments.join("/")),
        1 => proptest::collection::vec(hostile_segment(), 8..=20)
            .prop_map(|segments| segments.join("/")),
    ]
}

/// The refusal corpus: NUL anywhere, a `.`/`..` segment mid-path, an
/// empty segment, a leading slash, a trailing slash.
fn refused_member_path() -> impl Strategy<Value = String> {
    prop_oneof![
        // NUL embedded inside a segment (anywhere = any position via a/b/c).
        (hostile_segment(), hostile_segment(), hostile_segment())
            .prop_map(|(a, b, c)| format!("{a}\0{b}/{c}")),
        // A traversal-shaped segment mid-path.
        (
            hostile_segment(),
            hostile_segment(),
            prop::sample::select(vec![".", ".."]),
            hostile_segment()
        )
            .prop_map(|(a, b, bad, c)| format!("{a}/{b}/{bad}/{c}")),
        // Empty segment — `//` mid-path (planner decision: same
        // refusal class as `.`).
        (valid_member_path(), valid_member_path()).prop_map(|(a, b)| format!("{a}//{b}")),
        // Leading slash (empty first segment).
        valid_member_path().prop_map(|a| format!("/{a}")),
        // Trailing slash (empty last segment).
        valid_member_path().prop_map(|a| format!("{a}/")),
    ]
}

/// The safe domain: every byte in `[A-Za-z0-9._-]` — names that must
/// land on disk byte-identically (P4). `.`/`..` are dot-runs over the
/// safe alphabet, so the exact-traversal segments filter out here.
fn safe_member_path() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        string_regex(r"[A-Za-z0-9._-]{1,32}")
            .expect("safe segment regex compiles")
            .prop_filter("`.`/`..` refuse", |s| s != "." && s != ".."),
        1..=5,
    )
    .prop_map(|segments| segments.join("/"))
}

// ---- P1–P4 (the driving spec) ----

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// P1 — the mapping is a bijection: escape-each-segment then
    /// unescape-each-component reproduces the member path EXACTLY,
    /// over the full hostile corpus (case pairs, `%`, spaces,
    /// control bytes, NFC/NFD unicode, deep nesting).
    #[test]
    fn p1_round_trip_is_exact(member in valid_member_path()) {
        let local = local_path_for(&member)
            .unwrap_or_else(|err| panic!("generated path {member:?} must map, got {err}"));
        prop_assert_eq!(unescape_local(&local), member);
    }

    /// P2 — injectivity byte-wise: two DISTINCT member paths never
    /// render the same local path. Escaping preserves case, so
    /// `Foo`/`foo` stay byte-distinct here (their fold collision is
    /// the SET-level refusal — Task 2's P6).
    #[test]
    fn p2_injective_byte_wise(a in valid_member_path(), b in valid_member_path()) {
        prop_assume!(a != b, "distinct paths only — equal paths trivially map equal");
        let local_a = local_path_for(&a)
            .unwrap_or_else(|err| panic!("path {a:?} must map, got {err}"));
        let local_b = local_path_for(&b)
            .unwrap_or_else(|err| panic!("path {b:?} must map, got {err}"));
        prop_assert_ne!(
            local_a.to_string_lossy(),
            local_b.to_string_lossy(),
            "distinct members {:?} and {:?} collapsed onto one local path",
            a,
            b
        );
    }

    /// P3 — the refusal class is fail-closed: NUL anywhere, `.`/`..`
    /// segments, empty segments all REFUSE (never Ok, never
    /// sanitized), and every refusal names the offending member path.
    #[test]
    fn p3_refusals_name_the_member(bad in refused_member_path()) {
        let CoreError::InvalidInput { reason } = local_path_for(&bad).expect_err(
            "refusal-corpus path must refuse, never sanitize",
        ) else {
            panic!("refusal must ride InvalidInput (no new slugs)");
        };
        prop_assert!(
            reason.contains(&bad),
            "refusal must name the offending member path {:?}; got: {}",
            bad,
            reason
        );
    }

    /// P4 — the safe domain is a fixed point: names over
    /// `[A-Za-z0-9._-]` map to themselves byte-identically (the
    /// ordinary-name diffability guarantee).
    #[test]
    fn p4_safe_domain_is_idempotent(member in safe_member_path()) {
        let local = local_path_for(&member)
            .unwrap_or_else(|err| panic!("safe path {member:?} must map, got {err}"));
        prop_assert_eq!(
            ignition_core::client::scripts_codec::tree_relative_string(&local),
            member,
            "tree-relative rendering is the bijection's /-form (host separators never leak)"
        );
    }
}

/// Walk a mapped local path back to its member form — the push-side
/// half of the bijection, exactly as the tests exercise it: unescape
/// every component, re-join with the member-path separator.
fn unescape_local(local: &Path) -> String {
    let mut parts = Vec::new();
    for component in local.components() {
        let segment = component
            .as_os_str()
            .to_str()
            .expect("the mapping emits UTF-8-only names");
        parts.push(
            segment_unescape(segment)
                .unwrap_or_else(|err| panic!("component {segment:?} must unescape: {err}")),
        );
    }
    parts.join("/")
}

// ---- P5–P6: set-level injectivity (Task 2 — build_mapping) ----

/// A generated member SET (deduplicated by construction — exact
/// duplicates are the caller-bug refusal, pinned separately).
fn member_set() -> impl Strategy<Value = std::collections::BTreeSet<String>> {
    proptest::collection::btree_set(valid_member_path(), 1..=8)
}

proptest! {
    /// P5 — for every generated member SET that passes
    /// [`build_mapping`], the result is a TOTAL function (every input
    /// member present as a key) and pairwise-injective byte-wise.
    #[test]
    fn p5_build_mapping_total_and_pairwise_injective(members in member_set()) {
        let input: Vec<String> = members.into_iter().collect();
        let mapping = match build_mapping(&input) {
            Ok(mapping) => mapping,
            Err(_) => {
                prop_assume!(false, "refused set — the fold-collision class is P6's corpus");
                unreachable!()
            }
        };
        prop_assert_eq!(mapping.len(), input.len(), "total: one entry per member");
        for member in &input {
            prop_assert!(mapping.contains_key(member), "member {:?} missing from map", member);
        }
        let rendered: Vec<String> = mapping
            .values()
            .map(|local| local.to_string_lossy().into_owned())
            .collect();
        for (index, a) in rendered.iter().enumerate() {
            for b in rendered.iter().skip(index + 1) {
                prop_assert_ne!(a, b, "two members rendered the same local path");
            }
        }
    }
}

/// P6 — the APFS fold-collision class (Pitfall W1's core) refuses
/// naming BOTH member paths, and the refusal message is order-stable
/// (deterministic sorted iteration).
#[test]
fn p6_case_fold_collision_refuses_naming_both() {
    for (a, b) in [("P13/A", "p13/a"), ("Foo", "foo")] {
        let err = build_mapping(&[a.to_string(), b.to_string()])
            .expect_err("fold-colliding members must refuse");
        let CoreError::InvalidInput { reason } = err else {
            panic!("refusal must ride InvalidInput (no new slugs)");
        };
        assert!(
            reason.contains(a) && reason.contains(b),
            "refusal must name BOTH colliding members ({a:?}, {b:?}): {reason}"
        );

        // Order-stable: the same pair reversed refuses with the SAME
        // message (sorted iteration — 13-03's error surfaces verbatim
        // regardless of member order in the export).
        let reversed = build_mapping(&[b.to_string(), a.to_string()]).expect_err("reversed");
        let CoreError::InvalidInput {
            reason: reason_reversed,
        } = reversed
        else {
            panic!("refusal must ride InvalidInput (no new slugs)");
        };
        assert_eq!(reason, reason_reversed, "collision message is order-stable");
    }

    // Sanity: same folded DIRECTORY, different folded FILE — no
    // collision (`P13/A/x` folds to `p13/a/x`, `p13/B` to `p13/b`).
    let ok = build_mapping(&["P13/A/x".to_string(), "p13/B".to_string()])
        .expect("distinct folded paths map");
    assert_eq!(ok.len(), 2);
}

/// P6 — the exact duplicate refuses (the same member listed twice is
/// a caller bug, not something to absorb silently).
#[test]
fn p6_exact_duplicate_refuses() {
    let err = build_mapping(&[
        "ignition/script-python/a".to_string(),
        "ignition/script-python/a".to_string(),
    ])
    .expect_err("exact duplicate must refuse");
    let CoreError::InvalidInput { reason } = err else {
        panic!("refusal must ride InvalidInput (no new slugs)");
    };
    assert!(
        reason.contains("ignition/script-python/a"),
        "refusal names the duplicate: {reason}"
    );
}

/// Unicode case is NOT folded beyond ASCII (planner pin): NFC and NFD
/// variants differ byte-wise and both map — staying distinct files,
/// the recorded manifest holding the exact pairs either way.
#[test]
fn p6_unicode_variants_are_not_fold_collisions() {
    let nfc = "caf\u{e9}/view.json".to_string();
    let nfd = "cafe\u{301}/view.json".to_string();
    let mapping = build_mapping(&[nfc.clone(), nfd.clone()])
        .expect("byte-distinct members are not fold collisions");
    assert_eq!(mapping.len(), 2);
    assert_ne!(mapping[&nfc], mapping[&nfd]);
}

/// build_mapping is deterministic: the same set in any order yields
/// the identical map (13-03 records stable pairs regardless of
/// export member order).
#[test]
fn build_mapping_is_order_stable() {
    let members: Vec<String> = vec![
        "ignition/script-python/b".into(),
        "com.example/views/Dash/view.json".into(),
        "ignition/script-python/a".into(),
    ];
    let mut reversed = members.clone();
    reversed.reverse();
    assert_eq!(
        build_mapping(&members).unwrap(),
        build_mapping(&reversed).unwrap()
    );
}

// ---- Task 3: MemberSource — Zip vs Tree equivalence (one engine) ----

use ignition_core::client::resources;
use ignition_core::client::workspace::MemberSource;

/// The equivalence fixture: `project.json` + four members written in
/// SORTED user-path order (so the zip walk's member order equals the
/// tree's BTreeMap order and the `members()` lists compare directly):
/// - `com.example/views/Dashboard/view.json` (plain bytes)
/// - `ignition/my file%20/x.json` — the HOSTILE name (space + literal
///   `%20`): rides the escaped local name `my%20file%2520`
/// - `ignition/script-python/e2e/scratch` (plain bytes)
/// - `ignition/script-python/uat/resource.json` — a descriptor
///   carrying the lastModification volatility attributes
fn member_fixture_zip() -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("project.json", options)
        .expect("project.json starts");
    writer
        .write_all(br#"{"title":"T","enabled":true}"#)
        .expect("project.json writes");
    for (member, bytes) in [
        (
            "com.example/resources/views/Dashboard/view.json",
            br#"{"scope":"A"}"#.as_slice(),
        ),
        (
            "ignition/resources/my file%20/x.json",
            br#"{"payload":"hostile name rides fine"}"#.as_slice(),
        ),
        ("ignition/resources/script-python/e2e/scratch", b"print('e2e')"),
        (
            "ignition/resources/script-python/uat/resource.json",
            br#"{"scope":"G","version":1,"files":["scratch.py"],"attributes":{"lastModification":{"actor":"admin","timestamp":"2026-08-28T10:00:00Z","signature":"sig-a"},"lastModificationSignature":"sig-a","notes":"kept"}}"#.as_slice(),
        ),
    ] {
        writer.start_file(member, options).expect("member starts");
        writer.write_all(bytes).expect("member writes");
    }
    writer.finish().expect("zip finalizes").into_inner()
}

/// The tree-side descriptor byte rewrite: SAME semantics, DIFFERENT
/// lastModification volatility (as if a checkout-side edit refreshed
/// them). The normalized hash must still match the zip side — the
/// volatility guard rides BOTH sources.
fn rewritten_descriptor(bytes: &[u8]) -> Vec<u8> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).expect("descriptor json");
    let last_modification = value
        .get_mut("attributes")
        .and_then(|attributes| attributes.get_mut("lastModification"))
        .and_then(|last| last.as_object_mut())
        .expect("descriptor carries lastModification");
    last_modification.insert(
        "timestamp".to_string(),
        serde_json::json!("2026-08-29T11:30:00Z"),
    );
    last_modification.insert("signature".to_string(), serde_json::json!("sig-b"));
    serde_json::to_vec(&value).expect("rewritten descriptor serializes")
}

/// Populate a tempdir checkout from the fixture zip through
/// build_mapping — exactly what 13-03's checkout will do. Returns
/// (tempdir, mapping, fixture zip bytes).
fn populated_tree() -> (
    tempfile::TempDir,
    std::collections::BTreeMap<String, std::path::PathBuf>,
    Vec<u8>,
) {
    let zip_bytes = member_fixture_zip();
    let user_paths = resources::resource_members(&zip_bytes).expect("fixture zip walks");
    let mapping = build_mapping(&user_paths).expect("fixture members map");
    let temp = tempfile::tempdir().expect("tempdir");
    for (member, local) in &mapping {
        let target = temp.path().join(local);
        std::fs::create_dir_all(target.parent().expect("nested member")).expect("checkout dirs");
        let bytes = resources::read_member(&zip_bytes, member).expect("fixture member reads");
        let bytes = if member.ends_with("/resource.json") {
            rewritten_descriptor(&bytes)
        } else {
            bytes
        };
        std::fs::write(&target, bytes).expect("checkout file writes");
    }
    (temp, mapping, zip_bytes)
}

/// THE equivalence pin: the same members sourced from the export zip
/// and from a checked-out tree carry IDENTICAL member lists and
/// IDENTICAL member-hash maps — including the descriptor normalized
/// under DIFFERENT lastModification values on each side (the tree
/// file was rewritten) and the hostile-name member round-tripped
/// byte-exact through the escaped local name.
#[test]
fn zip_and_tree_sources_are_equivalent() {
    let (temp, mapping, zip_bytes) = populated_tree();
    let zip_source = MemberSource::Zip(zip_bytes);
    let tree_source = MemberSource::Tree {
        root: temp.path().to_path_buf(),
        mapping,
    };

    assert_eq!(
        zip_source.members().expect("zip members"),
        tree_source.members().expect("tree members"),
        "identical member lists from both sources"
    );
    assert_eq!(
        zip_source.member_hashes().expect("zip hashes"),
        tree_source.member_hashes().expect("tree hashes"),
        "identical member hashes from both sources — descriptor normalization \
         and hostile-name escaping ride BOTH"
    );

    // Byte-level read equality: an ordinary member and the hostile
    // name (space + literal `%20` through its escaped local path).
    for member in [
        "ignition/script-python/e2e/scratch",
        "ignition/my file%20/x.json",
    ] {
        assert_eq!(
            zip_source.read(member).expect("zip read"),
            tree_source.read(member).expect("tree read"),
            "byte-identical read from both sources: {member:?}"
        );
    }
}

/// THE tree strictness pins (manifest-scoped source): an on-disk file
/// absent from the mapping refuses (named); a mapped member missing
/// from disk refuses (named); reads of missing/unmapped members ride
/// `invalid_input` naming the member; `members()` stays
/// mapping-scoped regardless.
#[test]
fn tree_source_is_manifest_scoped() {
    let (temp, mapping, _) = populated_tree();
    let tree = MemberSource::Tree {
        root: temp.path().to_path_buf(),
        mapping,
    };

    // 1. An unknown local file refuses, named.
    std::fs::write(temp.path().join("rogue.txt"), b"rogue").expect("rogue writes");
    let CoreError::InvalidInput { reason } = tree.member_hashes().expect_err("rogue file refuses")
    else {
        panic!("strictness refusal must ride InvalidInput");
    };
    assert!(reason.contains("rogue.txt"), "rogue named: {reason}");
    std::fs::remove_file(temp.path().join("rogue.txt")).expect("rogue removed");

    // 2. A mapped member deleted from disk refuses, named.
    let victim = tree
        .members()
        .expect("members list")
        .first()
        .expect("nonempty")
        .clone();
    let MemberSource::Tree { root, mapping } = &tree else {
        panic!("tree variant");
    };
    let victim_local = mapping.get(&victim).expect("victim is mapped").clone();
    std::fs::remove_file(root.join(&victim_local)).expect("victim removed");
    let CoreError::InvalidInput { reason } =
        tree.member_hashes().expect_err("missing member refuses")
    else {
        panic!("strictness refusal must ride InvalidInput");
    };
    assert!(
        reason.contains(&victim),
        "missing member named: {reason} (victim: {victim})"
    );

    // 3. Reads: missing-but-mapped and unmapped both ride
    //    `invalid_input` naming the member.
    let CoreError::InvalidInput { reason } = tree.read(&victim).expect_err("missing read") else {
        panic!("read refusal must ride InvalidInput");
    };
    assert!(reason.contains(&victim));
    let CoreError::InvalidInput { reason } =
        tree.read("nope/never-mapped").expect_err("unmapped read")
    else {
        panic!("read refusal must ride InvalidInput");
    };
    assert!(reason.contains("nope/never-mapped"));

    // 4. `members()` stays mapping-scoped — no tree walk at all.
    assert!(
        tree.members().expect("members").contains(&victim),
        "members() is the recorded mapping's keys, whatever the disk says"
    );
}

/// The Zip variant is a VERBATIM delegation: it answers exactly what
/// the proven resources.rs functions answer (no re-implementation
/// drift possible — the calls ARE those functions).
#[test]
fn zip_source_matches_resources_functions_directly() {
    let zip_bytes = member_fixture_zip();
    let source = MemberSource::Zip(zip_bytes.clone());
    assert_eq!(
        source.members().expect("source members"),
        resources::resource_members(&zip_bytes).expect("direct members")
    );
    assert_eq!(
        source.member_hashes().expect("source hashes"),
        resources::member_hashes(&zip_bytes).expect("direct hashes")
    );
}

// ---- Explicit fixtures pinning the scheme's sharp edges ----

/// Case-variant siblings map BYTE-DISTINCT (per-member injectivity
/// holds at the path layer; the fold class is build_mapping's refusal).
#[test]
fn case_variant_siblings_map_byte_distinct() {
    let upper = local_path_for("P13/Foo").expect("upper maps");
    let lower = local_path_for("P13/foo").expect("lower maps");
    assert_ne!(upper.to_string_lossy(), lower.to_string_lossy());
}

/// The percent-trap pins: `%XX` is the ONLY escape form because `%`
/// itself escapes — a literal `%2e` in a member name must NEVER
/// collapse into `.` on the way through (the double-decode trap).
#[test]
fn percent_names_round_trip_without_double_decode() {
    for name in ["100%20done", "%2e%2e%2ftrick", "%25", "a%2fb", "%41%42"] {
        let local = local_path_for(&format!("ignition/{name}"))
            .unwrap_or_else(|err| panic!("percent name {name:?} must map: {err}"));
        let on_disk = local.file_name().expect("last segment").to_str().unwrap();
        assert_eq!(
            segment_unescape(on_disk).unwrap_or_else(|err| panic!("{on_disk:?}: {err}")),
            name,
            "no double decode: the member name survives verbatim"
        );
    }
}

/// Control bytes ride as `%XX` — pinned spelling.
#[test]
fn control_bytes_escape_to_hex() {
    assert_eq!(
        local_path_for("a\u{1}b/x")
            .expect("control byte maps")
            .to_string_lossy(),
        "a%01b/x"
    );
    assert_eq!(
        segment_escape("tab\tsep").expect("tab escapes"),
        "tab%09sep"
    );
}

/// Ordinary safe names land byte-identical; spaces escape reversibly.
#[test]
fn safe_names_stay_legible_spaces_escape() {
    assert_eq!(
        local_path_for("readme.md")
            .expect("plain name")
            .to_string_lossy(),
        "readme.md"
    );
    assert_eq!(
        local_path_for("my notes")
            .expect("spaced name")
            .to_string_lossy(),
        "my%20notes"
    );
}

/// NFC (`é` U+00E9) and NFD (`e`+U+0301) variants round-trip EXACTLY
/// and stay byte-DISTINCT — no unicode folding beyond ASCII (the
/// planner-pinned policy; the recorded manifest holds the mapping).
#[test]
fn nfc_nfd_variants_round_trip_and_stay_distinct() {
    let nfc = "caf\u{e9}/view.json".to_string();
    let nfd = "cafe\u{301}/view.json".to_string();
    let local_nfc = local_path_for(&nfc).expect("nfc maps");
    let local_nfd = local_path_for(&nfd).expect("nfd maps");
    assert_ne!(
        local_nfc.to_string_lossy(),
        local_nfd.to_string_lossy(),
        "NFC and NFD are distinct byte sequences and stay distinct files"
    );
    assert_eq!(unescape_local(&local_nfc), nfc);
    assert_eq!(unescape_local(&local_nfd), nfd);
}

/// Traversal shapes refuse EXACTLY — the segment, not the dots: a
/// run of dots inside a longer name (`...`, `..hidden`, `a..b`) is an
/// ordinary safe name and MUST map.
#[test]
fn traversal_refuses_exactly_dots_in_names_map() {
    for path in [".", "..", "./a", "../a", "a/.", "a/..", "a/./b", "a/../b"] {
        let err = local_path_for(path).expect_err("traversal-shaped path must refuse");
        let CoreError::InvalidInput { reason } = err else {
            panic!("refusal must ride InvalidInput");
        };
        assert!(
            reason.contains(path),
            "refusal must name {path:?}: {reason}"
        );
    }
    for name in ["...", "..hidden", "a..b", ".hidden"] {
        assert!(
            local_path_for(name).is_ok(),
            "dots inside a name are ordinary safe bytes: {name:?}"
        );
    }
}

/// NUL anywhere refuses — never a silent byte in a local name.
#[test]
fn nul_anywhere_refuses() {
    for path in ["a\0b", "\0", "a/b\0", "\0/leading"] {
        let err = local_path_for(path).expect_err("NUL must refuse");
        let CoreError::InvalidInput { reason } = err else {
            panic!("refusal must ride InvalidInput");
        };
        assert!(reason.contains(path));
    }
}

/// The 255-byte cap is on the ESCAPED segment (that is the on-disk
/// name): a 255-byte safe name passes byte-identically; 256 refuses;
/// escaping inflation refuses sooner (85 spaces = exactly 255 escaped
/// bytes passes; 86 = 258 refuses). Refusals name the member path.
#[test]
fn length_cap_on_escaped_form() {
    let max = "a".repeat(255);
    assert_eq!(
        local_path_for(&max)
            .expect("exactly at cap")
            .to_string_lossy(),
        max
    );

    let err = local_path_for(&"a".repeat(256)).expect_err("over cap must refuse");
    let CoreError::InvalidInput { reason } = err else {
        panic!("cap refusal must ride InvalidInput");
    };
    assert!(
        reason.contains(&"a".repeat(256)),
        "refusal names the member"
    );

    assert!(
        local_path_for(&" ".repeat(85)).is_ok(),
        "85 spaces = 255 escaped"
    );
    assert!(
        local_path_for(&" ".repeat(86)).is_err(),
        "86 spaces = 258 escaped"
    );
}

/// `segment_unescape` is the STRICT inverse: malformed `%` sequences
/// (truncated, bad hex) refuse, and decoded results outside the
/// escape image (NUL, `/`, `.`, `..`) refuse too — a hand-written
/// local path cannot smuggle structure back through decode.
#[test]
fn unescape_is_strict_inverse() {
    for bad in [
        "%", "%2", "%zz", "%2G", "abc%", "a%2", "%00", "%2e", "%2e%2e", "%2f",
    ] {
        assert!(
            segment_unescape(bad).is_err(),
            "malformed or out-of-image segment must refuse: {bad:?}"
        );
    }
    assert_eq!(
        segment_unescape("a%20b%2Ec").expect("valid escapes decode"),
        "a b.c"
    );
    // Direct round trip on a segment carrying everything hostile.
    let hostile = "caf\u{e9} \u{1} 100%20 %2e";
    let escaped = segment_escape(hostile).expect("hostile segment escapes");
    assert_eq!(
        segment_unescape(&escaped).expect("escaped decodes"),
        hostile,
        "escape/unescape round-trips exactly"
    );
}
