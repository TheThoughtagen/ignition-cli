//! Deterministic `attributes.uuid` for resources that render creates
//! (Phase 21, DRND-06, decision D-03).
//!
//! The uuid is RFC 9562 **version 8** (custom): the first 16 bytes of
//! `SHA-256(UUID_DOMAIN NUL gateway NUL module "/" type "/" name)`, with the
//! version nibble set to 8 and the variant bits set to `10`. D-03 chose
//! `sha2` (already a dependency) over a `sha1`/UUIDv5 or `uuid` crate.
//!
//! - **CD1 live check.** Decision CD1 in `21-LIVE-CAPTURES.md` records an
//!   8.3.6 gateway loading a hand-written resource carrying the uuid this
//!   function derives for `ign-deploy-rig` / `ignition` / `database-translator`
//!   / `CAP21_V8` (see `name_based_uuid_matches_live_vector`), so a version-8
//!   uuid is accepted and the derivation is frozen.
//! - **The environment is not an input.** One resource keeps one identity
//!   across a gateway's environments.
//! - **The gateway is an input.** Two gateway trees never share ids.
//! - **Inputs are exact UTF-8 bytes.** No normalization and no case folding:
//!   `Pump` and `pump`, or NFC and NFD spellings, are different resources.
//! - **Existing uuids are never recomputed.** Render only derives a uuid for
//!   a resource that has none.

use sha2::{Digest, Sha256};

/// Domain separation prefix hashed before the inputs. Changing it changes
/// every minted uuid, so it carries a version.
pub const UUID_DOMAIN: &str = "ign-deploy:uuid:v1";

/// The uuid render derives for resource `module/ty/name` of `gateway`, as
/// lowercase 8-4-4-4-12 hex.
pub fn name_based_uuid(gateway: &str, module: &str, ty: &str, name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(UUID_DOMAIN.as_bytes());
    hasher.update([0u8]);
    hasher.update(gateway.as_bytes());
    hasher.update([0u8]);
    hasher.update(module.as_bytes());
    hasher.update(b"/");
    hasher.update(ty.as_bytes());
    hasher.update(b"/");
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let mut b = [0u8; 16];
    b.copy_from_slice(&digest[..16]);
    b[6] = (b[6] & 0x0f) | 0x80;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Whether `s` is exactly 36 characters: hex digits (either case) with
/// hyphens at positions 8, 13, 18 and 23.
pub fn is_canonical_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_based_uuid_matches_live_vector() {
        // CD1 (21-LIVE-CAPTURES.md): this uuid was hand-written into
        // CAP21_V8/resource.json and the live 8.3.6 gateway loaded it.
        assert_eq!(
            name_based_uuid(
                "ign-deploy-rig",
                "ignition",
                "database-translator",
                "CAP21_V8"
            ),
            "4516ad9c-d6b0-8d86-89b9-802c96e33a9e"
        );
        // CD1 stale-signature capture used the same derivation.
        assert_eq!(
            name_based_uuid(
                "ign-deploy-rig",
                "ignition",
                "database-translator",
                "CAP21_STALE"
            ),
            "1caa175b-6703-89d8-8ead-c9b7c6bdfb96"
        );
    }

    #[test]
    fn uuid_is_version_8_and_rfc_variant() {
        for name in ["a", "b", "Pump 1", "é", "x/y"] {
            let u = name_based_uuid("g", "ignition", "schedule", name);
            assert!(is_canonical_uuid(&u), "{u}");
            assert_eq!(u, u.to_lowercase());
            let c: Vec<char> = u.chars().collect();
            assert_eq!(c[14], '8', "version nibble of {u}");
            assert!("89ab".contains(c[19]), "variant nibble of {u}");
        }
    }

    #[test]
    fn uuid_is_deterministic_and_input_sensitive() {
        let base = name_based_uuid("g", "m", "t", "n");
        assert_eq!(base, name_based_uuid("g", "m", "t", "n"));
        for other in [
            name_based_uuid("g2", "m", "t", "n"),
            name_based_uuid("g", "m2", "t", "n"),
            name_based_uuid("g", "m", "t2", "n"),
            name_based_uuid("g", "m", "t", "n2"),
        ] {
            assert_ne!(base, other);
        }
        assert_ne!(
            name_based_uuid("g", "m", "t", "Pump"),
            name_based_uuid("g", "m", "t", "pump")
        );
        // NFC (one code point) and NFD (e + combining acute) differ
        assert_ne!(
            name_based_uuid("g", "m", "t", "\u{e9}"),
            name_based_uuid("g", "m", "t", "e\u{301}")
        );
    }

    #[test]
    fn uuid_fields_are_separated() {
        assert_ne!(
            name_based_uuid("ab", "c", "t", "n"),
            name_based_uuid("a", "bc", "t", "n")
        );
        assert_ne!(
            name_based_uuid("g", "m", "ab", "c"),
            name_based_uuid("g", "m", "a", "bc")
        );
    }

    #[test]
    fn canonical_uuid_shape() {
        assert!(is_canonical_uuid("4516ad9c-d6b0-8d86-89b9-802c96e33a9e"));
        assert!(is_canonical_uuid("4516AD9C-D6B0-8D86-89B9-802C96E33A9E"));
        for bad in [
            "",
            "4516ad9cd6b08d8689b9802c96e33a9e",
            "4516ad9c-d6b0-8d86-89b9-802c96e33a9",
            "4516ad9c-d6b0-8d86-89b9-802c96e33a9ee",
            "4516ad9c-d6b0-8d86-89b9-802c96e33a9g",
            "{4516ad9c-d6b0-8d86-89b9-802c96e33a9e}",
            "4516ad9c_d6b0-8d86-89b9-802c96e33a9e",
        ] {
            assert!(!is_canonical_uuid(bad), "{bad:?}");
        }
    }
}
