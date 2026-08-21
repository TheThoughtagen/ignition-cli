//! Typed error taxonomy — THE exit-code contract for `ign`.
//!
//! The mapping error-class → exit code is LOCKED (Phase-1 API freeze). The
//! table exists in exactly two places: [`CoreError::exit_code`] here and the
//! README — kept in sync by the `exit_code_mapping_enumerated` unit test and
//! the golden-file tests in `crates/ignition-cli/tests/`.
//!
//! | exit | class          | slugs
//! |------|----------------|-----------------------------------------------
//! | 1    | internal       | `internal`
//! | 2    | usage          | `confirmation_required` (clap renders its own usage errors — never hook clap)
//! | 3    | config         | `profile_not_found`, `no_active_profile`, `secret_unavailable`, `config_invalid`
//! | 4    | network        | `network_error`
//! | 5    | auth           | `auth_rejected`
//! | 6    | target_state   | `gateway_too_old`, `gateway_not_commissioned`, `gateway_restarting`, `not_found`
//! | 7    | rig            | `rig_error` (reserved — first used in Phase 4)
//!
//! Slugs are public contract: never respell them. Exit codes are public
//! contract: never renumber them (the enumerated test guards both).

use serde::Serialize;

/// Every failure `ign` can report. One variant per contract class; `code()`,
/// `exit_code()`, `hint()` are total functions over it.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Unexpected runtime failure — the catch-all; report as a bug. Exit 1.
    #[error("internal error: {0}")]
    Internal(String),

    /// Destructive operation invoked without `--yes`. Exit 2 (same class as
    /// usage: it names a flag the caller must add; clap renders its own
    /// usage errors with its exit 2 — never hook clap).
    #[error("{operation} is destructive; rerun with --yes to confirm")]
    ConfirmationRequired { operation: String },

    /// Named profile absent from config. Exit 3.
    #[error("profile {name:?} not found (known profiles: {known:?})")]
    ProfileNotFound { name: String, known: Vec<String> },

    /// No `--profile`, no `IGNITION_PROFILE`, no active profile in config.
    /// Exit 3. Constructible from the CLI once config resolution lands
    /// (01-03); the taxonomy is complete on day one.
    #[error("no active profile configured")]
    NoActiveProfile,

    /// No credential resolvable for the profile (env, token_env, keyring all
    /// missed or failed). Exit 3.
    #[error("secret unavailable for profile {profile:?}")]
    SecretUnavailable { profile: String },

    /// Config file unreadable or wrong shape. Exit 3.
    #[error("invalid configuration: {reason}")]
    ConfigInvalid { reason: String },

    /// Gateway unreachable / timeout / TLS failure. Exit 4.
    #[error("gateway unreachable at {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// Gateway reachable but rejected credentials (401/403). Exit 5.
    #[error("gateway rejected credentials (HTTP {status})")]
    Auth {
        status: u16,
        /// URL/path of the request that was rejected, when known.
        endpoint: Option<String>,
    },

    /// Gateway reachable but the command is invalid for its current state —
    /// version below minimum, uncommissioned, mid-restart, or a missing
    /// resource. Exit 6.
    #[error("gateway version {found} is below minimum {minimum}")]
    GatewayTooOld {
        found: String,
        minimum: String,
        /// URL/path of the request that answered, when known.
        endpoint: Option<String>,
    },

    /// Gateway reachable but uncommissioned — every `/data` route 302s to
    /// `/welcome` (verified on a fresh 8.3.6 container; 02-RESEARCH
    /// §Error-Body Sniffing). Exit 6.
    #[error("gateway at {} is not commissioned", endpoint.as_deref().unwrap_or("unknown address"))]
    GatewayNotCommissioned {
        /// URL that was redirected to the commissioning wizard.
        endpoint: Option<String>,
    },

    /// Gateway restarting — webserver answers (503) but services are down
    /// (verified restart lifecycle: webserver never drops the connection).
    /// Exit 6.
    #[error("gateway is restarting (webserver up, services down)")]
    GatewayRestarting {
        /// URL that answered 503.
        endpoint: Option<String>,
    },

    /// Named resource absent (404) — terminating a nonexistent session id,
    /// an unknown path, or a pre-8.3 gateway's JSON
    /// `{"message": "No route match for path: …"}`. Exit 6.
    #[error("resource not found on the gateway")]
    NotFound {
        /// URL that answered 404.
        endpoint: Option<String>,
    },

    /// Docker/compose rig failure. Exit 7. Reserved — first used in Phase 4;
    /// trivially constructible so the taxonomy enumerates completely today.
    #[error("rig error: {0}")]
    Rig(String),
}

impl CoreError {
    /// Stable machine slug — public contract, never respell.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Internal(_) => "internal",
            Self::ConfirmationRequired { .. } => "confirmation_required",
            Self::ProfileNotFound { .. } => "profile_not_found",
            Self::NoActiveProfile => "no_active_profile",
            Self::SecretUnavailable { .. } => "secret_unavailable",
            Self::ConfigInvalid { .. } => "config_invalid",
            Self::Network { .. } => "network_error",
            Self::Auth { .. } => "auth_rejected",
            Self::GatewayTooOld { .. } => "gateway_too_old",
            Self::GatewayNotCommissioned { .. } => "gateway_not_commissioned",
            Self::GatewayRestarting { .. } => "gateway_restarting",
            Self::NotFound { .. } => "not_found",
            Self::Rig(_) => "rig_error",
        }
    }

    /// The LOCKED exit-code mapping — the only place exit codes are decided.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Internal(_) => 1,
            Self::ConfirmationRequired { .. } => 2,
            Self::ProfileNotFound { .. }
            | Self::NoActiveProfile
            | Self::SecretUnavailable { .. }
            | Self::ConfigInvalid { .. } => 3,
            Self::Network { .. } => 4,
            Self::Auth { .. } => 5,
            Self::GatewayTooOld { .. }
            | Self::GatewayNotCommissioned { .. }
            | Self::GatewayRestarting { .. }
            | Self::NotFound { .. } => 6,
            Self::Rig(_) => 7,
        }
    }

    /// Actionable next step (CORE-05). Every class carries one.
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Internal(_) => Some(
                "internal errors are bugs; re-run with -vv and report the \
                 diagnostics output"
                    .to_string(),
            ),
            Self::ConfirmationRequired { .. } => Some(
                "this operation is destructive; re-run with --yes or set \
                  IGNITION_YES=1"
                    .to_string(),
            ),
            Self::ProfileNotFound { known, .. } => Some(if known.is_empty() {
                "no profiles configured yet; run `ign profile add` to create \
                     one"
                .to_string()
            } else {
                format!(
                    "known profiles: {}; run `ign profile add` to add another",
                    known.join(", ")
                )
            }),
            Self::NoActiveProfile => Some(
                "pass --profile NAME, set IGNITION_PROFILE, or mark a profile \
                 active with `ign profile use`"
                    .to_string(),
            ),
            Self::SecretUnavailable { profile } => Some(format!(
                "set IGNITION_TOKEN (or token_env in the profile), or store a \
                 keyring entry: service 'ignition-cli', user 'profile:{profile}'"
            )),
            Self::ConfigInvalid { .. } => Some(
                "verify the config file is valid TOML with [profiles.NAME] \
                 tables; `ign profile add` writes a known-good one"
                    .to_string(),
            ),
            Self::Network { url, .. } => Some(format!(
                "check the gateway is reachable at {url} (host, port, VPN, TLS)"
            )),
            Self::Auth { status, .. } => Some(match status {
                401 => {
                    // 401 = token not recognized — the #1 setup failure is
                    // a key-only header (verified: key-only → 401, full
                    // `name:key` → 200; Basic is dead on 8.3 /data).
                    "token not recognized — the X-Ignition-API-Token header must be the FULL `name:key` string from the gateway UI (Platform→Security→API Keys); Basic auth does not work on 8.3 /data routes — create an API token"
                }
                403 => {
                    // 403 = recognized but under-permitted (verified
                    // semantics; see 02-RESEARCH Auth §4/§5).
                    "token recognized but under-permitted — Ignition token setup is three parts: (1) token holds an adequate security level, (2) gateway read/write permissions include that level, (3) 'Require secure connections' is unchecked for http gateways; run `ign doctor` for a diagnosis"
                }
                _ => "check the credential; Ignition token setup is three parts: security level, write permissions, token assignment",
            }
            .to_string()),
            Self::GatewayTooOld { minimum, .. } => {
                Some(format!("upgrade the gateway to at least {minimum}"))
            }
            Self::GatewayNotCommissioned { .. } => Some(
                "open http://<host>:<port>/welcome in a browser and complete the \
                 commissioning wizard"
                    .to_string(),
            ),
            Self::GatewayRestarting { .. } => Some(
                "wait for readiness with `ign wait restart` or retry in ~1 minute".to_string(),
            ),
            Self::NotFound { .. } => Some(
                "check the id/path; a 404 JSON 'No route match' body can also mean \
                 a pre-8.3 gateway"
                    .to_string(),
            ),
            Self::Rig(_) => Some(
                "check Docker is running and inspect the rig containers \
                 (docker ps)"
                    .to_string(),
            ),
        }
    }

    /// URL/path of the request involved, when one was — populated for the
    /// network, auth, and target-state classes (CORE-05).
    pub fn endpoint(&self) -> Option<String> {
        match self {
            Self::Network { url, .. } => Some(url.clone()),
            Self::Auth { endpoint, .. } => endpoint.clone(),
            Self::GatewayTooOld { endpoint, .. }
            | Self::GatewayNotCommissioned { endpoint }
            | Self::GatewayRestarting { endpoint }
            | Self::NotFound { endpoint } => endpoint.clone(),
            _ => None,
        }
    }

    /// Build the LOCKED failure envelope for this error (field order is part
    /// of the golden contract: `ok`, `profile`, `error` then `code`,
    /// `message`, `endpoint`, `hint`).
    pub fn envelope<'a>(&self, profile: Option<&'a str>) -> ErrorEnvelope<'a> {
        ErrorEnvelope {
            ok: false,
            profile,
            error: ErrorBody {
                code: self.code(),
                message: self.to_string(),
                endpoint: self.endpoint(),
                hint: self.hint(),
            },
        }
    }
}

/// LOCKED failure envelope shape: exactly the top-level fields `ok`,
/// `profile`, `error` — changing the set is a breaking change for agents.
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope<'a> {
    /// Always `false` in this envelope.
    pub ok: bool,
    /// Active profile echoed in every output (CORE-01); `None` until config
    /// resolution lands.
    pub profile: Option<&'a str>,
    /// The typed error body.
    pub error: ErrorBody,
}

/// LOCKED error body: `code` (stable slug), `message` (human-readable),
/// `endpoint` (when a request was involved), `hint` (actionable next step).
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Stable slug from [`CoreError::code`] — never respelled.
    pub code: &'static str,
    /// Human-readable description.
    pub message: String,
    /// URL/path when a request was involved.
    pub endpoint: Option<String>,
    /// Actionable next step.
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{CoreError, ErrorBody, ErrorEnvelope};

    /// Build a real `reqwest::Error` for the Network variant: a request to
    /// an unroutable loopback port fails at connect time (instant refusal —
    /// `reqwest::Error` has no public constructor).
    fn network_error() -> CoreError {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let url = "http://127.0.0.1:1";
        let source = rt
            .block_on(reqwest::get(url))
            .expect_err("request to an unroutable port must fail");
        CoreError::Network {
            url: url.to_string(),
            source,
        }
    }

    /// Pitfall-5 guard: the FULL 1–7 taxonomy enumerated on day one so no
    /// later phase can silently renumber it or respell a slug. The slugs are
    /// asserted against literals — that IS the stability contract.
    #[test]
    fn exit_code_mapping_enumerated() {
        let cases: Vec<(CoreError, u8, &'static str)> = vec![
            (CoreError::Internal("boom".into()), 1, "internal"),
            (
                CoreError::ConfirmationRequired {
                    operation: "project download".into(),
                },
                2,
                "confirmation_required",
            ),
            (
                CoreError::ProfileNotFound {
                    name: "nope".into(),
                    known: vec!["dev".into()],
                },
                3,
                "profile_not_found",
            ),
            (CoreError::NoActiveProfile, 3, "no_active_profile"),
            (
                CoreError::SecretUnavailable {
                    profile: "dev".into(),
                },
                3,
                "secret_unavailable",
            ),
            (
                CoreError::ConfigInvalid {
                    reason: "bad toml".into(),
                },
                3,
                "config_invalid",
            ),
            (network_error(), 4, "network_error"),
            (
                CoreError::Auth {
                    status: 401,
                    endpoint: None,
                },
                5,
                "auth_rejected",
            ),
            (
                CoreError::GatewayTooOld {
                    found: "8.1.0".into(),
                    minimum: "8.3.1".into(),
                    endpoint: None,
                },
                6,
                "gateway_too_old",
            ),
            (
                CoreError::GatewayNotCommissioned {
                    endpoint: Some("http://gw:8088/data/api/v1/gateway-info".into()),
                },
                6,
                "gateway_not_commissioned",
            ),
            (
                CoreError::GatewayRestarting {
                    endpoint: Some("http://gw:8088/data/api/v1/gateway-info".into()),
                },
                6,
                "gateway_restarting",
            ),
            (
                CoreError::NotFound {
                    endpoint: Some("http://gw:8088/data/api/v1/designer/42".into()),
                },
                6,
                "not_found",
            ),
            (CoreError::Rig("compose up failed".into()), 7, "rig_error"),
        ];
        for (err, code, slug) in cases {
            assert_eq!(err.exit_code(), code, "wrong exit code for: {err}");
            assert_eq!(err.code(), slug, "unstable slug for: {err}");
        }
    }

    /// CORE-05: config, auth, and target-state classes carry actionable
    /// hints (and every other class does too).
    #[test]
    fn hints_are_actionable_for_config_auth_target_state() {
        let profile_not_found = CoreError::ProfileNotFound {
            name: "x".into(),
            known: vec!["dev".into(), "prod".into()],
        };
        let hint = profile_not_found.hint().expect("hint required");
        assert!(
            hint.contains("dev"),
            "hint must list known profiles: {hint}"
        );
        assert!(
            hint.contains("ign profile add"),
            "hint must name the fix: {hint}"
        );

        let auth = CoreError::Auth {
            status: 403,
            endpoint: None,
        };
        let hint = auth.hint().expect("hint required");
        assert!(
            hint.contains("three parts"),
            "auth hint must name the three-part token setup: {hint}"
        );

        // Status-aware auth hints (02-RESEARCH Auth §4): 401 = not
        // recognized (name:key format), 403 = under-permitted.
        let unauthorized = CoreError::Auth {
            status: 401,
            endpoint: None,
        };
        let hint = unauthorized.hint().expect("hint required");
        assert!(
            hint.contains("name:key"),
            "401 hint must name the full name:key token format: {hint}"
        );
        assert!(
            hint.contains("API token"),
            "401 hint must say Basic cannot work: {hint}"
        );
        let hint403 = auth.hint().expect("hint required");
        assert!(
            hint403.contains("secure connections"),
            "403 hint must name the secure-channel part: {hint403}"
        );

        let too_old = CoreError::GatewayTooOld {
            found: "8.1.0".into(),
            minimum: "8.3.1".into(),
            endpoint: None,
        };
        let hint = too_old.hint().expect("hint required");
        assert!(
            hint.contains("8.3.1"),
            "target-state hint must name the minimum: {hint}"
        );

        // Totality: no class silently loses its hint later.
        let no_active = CoreError::NoActiveProfile;
        assert!(no_active.hint().is_some());
    }

    /// The failure envelope's serialized field order and endpoint population
    /// are contract: `ok`, `profile`, `error` / `code`, `message`,
    /// `endpoint`, `hint` (string-level comparison because `serde_json::Value`
    /// maps are key-sorted and would hide ordering).
    #[test]
    fn error_envelope_locked_shape_and_endpoint() {
        let auth = CoreError::Auth {
            status: 401,
            endpoint: Some("https://gw.example.com/data/api/v1/gateway-info".into()),
        };
        let envelope: ErrorEnvelope<'_> = auth.envelope(Some("dev"));
        let json = serde_json::to_string(&envelope).expect("serialize envelope");

        assert_eq!(
            json,
            concat!(
                r#"{"ok":false,"profile":"dev","error":{"code":"auth_rejected","#,
                r#""message":"gateway rejected credentials (HTTP 401)","#,
                r#""endpoint":"https://gw.example.com/data/api/v1/gateway-info","#,
                r#""hint":"token not recognized — the X-Ignition-API-Token header must be the FULL `name:key` string from the gateway UI (Platform→Security→API Keys); Basic auth does not work on 8.3 /data routes — create an API token"}}"#
            )
        );

        // Classes without a request involved carry no endpoint.
        let no_request = CoreError::NoActiveProfile;
        let body: &ErrorBody = &no_request.envelope(None).error;
        assert_eq!(body.endpoint, None);
    }
}
