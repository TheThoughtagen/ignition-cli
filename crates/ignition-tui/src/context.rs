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
use std::time::Duration;

use ignition_core::client::ReqwestGatewayApi;
use ignition_core::config;
use ignition_core::error::CoreError;
use ignition_core::session::Session;

use crate::ui::theme::{Palette, Theme, Tier, detect_tier};
use crate::workers::refresh::REFRESH_PERIOD;

/// Resolve `[ui].theme` to a Palette at `tier`. Unknown names WARN and
/// fall back to the default theme — the lenient-degradation contract
/// (Phase 8: a typo in a NEW key must never fail the load), decided at
/// TUI-resolution time where `load_for_tui` already degrades new-schema
/// surface. Pure over `(name, tier)`: tests pin it at fixed tiers,
/// [`build_context`] supplies the real detected tier.
fn resolve_palette(name: Option<&str>, tier: Tier) -> Palette {
    let theme = Theme::by_name(name.unwrap_or("default")).unwrap_or_else(|| {
        tracing::warn!(
            theme = name.unwrap_or(""),
            "unknown [ui].theme — using default theme"
        );
        Theme::by_name("default").expect("default theme exists")
    });
    theme.resolve(tier)
}

/// The cockpit's opening context: everything resolution knows about the
/// selected world, carried as ONE value so callers adopt the pieces in
/// a single assignment block (the profile-switch trap's antidote —
/// `update.rs::switch_profile` must set ALL of these before re-spawning
/// the refresh worker, or the dashboard silently keeps the old
/// cadence).
pub struct ResolvedContext {
    /// The resolved profile's name (flag > active — workers' target).
    pub profile_name: String,
    /// The profile's URL string (doctor's `profile_url`).
    pub profile_url: String,
    /// The profile's configured dashboard refresh cadence —
    /// `poll_interval_secs` when set, the 5 s [`REFRESH_PERIOD`]
    /// default when absent or degraded. THE single source the refresh
    /// worker reads via `AppState.poll_interval`.
    pub poll_interval: Duration,
    /// The resolved theme palette — `[ui].theme` at the detected
    /// capability tier; adopted by run_loop and the profile switch like
    /// [`ResolvedContext::poll_interval`].
    pub palette: Palette,
    /// The authed client handle (Session-constructed — the ONLY
    /// construction site the cockpit's world uses).
    pub api: Arc<ReqwestGatewayApi>,
}

/// Resolve the profile flag → the cockpit's opening [`ResolvedContext`]
/// the cockpit's opening context (`profile_url` is the profile's URL
/// string — doctor's `profile_url`). `None` selection (no flag, no
/// active profile) is [`CoreError::NoActiveProfile`] — the hint names
/// how to add one; the cockpit is a gateway surface and cannot open
/// without a target.
pub fn resolve(profile_flag: Option<&str>) -> Result<ResolvedContext, CoreError> {
    build_context(profile_flag)
}

/// Rebuild a context for a NAMED profile (06-02's profile switcher):
/// reload config from disk, overlay, resolve the named profile's
/// secret. The returned `poll_interval` is the NAMED profile's — the
/// switcher adopts it verbatim so the cadence changes without a
/// restart.
pub fn rebuild(profile_name: &str) -> Result<ResolvedContext, CoreError> {
    build_context(Some(profile_name))
}

/// Degraded load + Session resolution + REQUIRED-credential context.
/// The load is the TUI's degradation point (`config::load_for_tui` —
/// new-surface schema failures warn and default: a `[ui]` typo or a
/// wrong-typed/clamped-violating `poll_interval_secs` can never kill
/// cockpit startup, CORE-10). Everything downstream is Session's:
/// `Session::resolve_loaded` owns overlay → selection → LOCKED chain
/// in the required mode (a missing secret is `SecretUnavailable` exit
/// 3, never degraded — the cockpit is an authed surface), and raw TOML
/// / profile-deserialize / selection failures still hard-error
/// through it. The context carries the profile's URL string (doctor's
/// `profile_url` — the cockpit runs no doctor-less world) and its poll
/// cadence: the configured `poll_interval_secs`, or the 5 s
/// [`REFRESH_PERIOD`] default when absent/degraded — ONE source for
/// the default (this import; never a second constant).
fn build_context(profile_flag: Option<&str>) -> Result<ResolvedContext, CoreError> {
    let mut config = config::load_for_tui(&config::config_path())?;
    let (session, profile) = Session::resolve_loaded(&mut config, profile_flag)?;
    let poll_interval = Duration::from_secs(
        profile
            .poll_interval_secs
            .unwrap_or(REFRESH_PERIOD.as_secs()),
    );
    // Theme tier detection is the ONLY real-environment call in the
    // module — injected as an env snapshot so the pure `resolve_palette`
    // stays fixed-tier testable.
    let tier = detect_tier(&|k| std::env::var(k).ok());
    let palette = resolve_palette(config.ui.theme.as_deref(), tier);
    Ok(ResolvedContext {
        profile_name: session.profile_name().to_string(),
        profile_url: profile.url.to_string(),
        poll_interval,
        palette,
        api: session.api_handle(),
    })
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
    use super::{detect_tier, rebuild, resolve, resolve_palette};
    use crate::ui::theme::{Theme, Tier};
    use ignition_core::config::{self, AuthRef, Config, Profile};
    use ignition_core::error::CoreError;

    use std::path::PathBuf;
    use std::time::Duration;

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

        let ctx = resolve(Some("prod")).expect("flag profile resolves");
        assert_eq!(
            ctx.profile_name, "prod",
            "flag must beat the active profile"
        );
        assert_eq!(
            ctx.profile_url, "http://localhost:9443/",
            "profile url rides along"
        );
        assert_eq!(
            ctx.poll_interval,
            Duration::from_secs(5),
            "an absent poll_interval_secs resolves to the REFRESH_PERIOD default"
        );

        let ctx = resolve(None).expect("active profile resolves");
        assert_eq!(
            ctx.profile_name, "dev",
            "no flag falls back to config.active"
        );

        teardown(&vars);
    }

    /// rebuild targets a NAMED profile regardless of active.
    #[test]
    fn rebuild_resolves_named_profile() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let (_dir, vars) = isolated_env();

        let ctx = rebuild("prod").expect("named profile resolves");
        assert_eq!(ctx.profile_name, "prod");

        // An unknown name is the standard ProfileNotFound refusal.
        let err = match rebuild("nope") {
            Ok(_) => panic!("unknown profile must fail"),
            Err(err) => err,
        };
        assert!(matches!(err, CoreError::ProfileNotFound { .. }));

        teardown(&vars);
    }

    /// THE SWITCH TRAP'S unit proof (TUIX-05): each profile's configured
    /// `poll_interval_secs` rides the resolved context PER PROFILE —
    /// `resolve` carries the selected profile's cadence and `rebuild`
    /// (the switcher's entry point) carries the NAMED one, so a switch
    /// can adopt a different interval without a restart.
    #[test]
    fn resolve_and_rebuild_carry_each_profiles_configured_interval() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[profiles.dev]
url = "http://localhost:9088/"
poll_interval_secs = 2

[profiles.prod]
url = "http://localhost:9443/"
poll_interval_secs = 5
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let ctx = resolve(None).expect("active profile resolves");
        assert_eq!(
            ctx.poll_interval,
            Duration::from_secs(2),
            "the active profile's configured cadence flows through resolve"
        );

        let ctx = rebuild("prod").expect("named profile resolves");
        assert_eq!(
            ctx.poll_interval,
            Duration::from_secs(5),
            "rebuild returns the NAMED profile's interval — not the active one's"
        );

        teardown(&["IGNITION_TOKEN".to_string()]);
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

        let ctx = resolve(None).expect("a new-surface typo must degrade, not kill startup");
        assert_eq!(ctx.profile_name, "dev");
        assert_eq!(
            ctx.profile_url, "http://localhost:9088/",
            "the degraded profile still resolves to its url"
        );
        assert_eq!(
            ctx.poll_interval,
            Duration::from_secs(5),
            "the degraded load falls back to the 5 s default cadence"
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

        let ctx = resolve(None).expect("the TUI degrades the clamp instead of refusing");
        assert_eq!(
            ctx.profile_name, "dev",
            "the cockpit starts on the default cadence"
        );
        assert_eq!(
            ctx.poll_interval,
            Duration::from_secs(5),
            "the clamp-degraded profile polls at the 5 s default"
        );

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

    // ---- Theme resolution (12-02) ----
    //
    // Two levels, both pinned: the PURE helper `resolve_palette` at
    // fixed tiers (no env dependence — fallback + by_name plumbing) and
    // the full `resolve` integration through `build_context` (config →
    // detected tier → palette), including the lenient-degradation
    // shapes the load already guarantees for `[ui]`.

    /// The all-Reset mono pin — shared by the mono-theme fixture test
    /// (tier-INDEPENDENT: the mono theme is authored all-Reset at every
    /// tier, so this holds against ANY ambient COLORTERM/TERM).
    /// Equality against the mono-AUTHORED palette (12-03) pins every
    /// slot without naming a literal — the CI tokenization gate
    /// forbids that outside theme.rs.
    fn assert_all_reset(palette: crate::ui::theme::Palette, context: &str) {
        let mono = Theme::by_name("mono")
            .unwrap_or_else(|| panic!("mono is a registered theme"))
            .mono;
        assert_eq!(
            palette, mono,
            "{context}: every slot must be Reset (the mono-authored palette)"
        );
    }

    /// A known theme name resolves to EXACTLY the theme's authored
    /// palette at the given tier — pure plumbing, no env involved.
    #[test]
    fn resolve_palette_known_name_fixed_tier() {
        for (name, tier) in [
            ("dark", Tier::C16),
            ("light", Tier::Truecolor),
            ("default", Tier::C256),
            ("mono", Tier::Mono),
        ] {
            let expected = Theme::by_name(name)
                .unwrap_or_else(|| panic!("{name} is a registered theme"))
                .resolve(tier);
            assert_eq!(
                resolve_palette(Some(name), tier),
                expected,
                "known name `{name}` must resolve to its authored palette at the tier"
            );
        }
    }

    /// An unknown name falls back to the default theme at the SAME
    /// tier — identical to the `None` path (the warn rides the
    /// fallback, it does not change the outcome).
    #[test]
    fn resolve_palette_unknown_name_falls_back_to_default() {
        let fallback = resolve_palette(None, Tier::Mono);
        assert_eq!(
            resolve_palette(Some("banana"), Tier::Mono),
            fallback,
            "an unknown name takes exactly the None path"
        );
        assert_eq!(
            fallback,
            Theme::by_name("default")
                .expect("default theme exists")
                .resolve(Tier::Mono),
            "the fallback IS the default theme at the same tier"
        );
    }

    /// `None` (no `[ui].theme`, or degraded away by lenient_ui) is the
    /// default theme — here at C256.
    #[test]
    fn resolve_palette_none_is_default() {
        assert_eq!(
            resolve_palette(None, Tier::C256),
            Theme::by_name("default")
                .expect("default theme exists")
                .resolve(Tier::C256),
        );
    }

    /// `[ui] theme = "mono"` flows config → build_context → palette.
    /// TIER-INDEPENDENT by design (12-01 authored mono all-Reset at
    /// every tier): the assertion holds under whatever COLORTERM/TERM
    /// the CI or dev shell happens to carry — no ambient-env coupling.
    #[test]
    fn resolve_carries_configured_mono_theme_tier_independently() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[ui]
theme = "mono"

[profiles.dev]
url = "http://localhost:9088/"
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let ctx = resolve(None).expect("the mono theme resolves");
        assert_all_reset(ctx.palette, "theme = mono at the detected tier");

        teardown(&["IGNITION_TOKEN".to_string()]);
    }

    /// `theme = "banana"` (unknown name): `resolve` SUCCEEDS and the
    /// palette equals the fallback computed in-process under the SAME
    /// ambient env — proving the fallback actually flowed through
    /// build_context (helper-only evidence would not cover the wiring).
    #[test]
    fn resolve_degrades_unknown_theme_to_default_at_detected_tier() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[ui]
theme = "banana"

[profiles.dev]
url = "http://localhost:9088/"
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let ctx = resolve(None).expect("an unknown theme name must warn-and-fallback, not fail");
        let expected = resolve_palette(None, detect_tier(&|k| std::env::var(k).ok()));
        assert_eq!(
            ctx.palette, expected,
            "the unknown name fell back to the default at the detected tier"
        );

        teardown(&["IGNITION_TOKEN".to_string()]);
    }

    /// `theme = 42` (wrong-TYPED): the existing `lenient_ui`
    /// deserializer degrades the whole `[ui]` table to defaults —
    /// `resolve` still succeeds and lands the default palette at the
    /// detected tier (mirrors the `poll_interval_secs = "banana"`
    /// degradation test).
    #[test]
    fn resolve_degrades_wrong_typed_theme_to_default() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_raw_config(
            &dir,
            r#"
active = "dev"

[ui]
theme = 42

[profiles.dev]
url = "http://localhost:9088/"
"#,
        );
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        let ctx =
            resolve(None).expect("a wrong-typed [ui].theme degrades via lenient_ui, not fails");
        let expected = resolve_palette(None, detect_tier(&|k| std::env::var(k).ok()));
        assert_eq!(
            ctx.palette, expected,
            "the degraded table lands the default palette at the detected tier"
        );

        teardown(&["IGNITION_TOKEN".to_string()]);
    }
}
