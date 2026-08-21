//! Real-OS-keychain round-trip for `KeyringStore` — `#[ignore]`-gated so
//! default `cargo test` NEVER touches a real keychain (research Pitfall 8).
//! Runs only in the dedicated `keyring-smoke` CI job (gnome-keyring recipe,
//! verbatim from keyring-rs's own CI — the STATE.md blocker closure) or
//! explicitly: `cargo test -p ignition-core --test keyring_smoke -- --ignored`.

use ignition_core::config::{AuthRef, Credential, KeyringStore, Secret, SecretStore};

const PROFILE: &str = "ign-smoke-test";
const TOKEN: &str = "smoke-test-token-zzz";

/// set → resolve(Some) → delete → resolve(None) through `KeyringStore`.
#[test]
#[ignore = "touches the real OS keychain; run explicitly or in the keyring-smoke CI job"]
fn keyring_round_trip() {
    let store = KeyringStore;
    let auth = AuthRef::default();

    store
        .set(PROFILE, &Secret::new(TOKEN))
        .expect("set keyring entry");

    match store
        .resolve(PROFILE, &auth)
        .expect("resolve")
        .expect("entry present")
    {
        Credential::Token(token) => assert_eq!(token.expose(), TOKEN),
        Credential::Basic(..) => panic!("keyring stores tokens, not basic pairs"),
    }

    store.delete(PROFILE).expect("delete keyring entry");
    assert!(
        store.resolve(PROFILE, &auth).expect("resolve").is_none(),
        "entry must be gone after delete",
    );

    // Idempotent delete: removing a missing entry is success.
    store
        .delete(PROFILE)
        .expect("delete of missing entry is Ok");
}
