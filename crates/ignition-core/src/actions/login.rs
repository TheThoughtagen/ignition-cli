//! `ign session login` (QUICK-tg4) — the native IdP session exposed as
//! JSON, in the shape a Playwright `globalSetup` can consume verbatim.
//!
//! The flow itself is [`crate::client::idp`]'s, untouched: this module
//! is a PROJECTION of the `GatewaySession` that the trial-reset ladder
//! and `ign adopt` already obtain, plus the Playwright `storageState`
//! document derived from it.
//!
//! ## Why the verb exists
//!
//! Every Ignition browser-E2E suite otherwise reinvents the auth
//! bootstrap: drive the login form with a headless browser, scrape the
//! cookie, hope the IdP's challenge shape did not move. The dance is
//! already implemented here and already live-verified; exposing it is
//! one projection, and the scaffold `ign e2e init` writes consumes the
//! `storage_state` field directly.
//!
//! ## The exposure discipline
//!
//! This verb emits a LIVE credential by design — that is its product.
//! The discipline is that the human render withholds it (see
//! [`crate::actions::login::SessionLoginResult`] consumers in the CLI's
//! `render.rs`): the cookie NAME and the gateway URL are printed, the
//! cookie VALUE and the CSRF token are not, and the material is
//! reachable only under the global `--json` flag where a machine asked
//! for it on purpose.

use serde::{Deserialize, Serialize};

use crate::client::idp::{self, GatewaySession, IdpLoginFlow};
use crate::config::Secret;
use crate::error::CoreError;

/// `ign session login` options. The password never rides here — it is a
/// [`Secret`] parameter, the redaction discipline's single channel.
#[derive(Debug, Clone)]
pub struct SessionLoginOptions {
    /// The gateway username the IdP challenge is answered with.
    pub username: String,
}

/// One Playwright `storageState` cookie entry.
///
/// The wire spelling is PLAYWRIGHT's (`httpOnly`, `sameSite`), not
/// Rust's — a snake_cased key here produces a storage state Playwright
/// silently ignores, which is why the round trip is unit-pinned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageCookie {
    /// The session cookie's name (`webui-sid-<gatewayId>`).
    pub name: String,
    /// The session cookie's value.
    pub value: String,
    /// The gateway host WITHOUT its port — Playwright's cookie shape
    /// rejects a port inside `domain`.
    pub domain: String,
    /// Always `/` (the gateway sets `Path=/`).
    pub path: String,
    /// Always true (the gateway sets `HttpOnly`).
    pub http_only: bool,
    /// True only for an `https` gateway URL.
    pub secure: bool,
    /// Always `Strict` (the gateway's own `SameSite` attribute).
    pub same_site: String,
    /// `-1` — a session cookie. Typed `f64` so it serializes unquoted
    /// in the shape Playwright's own storage-state dumps use.
    pub expires: f64,
}

/// A Playwright `storageState` document — exactly two keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageState {
    /// The one-element cookie array.
    pub cookies: Vec<StorageCookie>,
    /// Always empty — the gateway session is cookie-borne; nothing
    /// lands in `localStorage`.
    pub origins: Vec<serde_json::Value>,
}

/// The `ign session login` envelope. ALL keys always present (the
/// family convention).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionLoginResult {
    /// The session cookie's name.
    pub cookie_name: String,
    /// The session cookie's value — the live credential.
    pub cookie_value: String,
    /// The CSRF token for `X-CSRF-Token` on gateway mutations.
    pub csrf_token: String,
    /// The gateway the session belongs to (the profile URL).
    pub gateway_url: String,
    /// The same session as a Playwright `storageState` document.
    pub storage_state: StorageState,
}

/// `SameSite` value the gateway's own `Set-Cookie` carries.
const SAME_SITE: &str = "Strict";

/// Project a [`GatewaySession`] into a Playwright `storageState`.
///
/// Pure — the cookie-shape rules (D7) are unit-testable without a
/// server. A `base_url` that will not parse is [`CoreError::Internal`],
/// matching [`IdpLoginFlow::new`]'s own posture on the same input.
pub fn storage_state_for(
    session: &GatewaySession,
    base_url: &str,
) -> Result<StorageState, CoreError> {
    let url = url::Url::parse(base_url)
        .map_err(|err| CoreError::Internal(format!("invalid gateway URL: {err}")))?;
    let domain = url
        .host_str()
        .ok_or_else(|| CoreError::Internal(format!("gateway URL {base_url:?} carries no host")))?
        .to_string();
    Ok(StorageState {
        cookies: vec![StorageCookie {
            name: session.cookie_name.clone(),
            value: session.cookie_value.clone(),
            domain,
            path: "/".into(),
            http_only: true,
            secure: url.scheme() == "https",
            same_site: SAME_SITE.into(),
            expires: -1.0,
        }],
        origins: Vec::new(),
    })
}

/// Run the IdP dance and project the session.
///
/// Reuses [`idp::login`] verbatim — there is exactly ONE login path in
/// this crate and this verb does not add a second.
pub async fn session_login(
    base_url: &str,
    opts: &SessionLoginOptions,
    password: &Secret,
) -> Result<SessionLoginResult, CoreError> {
    let flow = IdpLoginFlow::new(base_url)?;
    let (_flow, session) = idp::login(flow, &opts.username, password).await?;
    let storage_state = storage_state_for(&session, base_url)?;
    Ok(SessionLoginResult {
        cookie_name: session.cookie_name,
        cookie_value: session.cookie_value,
        csrf_token: session.csrf_token,
        gateway_url: base_url.to_string(),
        storage_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_session() -> GatewaySession {
        GatewaySession {
            cookie_name: "webui-sid-1766878194".into(),
            cookie_value: "session-value".into(),
            csrf_token: "csrf-value".into(),
        }
    }

    #[test]
    fn storage_state_shape_follows_the_playwright_rules() {
        let state = storage_state_for(&fixture_session(), "http://localhost:9088")
            .expect("a parseable URL");
        assert_eq!(state.origins.len(), 0, "origins stays empty");
        assert_eq!(state.cookies.len(), 1, "exactly one cookie");
        let cookie = &state.cookies[0];
        assert_eq!(cookie.domain, "localhost", "the port is NOT in domain");
        assert_eq!(cookie.path, "/");
        assert!(cookie.http_only);
        assert!(!cookie.secure, "an http gateway is not a secure cookie");
        assert_eq!(cookie.same_site, "Strict");
        assert_eq!(cookie.expires, -1.0);
    }

    #[test]
    fn an_https_gateway_marks_the_cookie_secure() {
        let state = storage_state_for(&fixture_session(), "https://gw.example.com:8043")
            .expect("a parseable URL");
        assert!(state.cookies[0].secure);
        assert_eq!(
            state.cookies[0].domain, "gw.example.com",
            "the port is stripped from https too"
        );
    }

    #[test]
    fn an_unparseable_url_is_internal_not_a_panic() {
        let err =
            storage_state_for(&fixture_session(), "not a url").expect_err("a garbage URL refuses");
        assert_eq!(err.exit_code(), 1, "the IdpLoginFlow::new posture");
    }

    /// THE wire-spelling pin: a Rust-cased key here produces a storage
    /// state Playwright ignores without complaint.
    #[test]
    fn the_wire_keys_are_playwrights_camel_case() {
        let state = storage_state_for(&fixture_session(), "http://localhost:9088").unwrap();
        let wire = serde_json::to_value(&state).expect("serializes");
        let cookie = &wire["cookies"][0];
        assert!(cookie.get("httpOnly").is_some(), "httpOnly, not http_only");
        assert!(cookie.get("sameSite").is_some(), "sameSite, not same_site");
        assert!(cookie.get("http_only").is_none());
        assert!(cookie.get("same_site").is_none());
        // -1 must not be quoted — Playwright reads a number here.
        assert!(cookie["expires"].is_number(), "expires rides as a number");
        let back: StorageState = serde_json::from_value(wire).expect("round-trips");
        assert_eq!(back, state);
    }
}
