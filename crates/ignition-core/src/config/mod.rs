//! Config discovery, load/save, selection, and the env overlay (research
//! Pattern 2).
//!
//! Discovery: `IGNITION_CLI_CONFIG` (explicit path — scripts and tests)
//! FIRST, the platform path second. macOS gotcha: `directories` ignores
//! `XDG_CONFIG_HOME`, which is exactly why every test drives the env
//! override.
//!
//! Precedence (LOCKED): CLI flag > `IGNITION_*` env > profile value >
//! default. Each env concern has exactly one home: profile selection env
//! (`IGNITION_PROFILE`) is folded into `--profile` by the bin's
//! `apply_env_defaults`; the URL env overlay lives here ([`apply_env_overlay`]);
//! auth env resolution lives in [`secret`].

pub mod profile;

pub use profile::{AuthRef, Config, Profile};

use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// Serializes env-var mutation across this crate's unit tests: env is
/// process-global and lib tests run in parallel threads (edition 2024 makes
/// `set_var` unsafe for exactly this reason — under this lock it is sound).
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Config file location: `IGNITION_CLI_CONFIG` env override first, the
/// platform config dir second.
pub fn config_path() -> PathBuf {
    std::env::var_os("IGNITION_CLI_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let dirs = directories::ProjectDirs::from("", "", "ignition-cli")
                .expect("no home directory discoverable");
            dirs.config_dir().join("config.toml")
        })
}

/// Load config from `path`. A missing file is a fresh install, NOT an error
/// (`version` must work day one). Unreadable or invalid TOML is
/// [`CoreError::ConfigInvalid`] (exit 3) naming the path. Unknown keys WARN
/// (tracing) and are otherwise tolerated — no `deny_unknown_fields`, ever
/// (Pitfall 7).
pub fn load(path: &Path) -> Result<Config, CoreError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(err) => {
            return Err(CoreError::ConfigInvalid {
                reason: format!("cannot read {}: {err}", path.display()),
            });
        }
    };
    if raw.trim().is_empty() {
        return Ok(Config::default());
    }
    warn_unknown_keys(&raw);
    toml::from_str(&raw).map_err(|err| CoreError::ConfigInvalid {
        reason: format!("{}: {err}", path.display()),
    })
}

const KNOWN_TOP_LEVEL: &[&str] = &["active", "profiles"];
const KNOWN_PROFILE_KEYS: &[&str] = &["url", "label", "ssl_verify", "auth"];
const KNOWN_AUTH_KEYS: &[&str] = &["token_env", "keyring", "user_env", "password_env"];

/// Warn (never fail) about config keys a future CLI version might
/// understand. Invalid TOML is skipped here — [`load`] reports it properly.
fn warn_unknown_keys(raw: &str) {
    let Ok(table) = raw.parse::<toml::Table>() else {
        return;
    };
    for (key, value) in &table {
        if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
            tracing::warn!(key = %key, "unknown config key (ignored)");
        }
        if key != "profiles" {
            continue;
        }
        let Some(profiles) = value.as_table() else {
            continue;
        };
        for (name, profile_value) in profiles {
            let Some(profile_table) = profile_value.as_table() else {
                continue;
            };
            for (profile_key, auth_value) in profile_table {
                if !KNOWN_PROFILE_KEYS.contains(&profile_key.as_str()) {
                    tracing::warn!(profile = %name, key = %profile_key, "unknown profile key (ignored)");
                }
                if profile_key == "auth"
                    && let Some(auth_table) = auth_value.as_table()
                {
                    for auth_key in auth_table.keys() {
                        if !KNOWN_AUTH_KEYS.contains(&auth_key.as_str()) {
                            tracing::warn!(profile = %name, key = auth_key, "unknown auth key (ignored)");
                        }
                    }
                }
            }
        }
    }
}

/// Persist config to `path`, creating parent dirs. The file is written —
/// and re-asserted — with 0600 permissions on unix (Pitfall 3.6
/// prevention, verified by test).
pub fn save(path: &Path, config: &Config) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| CoreError::ConfigInvalid {
            reason: format!("cannot create {}: {err}", parent.display()),
        })?;
    }
    let contents = toml::to_string_pretty(config).map_err(|err| CoreError::ConfigInvalid {
        reason: format!("cannot serialize config: {err}"),
    })?;
    std::fs::write(path, contents).map_err(|err| CoreError::ConfigInvalid {
        reason: format!("cannot write {}: {err}", path.display()),
    })?;
    enforce_0600(path)
}

/// Re-assert 0600 even when the file already existed with looser perms
/// (`OpenOptions::mode` only applies at creation).
#[cfg(unix)]
fn enforce_0600(path: &Path) -> Result<(), CoreError> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|err| {
        CoreError::ConfigInvalid {
            reason: format!("cannot set 0600 on {}: {err}", path.display()),
        }
    })
}

#[cfg(not(unix))]
fn enforce_0600(_path: &Path) -> Result<(), CoreError> {
    Ok(())
}

/// Env overlay (the `IGNITION_*` half of the LOCKED precedence): when
/// `IGNITION_URL` is set it overrides the selected profile's URL. Profile
/// selection env (`IGNITION_PROFILE`) is folded into the `--profile` flag by
/// the bin; auth env resolution lives in [`secret`] — one concern per home.
pub fn apply_env_overlay(config: &mut Config, selected_profile: Option<&str>) {
    let Some(name) = selected_profile else { return };
    let Ok(url_string) = std::env::var("IGNITION_URL") else {
        return;
    };
    if url_string.is_empty() {
        return;
    }
    let Ok(url) = url::Url::parse(&url_string) else {
        tracing::warn!(url = %url_string, "IGNITION_URL is not a valid URL; ignoring");
        return;
    };
    if let Some(profile) = config.profiles.get_mut(name) {
        profile.url = url;
    }
}

/// Resolve which profile a command targets: flag (which already contains
/// `IGNITION_PROFILE` via the bin's env-defaults step) > `config.active`.
///
/// Unknown name → [`CoreError::ProfileNotFound`] (exit 3). Nothing found →
/// `Ok(None)` — callers decide whether `None` is an error (gateway
/// commands: yes, via [`CoreError::NoActiveProfile`]; `version` and
/// `profile list` tolerate it).
pub fn resolve_selection(
    config: &Config,
    flag: Option<&str>,
) -> Result<Option<(String, Profile)>, CoreError> {
    let name = match flag.map(str::to_owned).or_else(|| config.active.clone()) {
        Some(name) => name,
        None => return Ok(None),
    };
    match config.profiles.get(&name) {
        Some(profile) => Ok(Some((name, profile.clone()))),
        None => Err(CoreError::ProfileNotFound {
            name,
            known: config.profiles.keys().cloned().collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Profile, apply_env_overlay, config_path, load, resolve_selection, save};
    use crate::config::AuthRef;
    use crate::config::ENV_LOCK;
    use crate::error::CoreError;

    use std::path::PathBuf;

    fn temp_config_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        (dir, path)
    }

    fn sample_config() -> Config {
        let mut config = Config {
            active: Some("dev".into()),
            ..Config::default()
        };
        config.profiles.insert(
            "dev".into(),
            Profile {
                url: "http://localhost:9088/".parse().expect("url"),
                label: Some("Dev rig".into()),
                ssl_verify: true,
                auth: AuthRef::TokenEnv {
                    token_env: "IGNITION_TOKEN".into(),
                },
            },
        );
        config.profiles.insert(
            "prod".into(),
            Profile {
                url: "https://gw.example.com:8443/".parse().expect("url"),
                label: None,
                ssl_verify: true,
                auth: AuthRef::Keyring {
                    keyring: "profile:prod".into(),
                },
            },
        );
        config
    }

    /// Save → load round-trips exactly, and the on-disk TOML omits `label`
    /// when unset (`skip_serializing_if` proven at the file level).
    #[test]
    fn round_trip_save_load() {
        let (_dir, path) = temp_config_path();
        let config = sample_config();

        save(&path, &config).expect("save");
        let reloaded = load(&path).expect("load");
        assert_eq!(reloaded, config, "round trip must be lossless");

        let raw = std::fs::read_to_string(&path).expect("read raw");
        assert!(raw.contains("label = \"Dev rig\""));
        let prod_section = raw
            .split("[profiles.prod]")
            .nth(1)
            .expect("prod section serialized");
        assert!(
            !prod_section.contains("label"),
            "unset label must not be serialized: {prod_section}",
        );
    }

    /// Config is written with 0600 perms — and re-asserted on overwrite
    /// even if something loosened them (Pitfall 3.6 prevention).
    #[test]
    #[cfg(unix)]
    fn save_enforces_0600_and_creates_parents() {
        use std::os::unix::fs::PermissionsExt;

        let (_dir, path) = temp_config_path();
        let nested = path.parent().unwrap().join("nested/deeper/config.toml");

        save(&nested, &sample_config()).expect("save creates parent dirs");
        let mode = std::fs::metadata(&nested)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "fresh config must be 0600");

        // Loosen, save again → still 0600 afterwards.
        std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o644)).expect("loosen");
        save(&nested, &sample_config()).expect("save again");
        let mode = std::fs::metadata(&nested)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "overwrite must re-assert 0600");
    }

    /// Unknown TOML keys warn but do NOT fail the load (Pitfall 7).
    #[test]
    fn unknown_keys_warn_but_do_not_fail() {
        let (_dir, path) = temp_config_path();
        std::fs::write(
            &path,
            r#"
future_top_level = "whatever"
active = "dev"

[profiles.dev]
url = "http://localhost:9088/"
future_profile_key = 42

[profiles.dev.auth]
token_env = "IGNITION_TOKEN"
future_auth_key = "x"
"#,
        )
        .expect("write");

        let config = load(&path).expect("unknown keys must not fail the load");
        assert_eq!(config.active.as_deref(), Some("dev"));
        assert!(config.profiles.contains_key("dev"));
    }

    /// Missing file is a fresh install, not an error; with no flag and no
    /// active profile, selection is `Ok(None)` (version/profile-list
    /// tolerate it).
    #[test]
    fn missing_file_and_no_selection_resolve_none() {
        let (_dir, path) = temp_config_path();
        assert!(!path.exists(), "fixture sanity");

        let config = load(&path).expect("missing file is not an error");
        assert_eq!(config, Config::default());

        let selection =
            resolve_selection(&config, None).expect("no active + no flag is not an error");
        assert!(selection.is_none());
    }

    /// Unknown profile → `ProfileNotFound` (exit 3) carrying the known
    /// profile list for the hint.
    #[test]
    fn unknown_profile_lists_known() {
        let config = sample_config();
        let err = resolve_selection(&config, Some("nope")).expect_err("unknown profile errors");
        match err {
            CoreError::ProfileNotFound {
                ref name,
                ref known,
            } => {
                assert_eq!(name, "nope");
                assert_eq!(known, &vec!["dev".to_string(), "prod".to_string()]);
            }
            other => panic!("wrong error class: {other}"),
        }
        assert_eq!(err.exit_code(), 3);
        let hint = err.hint().expect("hint");
        assert!(
            hint.contains("dev") && hint.contains("prod"),
            "hint names knowns: {hint}"
        );
    }

    /// Flag > active: an explicit flag selects a non-active profile.
    #[test]
    fn selection_flag_beats_active() {
        let config = sample_config(); // active = dev
        let (name, profile) = resolve_selection(&config, Some("prod"))
            .expect("flag selects prod")
            .expect("some");
        assert_eq!(name, "prod");
        assert_eq!(
            profile.auth,
            AuthRef::Keyring {
                keyring: "profile:prod".into()
            }
        );
    }

    /// `IGNITION_URL` overrides the SELECTED profile only (env overlay
    /// scoped, per the LOCKED precedence).
    #[test]
    fn env_overlay_overrides_selected_profile_url() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        // SAFETY: single-threaded under ENV_LOCK; restored before return.
        unsafe { std::env::set_var("IGNITION_URL", "http://override.example:7000") };

        let mut config = sample_config();
        apply_env_overlay(&mut config, Some("dev"));
        assert_eq!(
            config.profiles["dev"].url.as_str(),
            "http://override.example:7000/",
            "selected profile URL overridden",
        );
        assert_eq!(
            config.profiles["prod"].url.as_str(),
            "https://gw.example.com:8443/",
            "other profiles untouched",
        );

        // No selection → no-op even with the env set.
        let mut config = sample_config();
        apply_env_overlay(&mut config, None);
        assert_eq!(
            config.profiles["dev"].url.as_str(),
            "http://localhost:9088/",
            "no selected profile → overlay is a no-op",
        );

        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_URL") };
    }

    /// `IGNITION_CLI_CONFIG` wins over the platform path; without it, the
    /// platform path ends in `config.toml`.
    #[test]
    fn config_path_env_override_first() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let override_path = dir.path().join("my-config.toml");

        // SAFETY: single-threaded under ENV_LOCK; removed before return.
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &override_path) };
        assert_eq!(config_path(), override_path, "env override wins");
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };

        assert!(
            config_path().ends_with("config.toml"),
            "platform fallback lands on config.toml: {}",
            config_path().display(),
        );
    }
}
