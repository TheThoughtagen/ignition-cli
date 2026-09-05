//! The execution session — the ONE seam every auth/gateway-client
//! resolution flows through (CORE-09).
//!
//! Naming: THIS is an "execution session" — a resolved profile name plus
//! the gateway client it builds. It is NOT a gateway session; the
//! unrelated `client::sessions` family models the GATEWAY's
//! Designer/Perspective/Vision sessions. One word, two domains — the
//! doc-comments everywhere else say "gateway session" when they mean
//! that other thing.
//!
//! Choreography (LOCKED — copied verbatim from the duplicated resolution
//! sites this seam replaces: main.rs's `resolve_profile_context` +
//! `resolve_gateway_api` + `resolve_headerless_api` + `rig_gateway_client`,
//! and the TUI's `context::resolve_from` + `rig_client_with`):
//!
//! 1. The `IGNITION_URL` env overlay is applied FIRST, scoped to the
//!    WOULD-BE selection (flag > active) — `config::apply_env_overlay`.
//! 2. THEN the selection resolves — `config::resolve_selection`
//!    (flag > active; unknown name → `ProfileNotFound` with the known
//!    profiles in the hint; nothing resolvable → `NoActiveProfile`,
//!    exactly what main.rs's `Ok(None)` consumers do today).
//! 3. THEN the LOCKED secret chain (env tokens → keyring → basic pair)
//!    — `config::resolve_secret` over the one chain built below.
//!
//! The client is built from the POST-OVERLAY profile — the research-
//! locked precedence (flag > `IGNITION_URL` env > profile value) must
//! hold at the construction site, not just in the config unit tests
//! (main.rs:431's contract, preserved here verbatim).
//!
//! Three credential modes, one per real call-site family:
//!
//! - [`Session::resolve`] — REQUIRED credential (the authed reads: a
//!   missing secret is `SecretUnavailable`, exit 3 — never degraded).
//! - [`Session::resolve_degraded`] — the credential DEGRADES to `None`
//!   when the chain exhausts (version / waits / doctor: these must run
//!   without a secret; every other credential error still propagates).
//! - [`Session::for_url`] — headerless-BY-CONSTRUCTION rig clients: a
//!   caller-derived URL + explicit credential, no config read at all.
//!
//! Concrete-with-deref (PLANNER DECISION, research open question 4):
//! the handle is `Arc<ReqwestGatewayApi>`, not `Arc<dyn GatewayApi>` —
//! the TUI's workers/`ClientHandle` are concretely typed and Phase 8's
//! goal is construction-site unification, not handle-type churn;
//! dyn-widening rides Phase 14 where MCP actually needs dyn. [`Deref`]
//! feeds every existing free-fn action over `&GatewayApi` unchanged —
//! the `version()` dyn precedent (`actions/version.rs`) and the
//! `rig_stream.rs` cast precedent both keep working.
//!
//! Redaction boundary UNCHANGED: [`Secret::expose`] stays confined to
//! the client's `apply_auth` header site (CORE-02's grep-auditable
//! rule). This module composes existing public config fns and
//! introduces NO new exposure path.
//!
//! `IGNITION_PROFILE` is deliberately NOT read here: the bin folds it
//! into `--profile` in exactly one place (`apply_env_defaults`), so the
//! flag this seam receives is already the effective selection.

use std::ops::Deref;
use std::sync::Arc;

use crate::client::ReqwestGatewayApi;
use crate::config::{self, AuthRef, Config, Credential, Profile, SecretStore};
use crate::error::CoreError;

/// One resolved execution context: a named profile and the gateway
/// client built from its POST-OVERLAY state. Construct through the three
/// constructors — never by struct literal (the fields are the resolved
/// choreography's output, not inputs).
///
/// Manual `Debug` (no derive): the client isn't `Debug` and must never
/// be rendered credential-side — the profile NAME is the only safe
/// field (the redaction discipline, CORE-02).
pub struct Session {
    profile: String,
    url: url::Url,
    credential_present: bool,
    api: Arc<ReqwestGatewayApi>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("profile", &self.profile)
            .field("api", &"ReqwestGatewayApi")
            .finish()
    }
}

impl Session {
    /// Resolve through config: REQUIRED credential mode. Loads config
    /// (`IGNITION_CLI_CONFIG` first, platform path second), applies the
    /// env overlay scoped to the would-be selection, resolves the
    /// selection, then walks the LOCKED secret chain with NO
    /// degradation — a missing secret is `CoreError::SecretUnavailable`
    /// (exit 3), correct for authed reads. Replaces main.rs's
    /// `resolve_gateway_api` (and `named_profile_client`: per-side
    /// resolution is just `resolve(Some(name))`).
    pub fn resolve(profile_flag: Option<&str>) -> Result<Self, CoreError> {
        let mut config = config::load(&config::config_path())?;
        Self::resolve_loaded(&mut config, profile_flag).map(|(session, _)| session)
    }

    /// Resolve through a config the CALLER already loaded — the caller
    /// owns the load policy, the seam keeps everything downstream
    /// (overlay scoped to the selection → selection → LOCKED secret
    /// chain → REQUIRED-credential construction). This is the cockpit's
    /// entry point: the TUI loads with `config::load_for_tui`, whose
    /// NEW-surface degradation contract (a schema typo warns and
    /// defaults instead of killing startup) must apply BEFORE the seam
    /// runs, while selection/auth failures stay fatal. Returns the
    /// session AND the selected POST-OVERLAY profile — the url string
    /// and cadence fields consumers like the cockpit display, which a
    /// `Session` deliberately doesn't re-expose.
    pub fn resolve_loaded(
        config: &mut Config,
        profile_flag: Option<&str>,
    ) -> Result<(Self, Profile), CoreError> {
        let (name, profile) = resolve_selected(config, profile_flag)?;
        let credential = config::resolve_secret(&name, &profile.auth, &locked_secret_chain())?;
        let api = ReqwestGatewayApi::new(&profile, Some(credential))?;
        Ok((
            Self {
                profile: name,
                url: profile.url.clone(),
                credential_present: true,
                api: Arc::new(api),
            },
            profile,
        ))
    }

    /// Resolve ONE NAMED side of a multi-profile command through a
    /// config the CALLER already loaded — the project diff/sync shape.
    /// Verbatim port of main.rs's `named_profile_client`: the selection
    /// runs against the caller's config with NO env overlay applied
    /// (side B carries its own URL even while `IGNITION_URL` overlays
    /// the envelope's active profile — the contract the diff/sync
    /// goldens pin), then the LOCKED chain (required) and the client
    /// construction. The caller's `name` rides the secret resolution
    /// verbatim, and the impossible empty-selection arm stays
    /// `CoreError::Internal` exactly as the original wrote it.
    pub fn resolve_side(config: &mut Config, name: &str) -> Result<Self, CoreError> {
        let Some((_resolved, profile)) = config::resolve_selection(config, Some(name))? else {
            return Err(CoreError::Internal(
                "a named profile selection resolved to nothing".to_string(),
            ));
        };
        let credential = config::resolve_secret(name, &profile.auth, &locked_secret_chain())?;
        let api = ReqwestGatewayApi::new(&profile, Some(credential))?;
        Ok(Self {
            profile: name.to_string(),
            url: profile.url.clone(),
            credential_present: true,
            api: Arc::new(api),
        })
    }

    /// Resolve through config with the credential DEGRADED to `None`
    /// when the secret chain exhausts: `version` must not demand a
    /// secret (gateway-info answers), the wait commands must keep
    /// polling while auth is broken, the doctor diagnoses absent auth
    /// for a living. Every OTHER credential error propagates (the
    /// `resolve_secret_opt` behavior behind main.rs:431/760 and
    /// `resolve_headerless_api`). Selection errors are NOT degraded:
    /// no target is `NoActiveProfile`, as today.
    pub fn resolve_degraded(profile_flag: Option<&str>) -> Result<Self, CoreError> {
        let mut config = config::load(&config::config_path())?;
        let (name, profile) = resolve_selected(&mut config, profile_flag)?;
        let credential = resolve_secret_opt(&name, &profile.auth)?;
        let credential_present = credential.is_some();
        let api = ReqwestGatewayApi::new(&profile, credential)?;
        Ok(Self {
            profile: name,
            url: profile.url.clone(),
            credential_present,
            api: Arc::new(api),
        })
    }

    /// Headerless-BY-CONSTRUCTION client for the rig family: a caller-
    /// derived gateway URL (never a profile's), an explicit optional
    /// credential, and the caller's `ssl_verify` (rig probes use
    /// `false` — localhost probes against self-signed rig https are the
    /// norm). No config is read and no secret is resolved. Replaces
    /// main.rs's `rig_gateway_client` + the TUI's `rig_client_with`
    /// (the URL DERIVATION stays at the call sites; only the
    /// `ReqwestGatewayApi` construction moves behind this constructor —
    /// no second client construction anywhere).
    ///
    /// Rig sessions carry NO profile: [`Self::profile_name`] returns
    /// the empty string (callers translate to `None` where the output
    /// model wants a profile echo).
    pub fn for_url(
        url: url::Url,
        credential: Option<Credential>,
        ssl_verify: bool,
    ) -> Result<Self, CoreError> {
        let profile = Profile {
            url,
            label: None,
            ssl_verify,
            auth: AuthRef::default(),
            webdev_secret: None,
            poll_interval_secs: None,
        };
        let credential_present = credential.is_some();
        let api = ReqwestGatewayApi::new(&profile, credential)?;
        Ok(Self {
            profile: String::new(),
            url: profile.url.clone(),
            credential_present,
            api: Arc::new(api),
        })
    }

    /// The resolved profile's name — empty for [`Self::for_url`] rig
    /// sessions (no profile exists).
    pub fn profile_name(&self) -> &str {
        &self.profile
    }

    /// The resolved profile's configured gateway URL — the POST-OVERLAY
    /// value (flag > `IGNITION_URL` env > profile), i.e. exactly what the
    /// session's client targets. Doctor's url check re-parses this raw
    /// value: the honest diagnosis must describe the URL the client
    /// ACTUALLY connects to, overlay included. For [`Self::for_url`] rig
    /// sessions this is the caller-derived rig URL.
    pub fn profile_url(&self) -> &url::Url {
        &self.url
    }

    /// Whether a credential resolved into this session's client — the
    /// doctor's `credential_present` flag (a MISSING credential is a
    /// different diagnosis than an UNRECOGNIZED one; the degraded chain
    /// decides, this accessor reports, and the secret itself never
    /// crosses the seam). Always `true` for [`Self::resolve`]; the
    /// caller's `Some`-ness for [`Self::for_url`].
    pub fn credential_present(&self) -> bool {
        self.credential_present
    }

    /// The gateway client, borrowed.
    pub fn api(&self) -> &ReqwestGatewayApi {
        &self.api
    }

    /// The gateway client as an owned `Arc` handle — the TUI's
    /// workers/`ClientHandle` shape, without a second construction.
    pub fn api_handle(&self) -> Arc<ReqwestGatewayApi> {
        Arc::clone(&self.api)
    }
}

/// Feeds every existing free-fn action over `&GatewayApi` unchanged:
/// `version(&*session, …)` and friends work exactly as they did against
/// a bare `ReqwestGatewayApi`.
impl Deref for Session {
    type Target = ReqwestGatewayApi;

    fn deref(&self) -> &Self::Target {
        &self.api
    }
}

/// THE LOCKED secret chain (env tokens → keyring → basic pair), built
/// in exactly one place — the chain, not the structs, encodes the
/// order (main.rs's private `secret_chain`, now shared).
fn locked_secret_chain() -> Vec<Box<dyn SecretStore>> {
    vec![
        Box::new(config::EnvStore),
        Box::new(config::KeyringStore),
        Box::new(config::BasicEnvStore),
    ]
}

/// Overlay scoped to the WOULD-BE selection FIRST, then the selection —
/// the verbatim `resolve_profile_context` / TUI `resolve_from`
/// choreography: the `IGNITION_URL` env overlay targets the profile the
/// command is ABOUT to select (flag > active), and only then does the
/// selection resolve against the overlaid config. Nothing resolvable →
/// `NoActiveProfile` (the main.rs `Ok(None)` consumer behavior — not a
/// new error class).
fn resolve_selected(
    config: &mut config::Config,
    flag: Option<&str>,
) -> Result<(String, Profile), CoreError> {
    let overlay_target = flag.map(str::to_string).or_else(|| config.active.clone());
    config::apply_env_overlay(config, overlay_target.as_deref());
    match config::resolve_selection(config, flag)? {
        Some((name, profile)) => Ok((name, profile)),
        None => Err(CoreError::NoActiveProfile),
    }
}

/// Credential resolution degraded for non-authenticating consumers: the
/// LOCKED chain with `SecretUnavailable` mapped to `Ok(None)` — every
/// other credential error propagates (the main.rs `resolve_secret_opt`
/// port, verbatim).
fn resolve_secret_opt(profile: &str, auth: &AuthRef) -> Result<Option<Credential>, CoreError> {
    config::resolve_secret(profile, auth, &locked_secret_chain())
        .map(Some)
        .or_else(|err| match err {
            CoreError::SecretUnavailable { .. } => Ok(None),
            other => Err(other),
        })
}

#[cfg(test)]
mod tests {
    use super::{Session, resolve_selected};
    use crate::client::GatewayApi;
    use crate::config::ENV_LOCK;
    use crate::error::CoreError;

    /// The gateway-info JSON body — the one field every 8.3 gateway
    /// answers with (`ignitionVersion`, the `version` alias tolerated).
    fn info_body() -> serde_json::Value {
        serde_json::json!({ "ignitionVersion": "8.3.6 (b2026042713)" })
    }

    /// Lowercased Debug dump of a recorded request's headers — the
    /// status_contract.rs presence/absence assertion pattern.
    fn headers_debug(request: &wiremock::Request) -> String {
        format!("{:?}", request.headers).to_lowercase()
    }

    /// Write a config.toml into a tempdir and point
    /// `IGNITION_CLI_CONFIG` at it. Caller holds `ENV_LOCK`.
    fn isolate_config(dir: &tempfile::TempDir, toml: &str) {
        let path = dir.path().join("config.toml");
        std::fs::write(&path, toml).expect("write config fixture");
        // SAFETY: single-threaded under ENV_LOCK; each test unsets the
        // var before returning.
        unsafe { std::env::set_var("IGNITION_CLI_CONFIG", &path) };
    }

    fn unset(name: &str) {
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var(name) };
    }

    /// Two-profile fixture: `a` active at `url_a`, `b` at `url_b`, both
    /// on the generic `IGNITION_TOKEN` auth reference.
    fn two_profile_toml(url_a: &str, url_b: &str) -> String {
        format!(
            r#"
active = "a"

[profiles.a]
url = "{url_a}"

[profiles.b]
url = "{url_b}"
"#
        )
    }

    /// A scoped gateway-info mock — the guard records the requests that
    /// actually arrived (the status_contract.rs pattern).
    async fn mount_info(server: &wiremock::MockServer, expected: u64) -> wiremock::MockGuard {
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/data/api/v1/gateway-info"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(info_body()))
            .expect(expected)
            .mount_as_scoped(server)
            .await
    }

    /// PRECEDENCE: `IGNITION_URL` overrides the SELECTED profile only —
    /// select profile B by flag while the env carries the overlay URL;
    /// the session's client must target the OVERLAY (mock_a) for the
    /// selected profile, and a second session without the overlay env
    /// targets B's OWN URL (mock_b).
    ///
    /// Env mutation stays inside the `ENV_LOCK` scope; the `await`ed
    /// requests run AFTER the guard drops — the client snapshots URL +
    /// credential at construction, so the env vars are already gone.
    #[tokio::test]
    async fn env_overlay_targets_the_selected_profile() {
        let mock_a = wiremock::MockServer::start().await;
        let mock_b = wiremock::MockServer::start().await;
        let guard_a = mount_info(&mock_a, 1).await;
        let guard_b = mount_info(&mock_b, 1).await;
        let dir = tempfile::tempdir().expect("tempdir");

        // Phase 1: overlay set → the session targets the overlay URL.
        let session = {
            let _lock = ENV_LOCK.lock().expect("env lock");
            isolate_config(
                &dir,
                &two_profile_toml(mock_b.uri().as_str(), mock_b.uri().as_str()),
            );
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::set_var("IGNITION_URL", mock_a.uri()) };
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::set_var("IGNITION_TOKEN", "overlay-token") };
            let session = Session::resolve(Some("b")).expect("resolve selects b");
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::remove_var("IGNITION_URL") };
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::remove_var("IGNITION_TOKEN") };
            session
        };
        assert_eq!(session.profile_name(), "b");
        // The overlay URL (mock_a) answered for the selected profile —
        // NOT the profile's own URL (also mock_b here).
        let info = session.gateway_info().await.expect("overlay url answers");
        assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");
        assert_eq!(
            guard_a.received_requests().await.len(),
            1,
            "the env overlay URL took the request"
        );
        assert_eq!(
            guard_b.received_requests().await.len(),
            0,
            "the profile's own URL stayed untouched while the overlay was set"
        );

        // Phase 2: overlay gone → the session targets B's OWN URL.
        // (The required-mode credential is re-supplied per phase — the
        // env vars were already cleaned before phase 1's await.)
        let session = {
            let _lock = ENV_LOCK.lock().expect("env lock");
            isolate_config(
                &dir,
                &two_profile_toml(mock_b.uri().as_str(), mock_b.uri().as_str()),
            );
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::set_var("IGNITION_TOKEN", "overlay-token") };
            let session = Session::resolve(Some("b")).expect("resolve again");
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::remove_var("IGNITION_TOKEN") };
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };
            session
        };
        let info = session.gateway_info().await.expect("own url answers");
        assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");
        assert_eq!(
            guard_b.received_requests().await.len(),
            1,
            "without the overlay the selected profile's own URL answers"
        );
    }

    /// SELECTION: flag > active; unknown name → `ProfileNotFound` with
    /// the known profiles in the hint; nothing resolvable →
    /// `NoActiveProfile` (the existing main.rs consumer of
    /// `resolve_selection`'s `Ok(None)` — not a new error class).
    /// `IGNITION_PROFILE` is deliberately inert here: the bin folds it
    /// into the flag (`apply_env_defaults` — the one env→flag home), so
    /// the seam never re-reads it.
    #[test]
    fn selection_precedence_flag_over_active_and_errors() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_config(
            &dir,
            &two_profile_toml("http://a.example:9088/", "http://b.example:9088/"),
        );
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::set_var("IGNITION_TOKEN", "t") };

        // Flag beats active.
        let session = Session::resolve(Some("b")).expect("flag selects b");
        assert_eq!(session.profile_name(), "b");

        // Unknown name → ProfileNotFound carrying the knowns.
        let err = Session::resolve(Some("nope")).expect_err("unknown errors");
        match &err {
            CoreError::ProfileNotFound { name, known } => {
                assert_eq!(name, "nope");
                assert_eq!(known, &vec!["a".to_string(), "b".to_string()]);
            }
            other => panic!("wrong error class: {other}"),
        }
        assert_eq!(err.exit_code(), 3);

        // Nothing resolvable → NoActiveProfile, both modes.
        isolate_config(&dir, "");
        assert!(matches!(
            Session::resolve(None).expect_err("no selection errors"),
            CoreError::NoActiveProfile
        ));
        assert!(matches!(
            Session::resolve_degraded(None).expect_err("no selection errors"),
            CoreError::NoActiveProfile
        ));

        // The bin's env fold is upstream: IGNITION_PROFILE alone must
        // NOT drive the seam's selection.
        isolate_config(
            &dir,
            &two_profile_toml("http://a.example:9088/", "http://b.example:9088/"),
        );
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::set_var("IGNITION_PROFILE", "b") };
        let session = Session::resolve(None).expect("active wins without a flag");
        assert_eq!(
            session.profile_name(),
            "a",
            "IGNITION_PROFILE folding belongs to the bin, not the seam"
        );

        unset("IGNITION_PROFILE");
        unset("IGNITION_TOKEN");
        unset("IGNITION_CLI_CONFIG");
    }

    /// SECRET CHAIN (LOCKED): an env token beats the basic env pair
    /// (env-first), and — when NO secret exists anywhere — required
    /// mode errors with `SecretUnavailable` (exit 3) while degraded
    /// mode returns a header-less client (the authed gateway-info
    /// request carries NO auth header at all).
    ///
    /// Same lock discipline as the precedence test: env mutations and
    /// the sync `resolve*` calls hold `ENV_LOCK`; the `await`ed
    /// requests run with the guard dropped (the client already
    /// snapshotted its credential).
    #[tokio::test]
    async fn locked_chain_env_first_required_errors_degraded_headerless() {
        let mock = wiremock::MockServer::start().await;
        let guard = mount_info(&mock, 2).await;
        let dir = tempfile::tempdir().expect("tempdir");

        // Env token beats the basic pair: both sets, the authed request
        // carries the TOKEN header and no Authorization header.
        let session = {
            let _lock = ENV_LOCK.lock().expect("env lock");
            isolate_config(
                &dir,
                &two_profile_toml(mock.uri().as_str(), mock.uri().as_str()),
            );
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe {
                std::env::set_var("IGNITION_TOKEN", "chain-token");
                std::env::set_var("IGNITION_USER", "admin");
                std::env::set_var("IGNITION_PASSWORD", "pw");
            }
            let session = Session::resolve(Some("a")).expect("env token resolves");
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe {
                std::env::remove_var("IGNITION_TOKEN");
                std::env::remove_var("IGNITION_USER");
                std::env::remove_var("IGNITION_PASSWORD");
            }
            session
        };
        let info = session.gateway_info().await.expect("token answers");
        assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");
        let requests = guard.received_requests().await;
        assert_eq!(requests.len(), 1, "one token-carrying request so far");
        let headers = headers_debug(&requests[0]);
        assert!(
            headers.contains("x-ignition-api-token"),
            "env token must ride the token header: {headers}"
        );
        assert!(
            !headers.contains("authorization"),
            "basic pair must lose to the env token: {headers}"
        );

        // No secret anywhere: required mode refuses; degraded mode
        // survives (the authed request goes out with ZERO auth headers
        // — a credential must not sneak in from the OS keyring either).
        {
            let _lock = ENV_LOCK.lock().expect("env lock");
            isolate_config(
                &dir,
                &two_profile_toml(mock.uri().as_str(), mock.uri().as_str()),
            );
            let err = Session::resolve(Some("a")).expect_err("required mode demands a secret");
            assert!(matches!(err, CoreError::SecretUnavailable { .. }));
            assert_eq!(err.exit_code(), 3);
            assert!(
                err.hint().expect("hint").contains("IGNITION_TOKEN"),
                "hint names the env path"
            );
            let degraded = Session::resolve_degraded(Some("a")).expect("degraded tolerates none");
            // SAFETY: single-threaded under ENV_LOCK.
            unsafe { std::env::remove_var("IGNITION_CLI_CONFIG") };
            degraded
        }
        .gateway_info()
        .await
        .map(|info| {
            assert_eq!(info.ignition_version, "8.3.6 (b2026042713)");
        })
        .expect("headerless answers");
        let requests = guard.received_requests().await;
        assert_eq!(requests.len(), 2, "the headerless request arrived");
        let headers = headers_debug(&requests[1]);
        assert!(
            !headers.contains("x-ignition-api-token"),
            "degraded mode must be header-less: {headers}"
        );
        assert!(
            !headers.contains("authorization"),
            "degraded mode must be header-less: {headers}"
        );
    }

    /// The shared selection helper maps `Ok(None)` → `NoActiveProfile`
    /// (never a silent pass-through) — the exact consumer behavior the
    /// main.rs call sites implement today.
    #[test]
    fn resolve_selected_maps_none_to_no_active_profile() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        isolate_config(&dir, "");
        let mut config = crate::config::load(&crate::config::config_path()).expect("load");
        let err = resolve_selected(&mut config, None).expect_err("none → error");
        assert!(matches!(err, CoreError::NoActiveProfile));
        unset("IGNITION_CLI_CONFIG");
    }
}
