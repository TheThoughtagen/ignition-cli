//! `profile add/list/use` actions — pure config operations, no printing,
//! serde models out (declaration order = golden field order).

use std::path::Path;

use serde::Serialize;

use crate::config::{self, AuthRef, Config, Profile};
use crate::error::CoreError;

/// Result of `profile add` (declaration order = golden field order).
#[derive(Debug, Serialize)]
pub struct ProfileAddResult {
    /// The profile's name.
    pub name: String,
    /// Optional display label (CORE-01) — absent from JSON when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Gateway base URL (normalized).
    pub url: String,
    /// Safe credential kind string ("token_env"/"keyring"/"basic").
    pub auth_kind: &'static str,
    /// Whether this profile is the active one after the add.
    pub active: bool,
}

/// One `profile list` row — `auth_kind` is a safe kind string, NEVER a
/// secret or env value.
#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    /// Profile name.
    pub name: String,
    /// Optional display label (CORE-01) — absent from JSON when unset
    /// (mirrors [`Profile::label`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Gateway base URL (normalized).
    pub url: String,
    /// Safe credential kind string.
    pub auth_kind: &'static str,
}

/// Result of `profile list`.
#[derive(Debug, Serialize)]
pub struct ProfileListResult {
    /// The config's active profile name.
    pub active: Option<String>,
    /// Profiles in `BTreeMap` (name-sorted) order — deterministic goldens.
    pub profiles: Vec<ProfileSummary>,
}

/// Result of `profile use`.
#[derive(Debug, Serialize)]
pub struct ProfileUseResult {
    /// The newly active profile name.
    pub active: String,
}

/// Add (or overwrite) a profile and persist the config (0600).
///
/// `label` (CORE-01's display label) is stored on the profile when given;
/// `None` when `--label` was absent.
pub fn add(
    config_path: &Path,
    name: &str,
    url_str: &str,
    label: Option<&str>,
    auth: AuthRef,
    set_active: bool,
) -> Result<ProfileAddResult, CoreError> {
    let url = url::Url::parse(url_str).map_err(|err| CoreError::ConfigInvalid {
        reason: format!("invalid URL for profile {name:?}: {url_str} ({err})"),
    })?;
    let mut config = config::load(config_path)?;
    if config
        .profiles
        .insert(
            name.to_string(),
            Profile {
                url: url.clone(),
                label: label.map(str::to_string),
                ssl_verify: true,
                auth: auth.clone(),
                webdev_secret: None,
            },
        )
        .is_some()
    {
        tracing::warn!(profile = name, "overwriting existing profile");
    }
    if set_active {
        config.active = Some(name.to_string());
    }
    config::save(config_path, &config)?;

    Ok(ProfileAddResult {
        name: name.to_string(),
        label: label.map(str::to_string),
        url: url.to_string(),
        auth_kind: auth.kind(),
        active: config.active.as_deref() == Some(name),
    })
}

/// List profiles (name-sorted via `BTreeMap` order).
pub fn list(config: &Config) -> ProfileListResult {
    ProfileListResult {
        active: config.active.clone(),
        profiles: config
            .profiles
            .iter()
            .map(|(name, profile)| ProfileSummary {
                name: name.clone(),
                label: profile.label.clone(),
                url: profile.url.to_string(),
                auth_kind: profile.auth.kind(),
            })
            .collect(),
    }
}

/// Switch the active profile. A missing config file behaves like an unknown
/// profile (`load` yields the empty default, so the name simply is not in
/// the known list) — exit 3, same class.
pub fn use_profile(config_path: &Path, name: &str) -> Result<ProfileUseResult, CoreError> {
    let mut config = config::load(config_path)?;
    if !config.profiles.contains_key(name) {
        return Err(CoreError::ProfileNotFound {
            name: name.to_string(),
            known: config.profiles.keys().cloned().collect(),
        });
    }
    config.active = Some(name.to_string());
    config::save(config_path, &config)?;
    Ok(ProfileUseResult {
        active: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{add, list, use_profile};
    use crate::config::{AuthRef, load};
    use crate::error::CoreError;

    fn temp_config_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        (dir, path)
    }

    /// Add → list → use round-trip against a real (temp) config file,
    /// including the label skip and active tracking.
    #[test]
    fn add_list_use_round_trip() {
        let (_dir, path) = temp_config_path();

        let added = add(
            &path,
            "dev",
            "http://localhost:9088",
            Some("Dev rig"),
            AuthRef::TokenEnv {
                token_env: "IGNITION_TOKEN".into(),
            },
            true,
        )
        .expect("add dev");
        assert!(added.active, "--active sets it");
        assert_eq!(added.url, "http://localhost:9088/");

        add(
            &path,
            "prod",
            "https://gw.example.com:8443",
            None,
            AuthRef::Keyring {
                keyring: "profile:prod".into(),
            },
            false,
        )
        .expect("add prod");

        let listed = list(&load(&path).expect("load"));
        assert_eq!(listed.active.as_deref(), Some("dev"));
        assert_eq!(listed.profiles.len(), 2);
        assert_eq!(listed.profiles[0].name, "dev", "BTreeMap order");
        assert_eq!(listed.profiles[0].label.as_deref(), Some("Dev rig"));
        assert_eq!(listed.profiles[0].auth_kind, "token_env");
        assert_eq!(listed.profiles[1].label, None);
        assert_eq!(listed.profiles[1].auth_kind, "keyring");

        let used = use_profile(&path, "prod").expect("use prod");
        assert_eq!(used.active, "prod");
        assert_eq!(load(&path).expect("load").active.as_deref(), Some("prod"));
    }

    /// Invalid URL is a config-class error (exit 3); unknown `use` target
    /// carries the known list.
    #[test]
    fn add_rejects_invalid_url_and_use_rejects_unknown() {
        let (_dir, path) = temp_config_path();

        let err = add(&path, "dev", "not a url", None, AuthRef::default(), false)
            .expect_err("invalid URL rejected");
        assert!(matches!(err, CoreError::ConfigInvalid { .. }));
        assert_eq!(err.exit_code(), 3);

        add(
            &path,
            "dev",
            "http://localhost:9088",
            None,
            AuthRef::default(),
            false,
        )
        .expect("add dev");
        let err = use_profile(&path, "nope").expect_err("unknown profile");
        match err {
            CoreError::ProfileNotFound {
                ref name,
                ref known,
            } => {
                assert_eq!(name, "nope");
                assert_eq!(known, &vec!["dev".to_string()]);
            }
            other => panic!("wrong error: {other}"),
        }
    }
}
