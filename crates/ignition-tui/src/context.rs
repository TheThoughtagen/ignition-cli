//! Profile → client resolution for the TUI (Phase 8, CORE-09/CORE-10).
//!
//! EVERY gateway client the cockpit builds routes through
//! `ignition_core::session::Session` — the one core-owned seam for the
//! resolution choreography (env overlay scoped to the would-be
//! selection, then the selection, then the LOCKED secret chain) and
//! construction (`Session::resolve_loaded` / `Session::for_url`). The
//! TUI no longer duplicates any of it: the v1.0 copies of the secret
//! chain and the overlay→selection sequence were deleted when Session
//! absorbed them (research gotcha 9 — the "choke files stay untouched"
//! decision is superseded).
//!
//! The LOAD is the one TUI-owned step: `config::load_for_tui` degrades
//! NEW-surface schema failures (`[ui]` contents, `poll_interval_secs`
//! type/clamp) to defaults with a stderr tracing warning, so a config
//! typo can never kill cockpit startup (CORE-10). Resolution failures —
//! raw TOML, profile deserialize, selection, auth — still hard-error:
//! the caller (`lib.rs::run`) returns BEFORE `ratatui::init` with the
//! normal exit-3 envelope.
//!
//! Secrets stay confined: the [`Credential`] flows into the client and
//! is never formatted or stored — `Secret::expose` remains locked
//! inside the client's single header-construction site.

use std::sync::Arc;

use ignition_core::client::ReqwestGatewayApi;
use ignition_core::config;
use ignition_core::error::CoreError;
use ignition_core::session::Session;

/// Resolve the profile flag → `(name, url, Arc<ReqwestGatewayApi>)` for
/// the cockpit's opening context (`url` is the profile's URL string —
/// doctor's `profile_url`). `None` selection (no flag, no active
/// profile) is [`CoreError::NoActiveProfile`] — the hint names how to
/// add one; the cockpit is a gateway surface and cannot open without
/// a target.
pub fn resolve(
    profile_flag: Option<&str>,
) -> Result<(String, String, Arc<ReqwestGatewayApi>), CoreError> {
    build_client(profile_flag)
}

/// Rebuild a client for a NAMED profile (06-02's profile switcher):
/// reload config from disk, overlay, resolve the named profile's secret.
pub fn rebuild(profile_name: &str) -> Result<(String, String, Arc<ReqwestGatewayApi>), CoreError> {
    build_client(Some(profile_name))
}

/// Degraded load + Session resolution + REQUIRED-credential client.
/// The load is the TUI's degradation point (`config::load_for_tui` —
/// new-surface schema failures warn and default: a `[ui]` typo or a
/// wrong-typed/clamped-violating `poll_interval_secs` can never kill
/// cockpit startup, CORE-10). Everything downstream is Session's:
/// `Session::resolve_loaded` owns overlay → selection → LOCKED chain
/// in the required mode (a missing secret is `SecretUnavailable` exit
/// 3, never degraded — the cockpit is an authed surface), and raw TOML
/// / profile-deserialize / selection failures still hard-error
/// through it. Returns the profile's URL string alongside the
/// client (doctor's `profile_url` — the cockpit runs no doctor-less
/// world). The triple stays until 08-05 grows it into the resolved
/// context struct.
fn build_client(
    profile_flag: Option<&str>,
) -> Result<(String, String, Arc<ReqwestGatewayApi>), CoreError> {
    let mut config = config::load_for_tui(&config::config_path())?;
    let (session, profile) = Session::resolve_loaded(&mut config, profile_flag)?;
    let url = profile.url.to_string();
    Ok((
        session.profile_name().to_string(),
        url,
        session.api_handle(),
    ))
}

// ---- The rig family's client construction (06-06) ----
//
// The rig verbs address the RIG'S OWN derived gateway URL (never the
// profile's gateway — 04-03's lock): the URL DERIVATION stays at the
// call sites, and construction routes through `Session::for_url` —
// the core-owned headerless-BY-CONSTRUCTION constructor (the TUI twin
// of main.rs's rig clients). These helpers are ALSO the confinement
// home for every `Credential`/`Secret` construction outside the client
// itself — the rig workers pass raw env-sourced strings in and typed
// pairs out, so the phase's secrets-confinement grep keeps its
// single-file answer.

/// A HEADER-LESS client pointed at the rig's own gateway URL — the
/// commissioned-wait probe (`rig up`/`reset`) and `trial status`
/// target (those endpoints answer unauthenticated; fresh-rig
/// friendly). `ssl_verify=false`: localhost probes against
/// self-signed rig https are the norm. `None` is impossible for a
/// parseable URL — the caller already derived it.
pub fn rig_client(url: &str) -> Option<Arc<ReqwestGatewayApi>> {
    rig_client_with(url, None)
}

/// The token-bearing twin: the tier-0 `IGNITION_TOKEN` credential
/// rides the header (`snapshot`/`restore`'s only rung; `trial
/// reset`'s first).
pub fn rig_client_token(url: &str, token: &str) -> Option<Arc<ReqwestGatewayApi>> {
    rig_client_with(
        url,
        Some(config::Credential::Token(config::Secret::new(
            token.to_string(),
        ))),
    )
}

/// The shared constructor behind both rig clients: `Session::for_url`
/// with the rig's `ssl_verify=false` probe posture. `None` when the
/// URL doesn't parse or the client can't build (the callers' `?`
/// hatch).
fn rig_client_with(
    url: &str,
    credential: Option<config::Credential>,
) -> Option<Arc<ReqwestGatewayApi>> {
    let rig_url = url.parse().ok()?;
    let session = Session::for_url(rig_url, credential, false).ok()?;
    Some(session.api_handle())
}

/// A non-empty env var, when set (main.rs's private twin — the rig
/// family reads env inside its workers).
fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The trial-reset ladder's typed pair (the basic rung's user +
/// secret — constructed ONLY here, the confinement home).
pub type TrialBasic = (String, config::Secret);

/// The trial-reset ladder's credential parts, env-only (the cockpit
/// has no `--user` flag form — the `?` hatch names the env vars):
/// tier 0 = `IGNITION_TOKEN`; tier 1 = `IGNITION_USER` +
/// `IGNITION_PASSWORD` (password env-only, NEVER a flag — the CLI's
/// redaction discipline). BOTH rungs absent is the family's exit-3
/// refusal (the CLI dispatch's both-absent shape).
pub fn rig_trial_ladder() -> Result<(Option<String>, Option<TrialBasic>), CoreError> {
    let token = env_non_empty("IGNITION_TOKEN");
    let basic = env_non_empty("IGNITION_USER").zip(env_non_empty("IGNITION_PASSWORD"));
    if token.is_none() && basic.is_none() {
        return Err(CoreError::SecretUnavailable {
            profile: "rig".to_string(),
        });
    }
    let basic = basic.map(|(user, password)| (user, config::Secret::new(password)));
    Ok((token, basic))
}

/// The token-ONLY rung (`snapshot`/`restore` — the backup endpoints
/// require a token, live-verified 04-04): `IGNITION_TOKEN` or the
/// exit-3 refusal.
pub fn rig_token_only() -> Result<String, CoreError> {
    env_non_empty("IGNITION_TOKEN").ok_or(CoreError::SecretUnavailable {
        profile: "rig".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{rebuild, resolve};
    use ignition_core::config::{self, AuthRef, Config, Profile};
    use ignition_core::error::CoreError;

    use std::path::PathBuf;

    /// The crate-wide env lock (lib.rs) — serializes EVERY env-mutating
    /// ignition-tui test against the others (per-module locks do not).
    use crate::ENV_LOCK;

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
                    poll_interval_secs: None,
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

    /// RAW-text fixture variant: the load-boundary tests exercise the
    /// parser/validator, not the serializer, so the TOML is written
    /// verbatim (the `save()` round-trip would launder the very typos
    /// under test).
    fn isolate_raw_config(dir: &tempfile::TempDir, toml: &str) {
        let path = dir.path().join("config.toml");
        std::fs::write(&path, toml).expect("write config fixture");
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
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

        let (name, url, _api) = resolve(Some("prod")).expect("flag profile resolves");
        assert_eq!(name, "prod", "flag must beat the active profile");
        assert_eq!(url, "http://localhost:9443/", "profile url rides along");

        let (name, _url, _api) = resolve(None).expect("active profile resolves");
        assert_eq!(name, "dev", "no flag falls back to config.active");

        teardown(&vars);
    }

    /// rebuild targets a NAMED profile regardless of active.
    #[test]
    fn rebuild_resolves_named_profile() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_env();

        let (name, _url, _api) = rebuild("prod").expect("named profile resolves");
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

    // ---- The degradation boundary (CORE-10, TUI half) ----
    //
    // The TUI loads with `config::load_for_tui`: NEW-surface schema
    // failures degrade with a stderr warning and the cockpit STARTS;
    // resolution failures stay fatal pre-init (exit 3). These four pin
    // both sides of that line at the `resolve` level — the seam the
    // cockpit's startup actually goes through. Raw-text fixtures (the
    // `save()` round-trip would launder the typos under test); the
    // generic `IGNITION_TOKEN` head satisfies the required-credential
    // chain so the LOAD verdict is the only variable.

    /// A wrong-TYPED new-surface value (`poll_interval_secs = "banana"`)
    /// degrades to the default and the cockpit STARTS — `resolve`
    /// succeeds where a strict schema surface would have been a
    /// type-error refusal. (The interval consumer lands in 08-05; the
    /// load-level outcome is what this pins today.)
    #[test]
    fn resolve_degrades_wrong_typed_poll_interval() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = "banana"
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let (name, url, _api) =
            resolve(None).expect("a new-surface typo must degrade, not kill startup");
        assert_eq!(name, "dev");
        assert_eq!(
            url, "http://localhost:9088/",
            "the degraded profile still resolves to its url"
        );

        teardown(&["IGNITION_TOKEN".to_string()]);
    }

    /// `poll_interval_secs = 0` (the clamp violation) degrades to the
    /// default cadence on the TUI path — `resolve` succeeds — while the
    /// SAME file is refused by the strict CLI load (`PollIntervalTooSmall`,
    /// exit 3). This is the strict/degrading discriminator: one file,
    /// two load policies, both correct.
    #[test]
    fn resolve_degrades_clamp_violation_the_strict_load_refuses() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = 0
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let (name, _url, _api) =
            resolve(None).expect("the TUI degrades the clamp instead of refusing");
        assert_eq!(name, "dev", "the cockpit starts on the default cadence");

        let path = dir.path().join("config.toml");
        let strict = ignition_core::config::load(&path)
            .expect_err("the strict CLI load refuses the same file");
        assert_eq!(strict.exit_code(), 3);

        teardown(&["IGNITION_TOKEN".to_string()]);
    }

    /// A broken profile URL is a RESOLUTION failure, not a schema
    /// failure — still fatal (exit 3) even on the degrading load path.
    /// A config that cannot name a reachable profile cannot start the
    /// authed cockpit (the LOCKED NoActiveProfile family of refusals).
    #[test]
    fn resolve_still_refuses_broken_profile_url() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[profiles.dev]
url = "not a url at all"
poll_interval_secs = 5
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let err = match resolve(None) {
            Ok(_) => panic!("a broken profile url must stay fatal"),
            Err(err) => err,
        };
        assert!(matches!(err, CoreError::ConfigInvalid { .. }));
        assert_eq!(err.exit_code(), 3);

        teardown(&["IGNITION_TOKEN".to_string()]);
    }

    /// Garbage TOML (raw parse failure) is fatal pre-init — the cockpit
    /// never opens over an unreadable config, degradation notwithstanding.
    #[test]
    fn resolve_still_refuses_unparseable_toml() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(&dir, ":::: this is not toml ::::");
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let err = match resolve(None) {
            Ok(_) => panic!("garbage toml must stay fatal"),
            Err(err) => err,
        };
        assert!(matches!(err, CoreError::ConfigInvalid { .. }));
        assert_eq!(err.exit_code(), 3);

        teardown(&["IGNITION_TOKEN".to_string()]);
    }
}
