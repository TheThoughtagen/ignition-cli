//! The workspace engine's pure core (13-02) — the injective
//! user-path → local-fs-path mapping, plus (Task 3) the
//! [`MemberSource`] abstraction that lets the proven v1.0 member
//! engine ([`crate::client::resources`]) speak EITHER a gateway
//! export zip OR a checked-out directory tree.
//!
//! ## The escaping scheme (planner-locked, property-test-pinned)
//!
//! Member user paths from a gateway export are HOSTILE: two members
//! may differ only by case (`P13/A` vs `p13/a`), carry traversal
//! shapes (`..`), NULs, control bytes, arbitrary unicode (NFC vs NFD
//! decompositions), or percent signs. The mapping onto a checked-out
//! tree must be INJECTIVE — two members landing on one local path is
//! silent data loss at checkout and server-side deletion on push
//! (roadmap Pitfall W1) — and REVERSIBLE so push can walk back.
//!
//! Per-segment percent-encoding:
//!
//! - Safe segment bytes are `[A-Za-z0-9._-]`; names over the safe
//!   alphabet land on disk BYTE-IDENTICAL (diffable, editor-friendly).
//! - EVERY other byte — `%` itself, spaces, control chars, non-ASCII
//!   — percent-encodes as `%XX`. Because `%` is escaped too, `%XX`
//!   is the ONLY escape form and decoding is unambiguous (no
//!   double-decode trap: the literal name `%2e` round-trips as
//!   `%2e`, never collapsing into `.`).
//! - Refusals are fail-closed `CoreError::InvalidInput` naming the
//!   offending member path — never sanitized, never
//!   last-write-wins, and NO new exit slugs (planner lock: new slugs
//!   are Three-Place ATOMIC events; mapping refusals ride the
//!   existing `invalid_input` slug, exit 2):
//!   - a segment exactly `.` or `..` (traversal-shaped — refused,
//!     never escaped-around);
//!   - NUL anywhere in the member path;
//!   - an empty segment (`//`, leading or trailing slash) — same
//!     refusal class as `.`;
//!   - an ESCAPED segment exceeding 255 bytes — the on-disk name cap
//!     (APFS/ext4): the escaped form is the name that must land, so
//!     an oversized one refuses at mapping time rather than failing
//!     mid-write (escaping can inflate 1 byte to 3, so the cap is
//!     checked on the OUTPUT).
//! - ASCII-case collisions are refused at SET level by
//!   [`build_mapping`] (APFS is the deployment surface —
//!   case-insensitive by default): two members whose local paths
//!   differ only by ASCII case would fold onto one file, so the
//!   mapping build refuses naming BOTH member paths. Unicode case is
//!   NOT folded beyond ASCII — NFC/NFD variants differ byte-wise and
//!   stay distinct files (the recorded manifest holds the exact
//!   mapping either way).
//!
//! ## Recorded, never recomputed downstream
//!
//! [`build_mapping`] runs ONCE at checkout; the manifest (13-03)
//! stores the gateway↔local pairs. status/push READ the manifest —
//! re-deriving the mapping is Pitfall W1's documented anti-pattern.
//! These functions are the ONLY mapping implementation; the property
//! suite (`tests/workspace_path_mapping.rs`) machine-proves the
//! bijection/round-trip/refusal properties over hostile corpora —
//! SC-2's "property tests pass" at the mapping layer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::client::resources;
use crate::error::CoreError;

/// The maximum on-disk segment length the mapping will emit — the
/// classic 255-byte filesystem name cap. Checked on the ESCAPED form
/// (that is the byte string that must land on disk).
const MAX_ESCAPED_SEGMENT_BYTES: usize = 255;

/// The bytes a segment may carry through to disk UNESCAPED — the
/// planner-locked safe alphabet. Everything else rides as `%XX`.
fn is_safe_segment_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// Upper-case hex digits — the pinned `%XX` spelling (lowercase is
/// ACCEPTED on decode — valid hex is valid hex — but escape emits
/// exactly this form).
const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Escape ONE member-path segment for the local filesystem:
/// percent-encode every byte outside the safe alphabet. Refuses
/// (fail-closed, `invalid_input`):
/// - the exact segments `.` and `..` — traversal-shaped, refused
///   rather than escaped-around;
/// - the empty segment (same refusal class as `.`);
/// - any NUL byte;
/// - an escaped form longer than 255 bytes (the on-disk name cap —
///   checked on the OUTPUT, since escaping inflates).
///
/// The error names the offending segment; the member-path callers
/// ([`local_path_for`], [`build_mapping`]) wrap it with the full
/// member path.
pub fn segment_escape(segment: &str) -> Result<String, CoreError> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "workspace member segment is traversal-shaped or empty: \"{segment}\" — \
                 refusing rather than escaping around it"
            ),
        });
    }
    if segment.bytes().any(|byte| byte == 0) {
        return Err(CoreError::InvalidInput {
            reason: format!("workspace member segment carries a NUL byte: \"{segment}\""),
        });
    }
    let mut escaped = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if is_safe_segment_byte(byte) {
            escaped.push(byte as char);
        } else {
            escaped.push('%');
            escaped.push(char::from(HEX[usize::from(byte >> 4)]));
            escaped.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
    if escaped.len() > MAX_ESCAPED_SEGMENT_BYTES {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "workspace member segment escapes to {} bytes, over the {}-byte \
                 on-disk name cap: \"{segment}\"",
                escaped.len(),
                MAX_ESCAPED_SEGMENT_BYTES
            ),
        });
    }
    Ok(escaped)
}

/// The strict inverse of [`segment_escape`]: decode `%XX` (either hex
/// case) and reproduce the original bytes exactly. Malformed `%`
/// sequences (truncated, non-hex) refuse; a decoded result outside
/// the escape's image — NUL, `/`, `.`/`..`/empty — also refuses, so a
/// hand-written local path cannot smuggle path structure back
/// through decode (strict-image enforcement, property-pinned).
pub fn segment_unescape(segment: &str) -> Result<String, CoreError> {
    let bytes = segment.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let (high, low) = match bytes.get(index + 1..index + 3) {
            Some(pair) => (
                (pair[0] as char).to_digit(16),
                (pair[1] as char).to_digit(16),
            ),
            None => (None, None), // truncated — "%2" or a bare "%"
        };
        let (Some(high), Some(low)) = (high, low) else {
            return Err(CoreError::InvalidInput {
                reason: format!("malformed percent escape in workspace segment: \"{segment}\""),
            });
        };
        decoded.push((high * 16 + low) as u8);
        index += 3;
    }
    // Strict-image guards: segment_escape can never EMIT these.
    if decoded.contains(&0) {
        return Err(CoreError::InvalidInput {
            reason: format!("decoded workspace segment carries NUL: \"{segment}\""),
        });
    }
    if decoded.contains(&b'/') {
        return Err(CoreError::InvalidInput {
            reason: format!("decoded workspace segment carries a path separator: \"{segment}\""),
        });
    }
    let decoded = String::from_utf8(decoded).map_err(|_| CoreError::InvalidInput {
        reason: format!("decoded workspace segment is not valid UTF-8: \"{segment}\""),
    })?;
    if decoded.is_empty() || decoded == "." || decoded == ".." {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "decoded workspace segment is traversal-shaped or empty: \"{segment}\""
            ),
        });
    }
    Ok(decoded)
}

/// THE mapping: a gateway member user path → its local checkout
/// path. Split on `/` (member-path segment semantics — a slash
/// splits, never injects), escape each segment, join. Empty segments
/// (`//`, leading/trailing slash) refuse through [`segment_escape`],
/// and every refusal rides `invalid_input` naming the offending
/// member path. Injectivity and the exact round-trip are
/// machine-proven by the property suite (`tests/workspace_path_mapping.rs`).
pub fn local_path_for(member_user_path: &str) -> Result<PathBuf, CoreError> {
    let mut local = PathBuf::new();
    for segment in member_user_path.split('/') {
        let escaped = segment_escape(segment).map_err(|err| CoreError::InvalidInput {
            reason: format!("workspace member path \"{member_user_path}\" is not mappable: {err}"),
        })?;
        local.push(escaped);
    }
    Ok(local)
}

/// Build the WHOLE checkout mapping at once — what 13-03's checkout
/// calls exactly once, then records gateway↔local pairs into the
/// manifest (never recomputed downstream — Pitfall W1).
///
/// Set-level injectivity enforcement on top of the per-member
/// mapping:
/// - an exact duplicate member (the same path listed twice) refuses —
///   a caller bug, not something to absorb silently;
/// - any two members whose local paths are equal under ASCII
///   case-folding refuse, naming BOTH member paths (the APFS
///   Pitfall-W1 class: escaping preserves case, but the filesystem
///   folds it — `P13/A` and `p13/a` would silently overwrite at
///   checkout and delete server-side at push).
///
/// Deterministic: members iterate sorted, so the resulting map and
/// every error message are stable regardless of input order.
pub fn build_mapping(members: &[String]) -> Result<BTreeMap<String, PathBuf>, CoreError> {
    let mut sorted: Vec<&String> = members.iter().collect();
    sorted.sort();
    sorted.dedup();
    if sorted.len() != members.len() {
        let duplicate = members
            .iter()
            .find(|member| members.iter().filter(|other| *other == *member).count() > 1)
            .expect("length mismatch proves a duplicate exists");
        return Err(CoreError::InvalidInput {
            reason: format!(
                "workspace member list carries the exact duplicate \"{duplicate}\" — \
                 refusing rather than absorbing it"
            ),
        });
    }

    let mut mapping = BTreeMap::new();
    // folded (lower-cased) rendered local path → the member that owns it.
    let mut folded: BTreeMap<String, String> = BTreeMap::new();
    for member in sorted {
        let local = local_path_for(member)?;
        let rendered = local.to_string_lossy().into_owned();
        let key = rendered.to_ascii_lowercase();
        if let Some(owner) = folded.get(&key) {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "workspace members \"{owner}\" and \"{member}\" collide on the same \
                     case-insensitive local path \"{rendered}\" — one would silently \
                     overwrite the other at checkout"
                ),
            });
        }
        folded.insert(key, member.clone());
        mapping.insert(member.clone(), local);
    }
    Ok(mapping)
}

/// The member-content source abstraction (13-02 Task 3): the proven
/// v1.0 member engine ([`crate::client::resources`], zip-bytes-only)
/// speaks EITHER a gateway export zip OR a checked-out directory
/// tree, with ONE implementation behind both — 13-03's checkout and
/// 13-06's status compare build on this without touching proven code.
///
/// - [`MemberSource::Zip`] delegates VERBATIM to the existing
///   resources.rs functions — descriptor-normalized enumeration/
///   read/hash semantics stay single-source (no engine function is
///   copied here).
/// - [`MemberSource::Tree`] reads a checked-out tree through the
///   RECORDED mapping with the SAME member-hash semantics: a
///   member's hash is identical from either source for identical
///   bytes (equivalence pinned by test). A `resource.json` member
///   hashes its normalized descriptor exactly as the zip side does —
///   the same helpers ([`resources::normalize_descriptor`],
///   [`resources::fnv1a`], [`resources::FOLDER_DESCRIPTOR`]), never
///   a fork.
///
/// Tree strictness (pinned here): the tree is MANIFEST-SCOPED. A
/// mapped member missing from disk, or a local file not in the
/// mapping, is an error from the source — 13-06's status handles
/// unknown files at the action layer, where a human-facing verdict
/// belongs.
pub enum MemberSource {
    /// A gateway project-export zip's raw bytes.
    Zip(Vec<u8>),
    /// A checked-out directory tree rooted at `root`, addressed
    /// through the recorded checkout `mapping` (gateway member →
    /// local path — [`build_mapping`]'s output, stored in the
    /// manifest by 13-03).
    Tree {
        /// The checkout root the recorded local paths ride under.
        root: PathBuf,
        /// The recorded gateway-member → local-path pairs.
        mapping: BTreeMap<String, PathBuf>,
    },
}

impl MemberSource {
    /// Every resource member's user path. Zip: the export walk in
    /// member order (the proven [`resources::resource_members`]
    /// semantics — `project.json`, directory entries, and
    /// non-`resources`-shaped members skipped). Tree: the recorded
    /// mapping's keys, sorted.
    pub fn members(&self) -> Result<Vec<String>, CoreError> {
        match self {
            Self::Zip(bytes) => resources::resource_members(bytes),
            Self::Tree { mapping, .. } => Ok(mapping.keys().cloned().collect()),
        }
    }

    /// One member's bytes, verbatim. Zip: the proven
    /// [`resources::read_member`] (missing → `not_found`). Tree:
    /// `fs::read` at the mapped local path — a member outside the
    /// recorded mapping or a missing file is `invalid_input` naming
    /// the member (the tree is manifest-scoped, so "unmapped" is a
    /// caller/manifest contract break, not a 404-shaped lookup miss).
    pub fn read(&self, user_path: &str) -> Result<Vec<u8>, CoreError> {
        match self {
            Self::Zip(bytes) => resources::read_member(bytes, user_path),
            Self::Tree { root, mapping } => {
                let local = mapping
                    .get(user_path)
                    .ok_or_else(|| CoreError::InvalidInput {
                        reason: format!(
                            "workspace member \"{user_path}\" is not in the recorded \
                             checkout mapping"
                        ),
                    })?;
                std::fs::read(root.join(local)).map_err(|err| CoreError::InvalidInput {
                    reason: format!(
                        "workspace member \"{user_path}\" cannot be read from the \
                         checkout tree: {err}"
                    ),
                })
            }
        }
    }

    /// User path → FNV-1a digest for every member — the SAME
    /// descriptor rule both sides: a basename `resource.json` hashes
    /// its [`resources::normalize_descriptor`] output (the
    /// lastModification volatility guard), everything else hashes raw
    /// bytes. Zip: the proven [`resources::member_hashes`]. Tree:
    /// reads the mapped files after a strictness walk — a mapped
    /// member missing from disk, or an on-disk file absent from the
    /// mapping, is `invalid_input` (manifest-scoped tree).
    pub fn member_hashes(&self) -> Result<BTreeMap<String, u64>, CoreError> {
        match self {
            Self::Zip(bytes) => resources::member_hashes(bytes),
            Self::Tree { root, mapping } => {
                let found = scan_regular_files(root)?;
                for (member, local) in mapping {
                    if !found.contains(local) {
                        return Err(CoreError::InvalidInput {
                            reason: format!(
                                "workspace member \"{member}\" is missing from the \
                                 checkout tree (expected at \"{}\")",
                                local.to_string_lossy()
                            ),
                        });
                    }
                }
                for file in &found {
                    if !mapping.values().any(|local| local == file) {
                        return Err(CoreError::InvalidInput {
                            reason: format!(
                                "checkout tree contains local file \"{}\" that is not \
                                 in the recorded mapping — the tree is manifest-scoped",
                                file.to_string_lossy()
                            ),
                        });
                    }
                }
                let mut hashes = BTreeMap::new();
                for (member, local) in mapping {
                    let bytes =
                        std::fs::read(root.join(local)).map_err(|err| CoreError::InvalidInput {
                            reason: format!(
                                "workspace member \"{member}\" cannot be read from \
                                     the checkout tree: {err}"
                            ),
                        })?;
                    let is_descriptor = local.file_name()
                        == Some(std::ffi::OsStr::new(resources::FOLDER_DESCRIPTOR));
                    let content = if is_descriptor {
                        resources::normalize_descriptor(&bytes).unwrap_or(bytes)
                    } else {
                        bytes
                    };
                    hashes.insert(member.clone(), resources::fnv1a(&content));
                }
                Ok(hashes)
            }
        }
    }
}

/// Every regular file under `root`, as paths relative to it — the
/// strictness walk backing [`MemberSource::Tree::member_hashes`].
fn scan_regular_files(root: &std::path::Path) -> Result<BTreeSet<PathBuf>, CoreError> {
    fn walk(
        dir: &std::path::Path,
        prefix: &std::path::Path,
        found: &mut BTreeSet<PathBuf>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let relative = prefix.join(entry.file_name());
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                walk(&entry.path(), &relative, found)?;
            } else if file_type.is_file() {
                found.insert(relative);
            }
        }
        Ok(())
    }
    let mut found = BTreeSet::new();
    walk(root, std::path::Path::new(""), &mut found)
        .map_err(|err| CoreError::Internal(format!("cannot walk checkout tree {root:?}: {err}")))?;
    Ok(found)
}
