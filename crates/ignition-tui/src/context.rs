//! Profile → client resolution for the TUI (Phase 6 research, Pattern 3).
//!
//! The CLI's `resolve_profile_context` / `resolve_gateway_api` are PRIVATE
//! fns in ignition-cli's main.rs — not importable here, and the choke files
//! stay untouched. This module composes the same PUBLIC
//! `ignition_core::config` building blocks in the same LOCKED order, so the
//! TUI resolves a gateway exactly like the CLI does:
//!
//! `config_path()` → `load` → `apply_env_overlay` (scoped to the
//! selection) → `resolve_selection` (flag > active) → `resolve_secret`
//! (the LOCKED chain, no degradation — the cockpit is an authed surface)
//! → `ReqwestGatewayApi::new`.
//!
//! Secrets stay confined: the [`Credential`] flows into
//! `ReqwestGatewayApi::new` and is never formatted or stored —
//! `Secret::expose` remains locked inside the client's single
//! header-construction site.

use std::sync::Arc;

use ignition_core::client::ReqwestGatewayApi;
use ignition_core::config::{self, Config, Profile, SecretStore};
use ignition_core::error::CoreError;

/// The LOCKED secret chain (env tokens → keyring → basic pair), built in
/// exactly this order — identical to ignition-cli's private `secret_chain`.
/// The chain, not the structs, encodes the order.
fn secret_chain() -> Vec<Box<dyn SecretStore>> {
    vec![
        Box::new(config::EnvStore),
        Box::new(config::KeyringStore),
        Box::new(config::BasicEnvStore),
    ]
}

/// Resolve the profile flag → `(name, Arc<ReqwestGatewayApi>)` for the
/// cockpit's opening context. `None` selection (no flag, no active
/// profile) is [`CoreError::NoActiveProfile`] — the hint names how to
/// add one; the cockpit is a gateway surface and cannot open without
/// a target.
pub fn resolve(profile_flag: Option<&str>) -> Result<(String, Arc<ReqwestGatewayApi>), CoreError> {
    let mut config = config::load(&config::config_path())?;
    let (name, profile) = resolve_from(&mut config, profile_flag)?;
    build_client(name, profile)
}

/// Rebuild a client for a NAMED profile (06-02's profile switcher):
/// reload config from disk, overlay, resolve the named profile's secret.
pub fn rebuild(profile_name: &str) -> Result<(String, Arc<ReqwestGatewayApi>), CoreError> {
    let mut config = config::load(&config::config_path())?;
    let (name, profile) = resolve_from(&mut config, Some(profile_name))?;
    build_client(name, profile)
}

/// Overlay + selection, mirroring main.rs's private
/// `resolve_profile_context` semantics exactly: the `IGNITION_URL` env
/// overlay is scoped to the would-be selection (flag > active) FIRST,
/// then the selection resolves against the overlaid config.
fn resolve_from(config: &mut Config, flag: Option<&str>) -> Result<(String, Profile), CoreError> {
    let overlay_target = flag.map(str::to_string).or_else(|| config.active.clone());
    config::apply_env_overlay(config, overlay_target.as_deref());
    match config::resolve_selection(config, flag)? {
        Some((name, profile)) => Ok((name, profile)),
        None => Err(CoreError::NoActiveProfile),
    }
}

/// REQUIRED credential (authed-read chain — a missing secret is
/// `SecretUnavailable` exit 3, never degraded) + client construction.
fn build_client(
    name: String,
    profile: Profile,
) -> Result<(String, Arc<ReqwestGatewayApi>), CoreError> {
    let credential = config::resolve_secret(&name, &profile.auth, &secret_chain())?;
    let api = ReqwestGatewayApi::new(&profile, Some(credential))?;
    Ok((name, Arc::new(api)))
}

#[cfg(test)]
mod tests {
    use super::{rebuild, resolve};
    use ignition_core::config::{self, AuthRef, Config, Profile};
    use ignition_core::error::CoreError;

    use std::path::PathBuf;

    /// Serializes env-var mutation across these tests: env is
    /// process-global and lib tests run in parallel threads (edition 2024
    /// makes `set_var` unsafe for exactly this reason — under this lock it
    /// is sound). ignition-core's `ENV_LOCK` is `#[cfg(test)] pub(crate)`
    /// and not importable from here, so the pattern is replicated locally
    /// (same soundness rationale).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_config_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        (dir, path)
    }

    /// Two profiles (dev active, prod), token auth via unique per-test env
    /// vars — `IGNITION_TOKEN_<NAME>` resolves without touching any
    /// real-world secret.
    fn sample_config() -> Config {
        let mut config = Config {
            active: Some("dev".into()),
            ..Config::default()
        };
        for (name, port) in [("dev", 9088), ("prod", 9443)] {
            config.profiles.insert(
                name.into(),
                Profile {
                    url: format!("http://localhost:{port}/").parse().expect("url"),
                    label: None,
                    ssl_verify: true,
                    auth: AuthRef::default(),
                    webdev_secret: None,
                },
            );
        }
        config
    }

    /// Point `IGNITION_CLI_CONFIG` at a fresh temp file with the sample
    /// config written, plus the per-profile token envs. Caller holds
    /// `ENV_LOCK` for the whole scoped block.
    fn isolated_env() -> (tempfile::TempDir, Vec<String>) {
        let (dir, path) = temp_config_path();
        config::save(&path, &sample_config()).expect("save sample config");
        // Set the profile-specific token envs the EnvStore head of the
        // chain finds (`IGNITION_TOKEN_<PROFILE_UP>`).
        let mut set_vars = Vec::new();
        for (name, token) in [("dev", "t-dev"), ("prod", "t-prod")] {
            let var = format!("IGNITION_TOKEN_{}", name.to_uppercase());
            unsafe { std::env::set_var(&var, token) };
            set_vars.push(var);
        }
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
        (dir, set_vars)
    }

    /// Scope-bound cleanup BEFORE the guard drops: tests that follow must
    /// not inherit our env.
    fn teardown(vars: &[String]) {
        unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };
        for var in vars {
            unsafe { std::env::remove_var(var) };
        }
    }

    /// resolve picks flag > active and constructs a working client pair.
    #[test]
    fn resolve_prefers_flag_over_active() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_env();

        let (name, _api) = resolve(Some("prod")).expect("flag profile resolves");
        assert_eq!(name, "prod", "flag must beat the active profile");

        let (name, _api) = resolve(None).expect("active profile resolves");
        assert_eq!(name, "dev", "no flag falls back to config.active");

        teardown(&vars);
    }

    /// rebuild targets a NAMED profile regardless of active.
    #[test]
    fn rebuild_resolves_named_profile() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_env();

        let (name, _api) = rebuild("prod").expect("named profile resolves");
        assert_eq!(name, "prod");

        // An unknown name is the standard ProfileNotFound refusal.
        let err = match rebuild("nope") {
            Ok(_) => panic!("unknown profile must fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CoreError::ProfileNotFound { .. }));

        teardown(&vars);
    }

    /// No flag, no active profile → clean NoActiveProfile (exit 3) error,
    /// never a panic.
    #[test]
    fn resolve_without_any_profile_fails_cleanly() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let (dir, path) = temp_config_path();
        config::save(&path, &Config::default()).expect("save empty config");
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
        // The generic IGNITION_TOKEN head could satisfy secrets — remove
        // it so the no-profile path is the ONLY thing under test.
        unsafe { std::env::remove_var("IGNITION_TOKEN") };

        let err = match resolve(None) {
            Ok(_) => panic!("empty config must fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CoreError::NoActiveProfile));
        assert_eq!(err.exit_code(), 3);

        drop(dir);
        teardown(&[]);
    }
}
