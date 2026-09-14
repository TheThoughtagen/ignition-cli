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

use ignition_core::client::workspace::{local_path_for, segment_escape, segment_unescape};
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
        prop_assert_eq!(local.to_string_lossy(), member);
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
