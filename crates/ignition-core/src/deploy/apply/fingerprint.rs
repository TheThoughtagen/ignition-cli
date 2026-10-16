//! The touched-set fingerprint (Phase 23, research Pattern 5, D-04).
//!
//! A fingerprint is the SHA-256 over one line per touched path, in byte order
//! of the path: `path NUL (sha256 or "-") LF`. A path that does not exist has
//! the hash [`FINGERPRINT_ABSENT`]. The ledger is one more entry, keyed
//! [`LEDGER_KEY`]; it cannot collide with a collection path, which has at
//! least three components.
//!
//! It is computed here, in Rust, over hashes the host reported. A host script
//! never computes it: PowerShell sorts by UTF-16 code units, which orders
//! some non-ASCII names differently from the byte order used here, so the
//! two runners would disagree on the same files.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

/// The hash recorded for a path that does not exist.
pub const FINGERPRINT_ABSENT: &str = "-";

/// The key of the ledger in a touched-set map.
pub const LEDGER_KEY: &str = "ledger.json";

/// The fingerprint of `entries` (path to lowercase hex SHA-256, `None` for
/// absent): 64 lowercase hex digits.
pub fn fingerprint(entries: &BTreeMap<String, Option<String>>) -> String {
    let mut hasher = Sha256::new();
    // A `BTreeMap<String, _>` iterates in byte order of the key (`str`
    // compares by UTF-8 bytes), which is the order the format pins.
    for (path, sha256) in entries {
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(sha256.as_deref().unwrap_or(FINGERPRINT_ABSENT).as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const SHA_ONE: &str = "7692c3ad3540bb803c020b3aee66cd8887123234ea0c6e7143c0add73ff431ed";
    const SHA_LEDGER: &str = "fe14010b4fe83303852f0467c919ef9a7ca089b91e96e3aad7d426dd87079297";
    const SHA_TWO: &str = "3fc4ccfe745870e2c0d99f71f30ff0656c8dedd41cc1d7d3d376b0dbe685e2f3";

    /// Derived independently in Python from the listing format in the module
    /// doc, over [`golden_entries`].
    const GOLDEN: &str = "ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79";

    fn golden_entries() -> BTreeMap<String, Option<String>> {
        BTreeMap::from([
            (
                "ignition/tag-provider/default/tags.json".to_string(),
                Some(SHA_ONE.to_string()),
            ),
            (LEDGER_KEY.to_string(), Some(SHA_LEDGER.to_string())),
            ("com.example/views/Main/view.json".to_string(), None),
        ])
    }

    #[test]
    fn fingerprint_golden() {
        let entries = golden_entries();
        assert_eq!(fingerprint(&entries), GOLDEN);

        // Insertion order does not matter.
        let mut reordered = BTreeMap::new();
        for (k, v) in entries.iter().rev() {
            reordered.insert(k.clone(), v.clone());
        }
        assert_eq!(fingerprint(&reordered), GOLDEN);

        // One sha changes the digest, and so does absent versus present.
        let mut changed = entries.clone();
        changed.insert(
            "ignition/tag-provider/default/tags.json".to_string(),
            Some(SHA_TWO.to_string()),
        );
        assert_ne!(fingerprint(&changed), GOLDEN);
        let mut present = entries.clone();
        present.insert(
            "com.example/views/Main/view.json".to_string(),
            Some(SHA_TWO.to_string()),
        );
        assert_ne!(fingerprint(&present), GOLDEN);
    }

    #[test]
    fn the_ledger_sorts_by_bytes_like_any_other_key() {
        let mut entries = golden_entries();
        entries.insert("zz/type/name".to_string(), None);
        let mut listing = Vec::new();
        for (path, sha) in [
            ("com.example/views/Main/view.json", None),
            ("ignition/tag-provider/default/tags.json", Some(SHA_ONE)),
            ("ledger.json", Some(SHA_LEDGER)),
            ("zz/type/name", None),
        ] {
            listing.extend_from_slice(path.as_bytes());
            listing.push(0);
            listing.extend_from_slice(sha.unwrap_or(FINGERPRINT_ABSENT).as_bytes());
            listing.push(b'\n');
        }
        let expected = format!("{:x}", Sha256::digest(&listing));
        assert_eq!(fingerprint(&entries), expected);
    }

    #[test]
    fn non_ascii_names_sort_by_utf8_bytes_not_utf16_units() {
        // U+FF5E sorts after U+1F600 in UTF-16 (0xFF5E > 0xD83D) but before
        // it in UTF-8 (EF BD 9E < F0 9F 98 80).
        let high = "mod/type/\u{ff5e}".to_string();
        let astral = "mod/type/\u{1f600}".to_string();
        let entries = BTreeMap::from([(astral.clone(), None), (high.clone(), None)]);
        let mut listing = Vec::new();
        for path in [&high, &astral] {
            listing.extend_from_slice(path.as_bytes());
            listing.extend_from_slice(b"\0-\n");
        }
        assert_eq!(
            fingerprint(&entries),
            format!("{:x}", Sha256::digest(&listing))
        );
    }

    #[test]
    fn an_empty_set_has_a_fingerprint() {
        let empty = BTreeMap::new();
        assert_eq!(
            fingerprint(&empty),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
