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
//! | 2    | usage          | `confirmation_required`, `invalid_import_file`, `invalid_input` (clap renders its own usage errors — never hook clap)
//! | 3    | config         | `profile_not_found`, `no_active_profile`, `secret_unavailable`, `config_invalid`
//! | 4    | network        | `network_error`
//! | 5    | auth           | `auth_rejected`
//! | 6    | target_state   | `gateway_too_old`, `gateway_not_commissioned`, `gateway_restarting`, `not_found`, `project_exists`, `resource_binary`, `trial_not_expired` (04-03), `provider_not_found` (05-04), `routes_not_deployed`, `webdev_unlicensed`, `route_version_mismatch`, `webdev_route_error` (05-03), `tag_collision` (05-05), `alarm_journal_missing` (05-06)
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

    /// An import byte source the caller must fix (wrong file, too big,
    /// unreadable). Exit 2 — usage class: it names what the CALLER must
    /// change, like [`Self::ConfirmationRequired`] (03-02).
    #[error("invalid import file: {reason}")]
    InvalidImportFile { reason: String },

    /// A command input the caller must fix (unreadable `--file`, failed
    /// stdin read). Exit 2 — usage class, the generic sibling of
    /// [`Self::InvalidImportFile`] (03-03: `resource put`'s byte
    /// source).
    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },

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
    ///
    /// `source: None` marks a POLL deadline expiry (02-04 `poll.rs`):
    /// same class, same slug (`network_error`), no new variant — the
    /// transport-error `source` a real failure carries is simply absent,
    /// and `url` describes what was being waited on instead (the poll's
    /// subject + the last observation).
    #[error("gateway unreachable at {url}{source_note}", source_note = source.as_ref().map(|source| format!(": {source}")).unwrap_or_default())]
    Network {
        url: String,
        #[source]
        source: Option<reqwest::Error>,
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

    /// A project of this name already exists and the import's collision
    /// policy is abort — the CLI-side pre-check refused BEFORE any
    /// upload (the server's own answer remains the backstop). Exit 6 —
    /// target state: the command is invalid for the gateway's current
    /// state (03-02, the GatewayTooOld action-built-variant precedent:
    /// constructed by the actions layer, not classify).
    #[error("project {name} already exists on the gateway")]
    ProjectExists {
        /// The colliding project name.
        name: String,
        /// URL of the pre-check request, when known.
        endpoint: Option<String>,
    },

    /// A binary (data.bin-class) resource met the surgical JSON/text
    /// loop — REFUSED rather than corrupted through it. Exit 6 —
    /// target state: the command is invalid for that resource's
    /// nature; the export/import family owns binary resources
    /// (Pitfall 7).
    #[error("resource {path:?} has binary content — not editable via the resource loop")]
    ResourceBinary {
        /// The resource path that was refused.
        path: String,
        /// URL of the request involved, when known.
        endpoint: Option<String>,
    },

    /// The gateway refuses trial resets while the trial is still
    /// active — live-discovered on 8.3.3 during 04-03's spike: the
    /// reset POST answers 403 on a NON-expired trial (verified from
    /// the browser page itself with the exact UI headers), and 200 +
    /// the flip on an expired one. The action layer's expiry pre-check
    /// turns that misleading auth-shaped 403 into the honest
    /// target-state refusal. Exit 6 (the ProjectExists precedent:
    /// action-constructed, not classify).
    #[error(
        "trial is not expired ({remaining_s}s left) — the gateway only honors resets once the trial expires"
    )]
    TrialNotExpired {
        /// Seconds left on the active trial.
        remaining_s: i64,
        /// URL of the rig's trial endpoint, when known.
        endpoint: Option<String>,
    },

    /// The WebDev route family a command depends on is not deployed —
    /// the presence probe answered 405, the live-proven 8.3 absent
    /// marker (missing routes AND missing projects both answer 405,
    /// NOT 404; 05-RESEARCH Pitfall 1). Exit 6 — target state: the
    /// command is invalid until `ign webdev deploy` installs the
    /// routes (the TrialNotExpired precedent: action-constructed, not
    /// classify).
    #[error(
        "webdev routes are not deployed (probe of {route:?} in project {project:?} answered 405)"
    )]
    RoutesNotDeployed {
        /// The deploy project the probe targeted.
        project: String,
        /// The route folder the probe named.
        route: String,
        /// Path of the probe request, when known.
        endpoint: Option<String>,
    },

    /// The WebDev module answered 402 — installed but unlicensed (a
    /// trial-expired gateway; live-verified cross-version on 8.3.6,
    /// 05-RESEARCH §Servlet). Exit 6 — no `/system/webdev` route can
    /// answer until the gateway is licensed.
    #[error(
        "the WebDev module is unlicensed on this gateway (HTTP 402 — trial-expired rigs cannot serve /system/webdev routes)"
    )]
    WebdevUnlicensed {
        /// Path of the probe request, when known.
        endpoint: Option<String>,
    },

    /// A deployed route's handshake version differs from the embedded
    /// bundle's — the CLI refuses rather than auto-upgrading either
    /// side (roadmap-locked: actionable error, no auto-upgrade
    /// magic). Exit 6.
    #[error("route {route:?} version mismatch: deployed {deployed}, this CLI expects {expected}")]
    RouteVersionMismatch {
        /// The route folder that answered.
        route: String,
        /// The route's deployed `routeVersion`.
        deployed: String,
        /// The embedded bundle's version
        /// ([`crate::webdev::ROUTE_BUNDLE_VERSION`]).
        expected: String,
        /// Path of the probe request, when known.
        endpoint: Option<String>,
    },

    /// A tag provider of this name does not exist — the
    /// find→signature→delete chain's find half missed (05-04,
    /// TAGS-01). Exit 6 — target state: the named thing is absent
    /// (the ProjectExists precedent family: action-constructed, not
    /// classify — the honest, family-specific refusal over a bare
    /// 404).
    #[error("tag provider {name:?} not found on the gateway")]
    ProviderNotFound {
        /// The provider name that missed.
        name: String,
        /// URL of the find request, when known.
        endpoint: Option<String>,
    },

    /// A WebDev route answered HTTP 200 with a body denial
    /// (`{ok:false, error{code,message}}`) whose machine code this CLI
    /// does not specifically map — code + message ride verbatim so
    /// agents can branch on the stable route contract (05-01). Exit 6
    /// — target state: the deployed route refused the action.
    #[error("webdev route denied the call ({code}): {message}")]
    WebdevRouteError {
        /// The route's machine error code (stable contract).
        code: String,
        /// The route's human message.
        message: String,
        /// Path of the request, when known.
        endpoint: Option<String>,
    },

    /// A tag import under abort policy found EXISTING tags at the
    /// target provider (05-05, TAGS-09) — the browse pre-check
    /// refuses BEFORE any route write (the LOCKED Phase-3 collision
    /// matrix mapped onto configure's 'a'/'o'). Exit 6 — target
    /// state: the named tags exist; overwrite is the explicit,
    /// guarded opt-in.
    #[error(
        "tag collision importing into provider {provider:?}: {} already exist(s)",
        names.join(", ")
    )]
    TagCollision {
        /// The target provider the import was headed for.
        provider: String,
        /// The colliding top-level tag names the pre-check found.
        names: Vec<String>,
        /// URL of the pre-check browse request, when known.
        endpoint: Option<String>,
    },

    /// The gateway has no alarm-journal profile configured — alarm
    /// history has nowhere to read from. The alarms route's
    /// structured `no_alarm_journal` denial maps here (the
    /// denial_to_error seam, 05-06 TAGS-07): DEFAULT rigs hit this
    /// ALWAYS, because the journal is a config-resource chain —
    /// database connection + `ignition/alarm-journal` profile + the
    /// `general-alarm-settings` singleton pointing at it. Exit 6 —
    /// target state: the command is invalid for the gateway's
    /// current state until that chain is provisioned (the honest,
    /// actionable refusal over a bare route error).
    #[error("no alarm journal profile is configured on this gateway — alarm history has nothing to read")]
    AlarmJournalMissing {
        /// URL of the alarms route request, when known.
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
            Self::InvalidImportFile { .. } => "invalid_import_file",
            Self::InvalidInput { .. } => "invalid_input",
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
            Self::ProjectExists { .. } => "project_exists",
            Self::ResourceBinary { .. } => "resource_binary",
            Self::TrialNotExpired { .. } => "trial_not_expired",
            Self::ProviderNotFound { .. } => "provider_not_found",
            Self::RoutesNotDeployed { .. } => "routes_not_deployed",
            Self::WebdevUnlicensed { .. } => "webdev_unlicensed",
            Self::RouteVersionMismatch { .. } => "route_version_mismatch",
            Self::WebdevRouteError { .. } => "webdev_route_error",
            Self::TagCollision { .. } => "tag_collision",
            Self::AlarmJournalMissing { .. } => "alarm_journal_missing",
            Self::Rig(_) => "rig_error",
        }
    }

    /// The LOCKED exit-code mapping — the only place exit codes are decided.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Internal(_) => 1,
            Self::ConfirmationRequired { .. }
            | Self::InvalidImportFile { .. }
            | Self::InvalidInput { .. } => 2,
            Self::ProfileNotFound { .. }
            | Self::NoActiveProfile
            | Self::SecretUnavailable { .. }
            | Self::ConfigInvalid { .. } => 3,
            Self::Network { .. } => 4,
            Self::Auth { .. } => 5,
            Self::GatewayTooOld { .. }
            | Self::GatewayNotCommissioned { .. }
            | Self::GatewayRestarting { .. }
            | Self::NotFound { .. }
            | Self::ProjectExists { .. }
            | Self::ResourceBinary { .. }
            | Self::TrialNotExpired { .. }
            | Self::ProviderNotFound { .. }
            | Self::RoutesNotDeployed { .. }
            | Self::WebdevUnlicensed { .. }
            | Self::RouteVersionMismatch { .. }
            | Self::WebdevRouteError { .. }
            | Self::TagCollision { .. }
            | Self::AlarmJournalMissing { .. } => 6,
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
            Self::InvalidImportFile { .. } => Some(
                "import expects a project-export ZIP (PK\\x03\\x04 magic) of at \
                   most 512 MB — pass a file produced by `ign project export` \
                   via --file (or `-` to pipe one on stdin)"
                    .to_string(),
            ),
            Self::InvalidInput { .. } => Some(
                "fix the input source — a readable file path via --file, or `-` \
                   to pipe the content on stdin"
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
            Self::ProjectExists { .. } => Some(
                "the default collision policy refuses to overwrite; re-run with \
                  --collision-policy overwrite to replace it — overwrite \
                  REPLACES the ENTIRE project (resources absent from the ZIP \
                  are deleted; merge is Designer-only)"
                    .to_string(),
            ),
            Self::ResourceBinary { .. } => Some(
                "resource content is binary — use `ign project export`/`import` \
                  for data.bin-class resources"
                    .to_string(),
            ),
            Self::TrialNotExpired { .. } => Some(
                "wait for the trial to expire (watch `ign rig trial status`), or \
                  run `ign rig reset --yes` for a completely fresh trial volume"
                    .to_string(),
            ),
            Self::RoutesNotDeployed { .. } => Some(
                "run `ign webdev deploy` to install the CLI's WebDev routes into \
                  the gateway, then retry"
                    .to_string(),
            ),
            Self::WebdevUnlicensed { .. } => Some(
                "license the gateway — the WebDev module answers 402 while \
                  unlicensed (on a rig, `ign rig trial reset --yes` restarts an \
                  expired trial)"
                    .to_string(),
            ),
            Self::RouteVersionMismatch { deployed, expected, .. } => {
                // Direction decides the fix (roadmap criterion): an older
                // deployed route → redeploy from THIS binary; a NEWER
                // deployed route → this CLI is behind (the route bundle
                // travels with the binary). Same slug either way.
                let newer = semver::Version::parse(deployed)
                    .ok()
                    .zip(semver::Version::parse(expected).ok())
                    .is_some_and(|(deployed, expected)| deployed > expected);
                Some(if newer {
                    "the deployed routes are NEWER than this CLI — update ign \
                      (the route bundle travels with the binary)"
                        .to_string()
                } else {
                    "run `ign webdev deploy` to redeploy the route version \
                      this CLI expects"
                        .to_string()
                })
            }
            Self::ProviderNotFound { .. } => Some(
                "check the provider name; `ign tags provider list` shows the \
                  gateway's tag providers"
                    .to_string(),
            ),
            Self::TagCollision { .. } => Some(
                "re-run with --collision-policy overwrite to replace the \
                  existing tags (destructive: requires --yes)"
                    .to_string(),
            ),
            Self::AlarmJournalMissing { .. } => Some(
                "alarm history needs a journal profile — provision a database \
                  connection + alarm-journal profile on the gateway (and point \
                  the general-alarm-settings singleton at it), then retry; see \
                  the README 'Alarm history' section"
                    .to_string(),
            ),
            Self::WebdevRouteError { code, .. } => Some(if code == "secret_required" || code == "secret_mismatch" {
                "the scriptExec route is secret-gated — deploy it with `ign \
                  webdev deploy --with-script-exec` (the secret is generated \
                  and stored in the profile config at 0600); a mismatch means \
                  the route was deployed with a different secret: redeploy or \
                  pass --rotate-secret"
                    .to_string()
            } else {
                "the deployed route refused the action — the code and message \
                  are the route's stable contract; `ign webdev status` \
                  diagnoses the deployment"
                    .to_string()
            }),
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
            | Self::NotFound { endpoint }
            | Self::ProjectExists { endpoint, .. }
            | Self::ResourceBinary { endpoint, .. }
            | Self::TrialNotExpired { endpoint, .. }
            | Self::ProviderNotFound { endpoint, .. }
            | Self::RoutesNotDeployed { endpoint, .. }
            | Self::WebdevUnlicensed { endpoint }
            | Self::RouteVersionMismatch { endpoint, .. }
            | Self::WebdevRouteError { endpoint, .. }
            | Self::TagCollision { endpoint, .. }
            | Self::AlarmJournalMissing { endpoint } => endpoint.clone(),
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
            source: Some(source),
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
                CoreError::InvalidImportFile {
                    reason: "missing ZIP magic".into(),
                },
                2,
                "invalid_import_file",
            ),
            (
                CoreError::InvalidInput {
                    reason: "cannot read put.json".into(),
                },
                2,
                "invalid_input",
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
            (
                CoreError::ProjectExists {
                    name: "PlantFloor".into(),
                    endpoint: None,
                },
                6,
                "project_exists",
            ),
            (
                CoreError::ResourceBinary {
                    path: "com.x/perspective/session-permissions".into(),
                    endpoint: None,
                },
                6,
                "resource_binary",
            ),
            (
                CoreError::TrialNotExpired {
                    remaining_s: 6590,
                    endpoint: Some("http://localhost:9088/data/api/v1/trial".into()),
                },
                6,
                "trial_not_expired",
            ),
            (
                CoreError::ProviderNotFound {
                    name: "nope".into(),
                    endpoint: Some("/data/api/v1/resources/find/ignition/tag-provider/nope".into()),
                },
                6,
                "provider_not_found",
            ),
            (
                CoreError::RoutesNotDeployed {
                    project: "ign-cli".into(),
                    route: "tags".into(),
                    endpoint: Some("/system/webdev/ign-cli/cli/tags".into()),
                },
                6,
                "routes_not_deployed",
            ),
            (
                CoreError::WebdevUnlicensed {
                    endpoint: Some("/system/webdev/ign-cli/cli/tags".into()),
                },
                6,
                "webdev_unlicensed",
            ),
            (
                CoreError::RouteVersionMismatch {
                    route: "tags".into(),
                    deployed: "0.9.0".into(),
                    expected: "1.0.0".into(),
                    endpoint: Some("/system/webdev/ign-cli/cli/tags".into()),
                },
                6,
                "route_version_mismatch",
            ),
            (
                CoreError::WebdevRouteError {
                    code: "route_error".into(),
                    message: "boom".into(),
                    endpoint: Some("/system/webdev/ign-cli/cli/tags".into()),
                },
                6,
                "webdev_route_error",
            ),
            (
                CoreError::TagCollision {
                    provider: "p5import".into(),
                    names: vec!["T1".into(), "P5".into()],
                    endpoint: Some("/system/webdev/ign-cli/cli/tags".into()),
                },
                6,
                "tag_collision",
            ),
            (
                CoreError::AlarmJournalMissing {
                    endpoint: Some("/system/webdev/ign-cli/cli/alarms".into()),
                },
                6,
                "alarm_journal_missing",
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

        // The WebDev refusal matrix (05-03): every hint names the fix
        // — `ign webdev deploy` for absent/older routes, `update ign`
        // for newer ones (the roadmap's actionable-error criterion).
        let undeployed = CoreError::RoutesNotDeployed {
            project: "ign-cli".into(),
            route: "tags".into(),
            endpoint: None,
        };
        let hint = undeployed.hint().expect("hint required");
        assert!(
            hint.contains("ign webdev deploy"),
            "absent-routes hint must name the fix: {hint}"
        );

        let older = CoreError::RouteVersionMismatch {
            route: "tags".into(),
            deployed: "0.9.0".into(),
            expected: "1.0.0".into(),
            endpoint: None,
        };
        let hint = older.hint().expect("hint required");
        assert!(
            hint.contains("ign webdev deploy") && !hint.contains("update ign"),
            "older-route hint says redeploy: {hint}"
        );

        let newer = CoreError::RouteVersionMismatch {
            route: "tags".into(),
            deployed: "1.1.0".into(),
            expected: "1.0.0".into(),
            endpoint: None,
        };
        let hint = newer.hint().expect("hint required");
        assert!(
            hint.contains("update ign") && !hint.contains("ign webdev deploy"),
            "newer-route hint says update ign: {hint}"
        );

        let secret_gate = CoreError::WebdevRouteError {
            code: "secret_required".into(),
            message: "missing x-ignition-cli-secret header".into(),
            endpoint: None,
        };
        let hint = secret_gate.hint().expect("hint required");
        assert!(
            hint.contains("--with-script-exec"),
            "secret-gate hint names the deploy flag: {hint}"
        );

        // The alarm-journal refusal (05-06): the hint names the missing
        // provisioning chain AND the README section.
        let journal = CoreError::AlarmJournalMissing { endpoint: None };
        let hint = journal.hint().expect("hint required");
        assert!(
            hint.contains("journal profile") && hint.contains("database connection"),
            "journal hint names the chain: {hint}"
        );
        assert!(
            hint.contains("README"),
            "journal hint points at the README section: {hint}"
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
