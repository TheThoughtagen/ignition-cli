//! Secrets: the `Secret` newtype (type-level redaction), the `SecretStore`
//! seam, and env-first resolution (research Pattern 3 — the STATE.md keyring
//! blocker resolution).
//!
//! LOCKED resolution order (CORE-02 must-have):
//! `IGNITION_TOKEN_<PROFILE>` → profile `token_env` name → `IGNITION_TOKEN`
//! → keyring entry → `IGNITION_USER`+`IGNITION_PASSWORD`.
//! The order lives in the STORE CHAIN (see the unit tests and, from 01-04,
//! the dispatch construction site) — which is why the basic env pair is a
//! separate [`BasicEnvStore`] placed AFTER [`KeyringStore`]: env tokens
//! first, keyring second, basic env last, exactly as locked.
//!
//! The env-first order means default CI and tests never need a secret
//! service at all; `KeyringStore` fails soft (warn + skip) wherever no OS
//! keyring exists (headless Linux without D-Bus — keyring-rs fails fast at
//! `Entry::new`, never hangs).

use crate::config::AuthRef;
use crate::error::CoreError;

/// A value that must never render. NO `Serialize` impl exists on purpose
/// (type-level redaction: it cannot appear in JSON output); `Debug`/`Display`
/// render `***` so tracing logs are safe by construction (CORE-02).
#[derive(Clone)]
pub struct Secret(String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The ONLY way to read the value — grep-auditable: the single future
    /// read site is the reqwest header construction in 01-04.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// A resolved credential: token OR basic pair — never both (that rule is
/// enforced at the header-construction site in 01-04).
#[derive(Debug, Clone)]
pub enum Credential {
    /// Bearer-style API token.
    Token(Secret),
    /// Basic-auth user/password pair.
    Basic(Secret, Secret),
}

/// One place to look for a credential. `Ok(None)` = not found here, try the
/// next store; `Err` = found-but-unreadable (surface with hint).
pub trait SecretStore: Send + Sync {
    fn resolve(&self, profile: &str, auth: &AuthRef) -> Result<Option<Credential>, CoreError>;
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Token env lookups: `IGNITION_TOKEN_<PROFILE_UP>` (profile uppercased,
/// non-alphanumeric → `_`) → the profile's `token_env` var name → generic
/// `IGNITION_TOKEN`.
pub struct EnvStore;

impl SecretStore for EnvStore {
    fn resolve(&self, profile: &str, auth: &AuthRef) -> Result<Option<Credential>, CoreError> {
        let specific = format!("IGNITION_TOKEN_{}", profile_env_suffix(profile));
        if let Some(token) = env_var(&specific) {
            return Ok(Some(Credential::Token(Secret::new(token))));
        }
        if let AuthRef::TokenEnv { token_env } = auth
            && let Some(token) = env_var(token_env)
        {
            return Ok(Some(Credential::Token(Secret::new(token))));
        }
        if let Some(token) = env_var("IGNITION_TOKEN") {
            return Ok(Some(Credential::Token(Secret::new(token))));
        }
        Ok(None)
    }
}

/// Profile name → env-var-safe uppercase suffix: non-alphanumeric
/// characters become `_` (`my-rig` → `MY_RIG`).
fn profile_env_suffix(profile: &str) -> String {
    profile
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// `IGNITION_USER` + `IGNITION_PASSWORD` basic pair — LAST in the LOCKED
/// order (after keyring), which is why it is a separate store from
/// [`EnvStore`]: the chain, not the struct, encodes the order.
pub struct BasicEnvStore;

impl SecretStore for BasicEnvStore {
    fn resolve(&self, _profile: &str, _auth: &AuthRef) -> Result<Option<Credential>, CoreError> {
        match (env_var("IGNITION_USER"), env_var("IGNITION_PASSWORD")) {
            (Some(user), Some(password)) => Ok(Some(Credential::Basic(
                Secret::new(user),
                Secret::new(password),
            ))),
            _ => Ok(None),
        }
    }
}

/// OS keyring lookup: service `ignition-cli`, user `profile:<name>`.
///
/// ANY `Entry::new` failure → `tracing::warn!` + `Ok(None)` (store
/// unavailable — headless Linux without D-Bus: skip, never fatal, never
/// hang; keyring-rs fails fast at construction). A live entry that exists
/// but cannot be read surfaces as `Err` (found-but-unreadable).
pub struct KeyringStore;

/// Keyring coordinates for a profile: fixed service, `profile:<name>` user.
fn keyring_entry(profile: &str) -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new("ignition-cli", &format!("profile:{profile}"))
}

impl SecretStore for KeyringStore {
    fn resolve(&self, profile: &str, _auth: &AuthRef) -> Result<Option<Credential>, CoreError> {
        let entry = match keyring_entry(profile) {
            Ok(entry) => entry,
            Err(err) => {
                tracing::warn!(error = %err, profile, "keyring unavailable; skipping");
                return Ok(None);
            }
        };
        match entry.get_password() {
            Ok(password) => Ok(Some(Credential::Token(Secret::new(password)))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => {
                tracing::warn!(error = %err, profile, "keyring entry unreadable");
                Err(CoreError::SecretUnavailable {
                    profile: profile.to_string(),
                })
            }
        }
    }
}

impl KeyringStore {
    /// Store a token for `profile` in the OS keyring. Unlike [`SecretStore::resolve`],
    /// a SET failure is an error (writing requires a working store) — used by
    /// future `profile add --keyring` flows and the keyring smoke test.
    pub fn set(&self, profile: &str, secret: &Secret) -> Result<(), CoreError> {
        let entry = keyring_entry(profile).map_err(|_| CoreError::SecretUnavailable {
            profile: profile.to_string(),
        })?;
        entry
            .set_password(secret.expose())
            .map_err(|_| CoreError::SecretUnavailable {
                profile: profile.to_string(),
            })
    }

    /// Delete the keyring entry for `profile`; a missing entry is success
    /// (idempotent).
    pub fn delete(&self, profile: &str) -> Result<(), CoreError> {
        let entry = keyring_entry(profile).map_err(|_| CoreError::SecretUnavailable {
            profile: profile.to_string(),
        })?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CoreError::SecretUnavailable {
                profile: profile.to_string(),
            }),
        }
    }
}

/// First store to yield a credential wins; exhausted →
/// [`CoreError::SecretUnavailable`] (exit 3) whose hint names the env-var
/// path — the supported headless route.
pub fn resolve_secret(
    profile: &str,
    auth: &AuthRef,
    stores: &[Box<dyn SecretStore>],
) -> Result<Credential, CoreError> {
    for store in stores {
        match store.resolve(profile, auth)? {
            Some(credential) => return Ok(credential),
            None => continue,
        }
    }
    Err(CoreError::SecretUnavailable {
        profile: profile.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        BasicEnvStore, Credential, EnvStore, KeyringStore, Secret, SecretStore, resolve_secret,
    };
    use crate::config::AuthRef;
    use crate::config::ENV_LOCK;
    use crate::error::CoreError;

    /// Test double for chain-order tests: yields a fixed answer.
    struct FixedStore(Result<Option<Credential>, ()>);
    impl SecretStore for FixedStore {
        fn resolve(
            &self,
            _profile: &str,
            _auth: &AuthRef,
        ) -> Result<Option<Credential>, CoreError> {
            self.0.clone().map_err(|()| CoreError::SecretUnavailable {
                profile: "fixed".into(),
            })
        }
    }

    /// CORE-02 type-level redaction: Debug/Display never leak the value.
    #[test]
    fn secret_renders_redacted() {
        let secret = Secret::new("CANARY-t0k3n");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(format!("{secret}"), "***");
        assert_eq!(
            secret.expose(),
            "CANARY-t0k3n",
            "expose is the only read path"
        );
    }

    /// Order step 1: `IGNITION_TOKEN_<PROFILE_UP>` beats both the
    /// auth-ref var and the generic token.
    #[test]
    fn env_store_profile_specific_token_wins() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        // SAFETY: single-threaded under ENV_LOCK; removed before return.
        unsafe {
            std::env::set_var("IGNITION_TOKEN_DEV", "specific");
            std::env::set_var("IGNITION_TOKEN", "generic");
        }
        let auth = AuthRef::TokenEnv {
            token_env: "MY_TOKEN".into(),
        };
        let credential = EnvStore
            .resolve("dev", &auth)
            .expect("resolve")
            .expect("some");
        let Credential::Token(token) = credential else {
            panic!("expected token credential");
        };
        assert_eq!(token.expose(), "specific");
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe {
            std::env::remove_var("IGNITION_TOKEN_DEV");
            std::env::remove_var("IGNITION_TOKEN");
        }
    }

    /// Order step 2: the profile's `token_env` var beats the generic one;
    /// non-alphanumeric profile chars map to `_` in the specific var name.
    #[test]
    fn env_store_token_env_ref_and_suffix_mapping() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        // SAFETY: single-threaded under ENV_LOCK; removed before return.
        unsafe {
            std::env::set_var("MY_TOKEN", "from-ref");
            std::env::set_var("IGNITION_TOKEN", "generic");
            std::env::set_var("IGNITION_TOKEN_MY_RIG", "rig-specific");
        }

        let auth = AuthRef::TokenEnv {
            token_env: "MY_TOKEN".into(),
        };
        let credential = EnvStore
            .resolve("dev", &auth)
            .expect("resolve")
            .expect("some");
        let Credential::Token(token) = credential else {
            panic!("expected token credential");
        };
        assert_eq!(token.expose(), "from-ref", "token_env ref beats generic");

        let credential = EnvStore
            .resolve("my-rig", &auth)
            .expect("resolve")
            .expect("some");
        let Credential::Token(token) = credential else {
            panic!("expected token credential");
        };
        assert_eq!(
            token.expose(),
            "rig-specific",
            "hyphen maps to _ then uppercases"
        );

        // SAFETY: single-threaded under ENV_LOCK.
        unsafe {
            std::env::remove_var("MY_TOKEN");
            std::env::remove_var("IGNITION_TOKEN");
            std::env::remove_var("IGNITION_TOKEN_MY_RIG");
        }
    }

    /// Order step 5: the basic env pair needs BOTH vars; a lone user is not
    /// a credential.
    #[test]
    fn basic_env_store_requires_both_vars() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        // SAFETY: single-threaded under ENV_LOCK; removed before return.
        unsafe {
            std::env::set_var("IGNITION_USER", "admin");
            std::env::remove_var("IGNITION_PASSWORD");
        }
        assert!(
            BasicEnvStore
                .resolve("dev", &AuthRef::default())
                .expect("resolve")
                .is_none()
        );

        // SAFETY: single-threaded under ENV_LOCK.
        unsafe {
            std::env::set_var("IGNITION_PASSWORD", "pw");
        }
        let credential = BasicEnvStore
            .resolve("dev", &AuthRef::default())
            .expect("resolve")
            .expect("some with both vars");
        let Credential::Basic(user, password) = credential else {
            panic!("expected basic credential");
        };
        assert_eq!(user.expose(), "admin");
        assert_eq!(password.expose(), "pw");

        // SAFETY: single-threaded under ENV_LOCK.
        unsafe {
            std::env::remove_var("IGNITION_USER");
            std::env::remove_var("IGNITION_PASSWORD");
        }
    }

    /// The LOCKED chain order end-to-end (env tokens → keyring-shaped store
    /// → basic env) via test doubles, plus first-Some-wins and exhaustion.
    #[test]
    fn resolve_secret_chain_order_first_some_wins_and_exhaustion() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        // SAFETY: single-threaded under ENV_LOCK; removed before return.
        unsafe {
            std::env::set_var("IGNITION_TOKEN", "env-token");
            std::env::set_var("IGNITION_USER", "admin");
            std::env::set_var("IGNITION_PASSWORD", "pw");
        }
        let auth = AuthRef::default();

        // A store shaped like a populated keyring sits BETWEEN EnvStore and
        // BasicEnvStore in the chain; the env token must still win (env-first).
        let keyring_like = FixedStore(Ok(Some(Credential::Token(Secret::new("keyring-token")))));
        let chain: Vec<Box<dyn SecretStore>> = vec![
            Box::new(EnvStore),
            Box::new(keyring_like),
            Box::new(BasicEnvStore),
        ];
        let credential = resolve_secret("dev", &auth, &chain).expect("resolve");
        let Credential::Token(token) = credential else {
            panic!("expected token credential");
        };
        assert_eq!(token.expose(), "env-token");

        // Keyring-shaped store wins over basic env (order: keyring before USER/PASSWORD).
        let keyring_like = FixedStore(Ok(Some(Credential::Token(Secret::new("keyring-token")))));
        let chain: Vec<Box<dyn SecretStore>> = vec![
            Box::new(EnvStore),
            Box::new(keyring_like),
            Box::new(BasicEnvStore),
        ];
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_TOKEN") };
        let credential = resolve_secret("dev", &auth, &chain).expect("resolve");
        let Credential::Token(token) = credential else {
            panic!("expected token credential");
        };
        assert_eq!(token.expose(), "keyring-token", "keyring beats basic env");

        // Basic env is the last resort.
        let chain: Vec<Box<dyn SecretStore>> = vec![Box::new(EnvStore), Box::new(BasicEnvStore)];
        let credential = resolve_secret("dev", &auth, &chain).expect("resolve");
        let Credential::Basic(user, _) = credential else {
            panic!("expected basic credential");
        };
        assert_eq!(user.expose(), "admin");

        // Exhausted → SecretUnavailable (exit 3) with the env-first hint.
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe {
            std::env::remove_var("IGNITION_USER");
            std::env::remove_var("IGNITION_PASSWORD");
        }
        let err = resolve_secret("dev", &auth, &[]).expect_err("empty chain exhausts");
        assert!(matches!(err, CoreError::SecretUnavailable { .. }));
        assert_eq!(err.exit_code(), 3);
        assert!(
            err.hint().expect("hint").contains("IGNITION_TOKEN"),
            "hint names the env path: {}",
            err.hint().unwrap(),
        );
    }

    /// `KeyringStore` trait-level resolve is exercised ONLY by the
    /// `#[ignore]`-gated smoke test (Pitfall 8: unit tests never touch a
    /// real keychain). This test merely pins that the type exists at the
    /// chain type level without calling into the OS.
    #[test]
    fn keyring_store_is_constructible_without_side_effects() {
        let _store = KeyringStore;
    }
}
