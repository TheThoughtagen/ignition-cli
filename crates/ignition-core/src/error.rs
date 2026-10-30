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
//! | 2    | usage          | `confirmation_required`, `invalid_import_file`, `invalid_input`, `gateway_client_error` (09-01), `deploy_tree_invalid`, `embedded_secret` (21-03), `destructive_change_refused` (23-02) (clap renders its own usage errors — never hook clap)
//! | 3    | config         | `profile_not_found`, `no_active_profile`, `secret_unavailable`, `config_invalid`, `poll_interval_too_small` (08-01), `module_not_registered` (16-01)
//! | 4    | network        | `network_error`, `module_feed_unreachable` (15-01), `transport_failed` (22-01)
//! | 5    | auth           | `auth_rejected`
//! | 6    | target_state   | `gateway_too_old`, `gateway_not_commissioned`, `gateway_restarting`, `not_found`, `project_exists`, `resource_binary`, `trial_not_expired` (04-03), `provider_not_found` (05-04), `routes_not_deployed`, `webdev_unlicensed`, `route_version_mismatch`, `webdev_route_error` (05-03), `tag_collision` (05-05), `alarm_journal_missing` (05-06), `import_denied` (05-07), `session_not_prunable` (06-07), `eam_not_controller` (07-02), `eam_task_type_refused` (07-02), `eam_task_in_flight` (07-06), `script_exec_not_configured` (07-03), `lint_tool_absent` (07-04), `provider_root_unsupported` (07-06), `bundle_not_available` (09-07), `module_release_not_found`, `module_digest_mismatch`, `module_feed_unusable` (15-01), `module_digest_changed` (15-02), `module_uninstall_denied` (19-01), `scan_lock_held` (20-02), `resource_signature_rejected` (20-02), `resource_exists` (20-REVIEW WR-01), `mode_exists` (20-REVIEW WR-01), `deploy_ledger_invalid` (22-03), `plan_stale`, `deploy_blocked`, `deploy_preflight_failed`, `deploy_journal_present`, `deploy_live_changed`, `deploy_run_invalid`, `rollback_failed` (23-02), `project_git_managed` (25-07), `deploy_unreadable` (quick-261029-ms3)
//! | 7    | rig            | `rig_error` (reserved — first used in Phase 4)
//!
//! Slugs are public contract: never respell them. Exit codes are public
//! contract: never renumber them (the enumerated test guards both).

use serde::Serialize;

/// The `ign tui` TTY-refusal reason (06-07). The InvalidInput hint is
/// content-addressed off this exact string: the frozen taxonomy keeps
/// ONE usage-input variant (no hint field, no new variant), but the
/// TTY refusal's fix is terminal-related, not `--file`/stdin —
/// [`CoreError::hint`] special-cases this one reason while every other
/// raise site keeps the resource-put default. Construct via
/// [`CoreError::tui_tty_refusal`] so the reason/hint pair cannot drift.
pub const TUI_TTY_REFUSAL_REASON: &str = "ign tui requires a terminal (stdout is not a TTY)";

/// The `ign top` (itop) TTY-refusal reason — the SAME usage class and
/// slug as [`TUI_TTY_REFUSAL_REASON`] at one command over (the monitor
/// is its own full-screen surface, so its refusal names ITS command —
/// the RoutesNotDeployed rule: a hint must never name a command that
/// does not fix it). [`CoreError::hint`] content-addresses this
/// constant exactly like its tui sibling.
pub const TOP_TTY_REFUSAL_REASON: &str = "ign top requires a terminal (stdout is not a TTY)";

/// The TAGS-12 loss-gate refusal reason prefix (11-07 gap closure). The
/// InvalidInput hint is content-addressed off this literal: the loss
/// gate's reason is DYNAMIC prose (the CLI's `render_loss_prose` header
/// `loss report ({label}): …` plus per-fact lines), so unlike
/// [`TUI_TTY_REFUSAL_REASON`] the sentinel cannot be the whole reason —
/// it is the stable header prefix instead. Same slug (`invalid_input`),
/// same exit 2 (frozen taxonomy; only the hint differs) — the 06-07
/// TTY-refusal pattern at one removal. The contract_tags loss-gate
/// pins are the drift guard: if the prose header ever changes, the
/// hint silently regresses to the generic default and those pins fail.
pub const LOSS_GATE_REFUSAL_REASON_PREFIX: &str = "loss report (";

/// The TESTING bundle's route prefix (QUICK-p0g). The
/// [`CoreError::RoutesNotDeployed`] hint is content-addressed off it
/// — same third instance of the 06-07 pattern: the frozen taxonomy
/// keeps ONE absent-routes variant and one slug
/// (`routes_not_deployed`, exit 6), but the testing bundle installs
/// through a DIFFERENT command (`ign webdev deploy --with-testing` /
/// `ign adopt --project NAME --testing`), so the generic
/// `ign webdev deploy` hint would name a command that does not fix
/// it. Raise sites pass `route: "testing/run"`.
pub const TESTING_ROUTE_PREFIX: &str = "testing/";

/// The api-call catch-all's body cap (09-01): a gateway 4xx body rides
/// [`CoreError::GatewayClientError`] VERBATIM up to this many bytes; a
/// larger body is truncated at [`truncate_api_body`] with the explicit
/// [`GATEWAY_CLIENT_BODY_TRUNCATION_MARKER`]. 4 KiB is the plan-locked
/// cap — an unbounded passthrough would let a pathological gateway
/// page flood the agent's envelope.
pub const GATEWAY_CLIENT_BODY_CAP_BYTES: usize = 4096;

/// The truncation marker [`truncate_api_body`] appends when the api-call
/// body exceeds [`GATEWAY_CLIENT_BODY_CAP_BYTES`]. ASCII-pinned (no
/// multi-byte characters) so golden-file consumers never see an encoding
/// surprise at the cut.
pub const GATEWAY_CLIENT_BODY_TRUNCATION_MARKER: &str = "... [truncated]";

/// The ONE construction site for a capped api-call body: returns `body`
/// verbatim when it fits [`GATEWAY_CLIENT_BODY_CAP_BYTES`], otherwise its
/// first cap bytes (on a UTF-8 char boundary) plus
/// [`GATEWAY_CLIENT_BODY_TRUNCATION_MARKER`]. The variant always carries
/// its final form — construction cannot forget the cap.
pub fn truncate_api_body(body: &str) -> String {
    if body.len() <= GATEWAY_CLIENT_BODY_CAP_BYTES {
        return body.to_string();
    }
    let mut end = GATEWAY_CLIENT_BODY_CAP_BYTES;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = body[..end].to_string();
    truncated.push_str(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER);
    truncated
}

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

    /// JVM profiling invoked without `--i-accept-the-risk`. Exit 2
    /// (usage class — it names a flag the caller must add, like
    /// [`Self::ConfirmationRequired`]) but DELIBERATELY NOT satisfiable
    /// by the global `--yes`/`IGNITION_YES`: active JVM instrumentation
    /// is a stronger opt-in than the destructive-verb guard — the
    /// caller must acknowledge THIS specific production risk, not
    /// confirm generically.
    #[error(
        "{operation} actively instruments the gateway JVM; rerun with --i-accept-the-risk to acknowledge the production risk"
    )]
    ProfilingRiskNotAccepted { operation: String },

    /// An import byte source the caller must fix (wrong file, too big,
    /// unreadable). Exit 2 — usage class: it names what the CALLER must
    /// change, like [`Self::ConfirmationRequired`] (03-02).
    #[error("invalid import file: {reason}")]
    InvalidImportFile { reason: String },

    /// The gateway ANSWERED the import POST with HTTP 200 but the
    /// body says `{"success": false, "problem": "…"}` — the
    /// denial-rides-200 class the WebDev family handles (05-01)
    /// applied to the project-import family (05-07, UAT Gap 1):
    /// without this check the import caller reports ok while nothing
    /// landed. Exit 6 — target state: the gateway refused the
    /// import (the problem text names why, verbatim).
    #[error("gateway rejected the project import for {project:?}: {problem}")]
    ImportDenied {
        /// The project the import was headed for.
        project: String,
        /// The gateway's own `problem` text (verbatim when present).
        problem: String,
        /// URL of the import request, when known.
        endpoint: Option<String>,
    },

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

    /// The adopt bootstrap's login rung needs `IGNITION_PASSWORD` and it
    /// was absent/empty (ADOPT-05 review round: the generic
    /// [`Self::SecretUnavailable`] hint names the TOKEN paths, which is
    /// exactly wrong for the PASSWORD gate — same exit class, own slug,
    /// never a new exit code; the PollIntervalTooSmall precedent).
    #[error("login password unavailable for profile {profile:?}")]
    PasswordUnavailable { profile: String },

    /// Config file unreadable or wrong shape. Exit 3.
    #[error("invalid configuration: {reason}")]
    ConfigInvalid { reason: String },

    /// A profile's `poll_interval_secs` is below the 1-second floor
    /// (08-01, TUIX-05 clamp): sub-second gateway polling is refused at
    /// load time — the TUI's background refresh cadence may not hammer
    /// the gateway. Exit 3 — the CONFIG class (the Phase-7 additive-slug
    /// precedent, e.g. `eam_not_controller`: same exit class, own slug,
    /// never a new exit code — the 1–7 taxonomy is frozen).
    #[error("profile {profile:?}: poll_interval_secs must be >= 1 (sub-second polling refused)")]
    PollIntervalTooSmall {
        /// The profile carrying the refused value.
        profile: String,
    },

    /// Gateway unreachable / timeout / TLS failure. Exit 4.
    ///
    /// `source: None` marks a POLL deadline expiry (02-04 `poll.rs`):
    /// same class, same slug (`network_error`), no new variant — the
    /// transport-error `source` a real failure carries is simply
    /// absent, and `url` describes what was being waited on instead
    /// (the poll's subject). The deadline's last observation rides
    /// `observation` (09-07): when it is `Some` the gateway ANSWERED
    /// with a concrete state, and the message leads with "no terminal
    /// state" — it NEVER claims unreachability for an observed
    /// answer; `None` keeps the plain unreachability wording.
    #[error(
        "{lead} at {url}{source_note}{observation_note}",
        lead = if observation.is_some() {
            "no terminal state"
        } else {
            "gateway unreachable"
        },
        source_note = source.as_ref().map(|source| format!(": {source}")).unwrap_or_default(),
        observation_note = observation
            .as_deref()
            .map(|obs| format!("; last observation: {obs}"))
            .unwrap_or_default()
    )]
    Network {
        url: String,
        #[source]
        source: Option<reqwest::Error>,
        /// The poll deadline's last concrete observation (e.g. the
        /// gateway's reported state) — `None` for transport failures
        /// and observation-less deadlines. `Some` ⇒ the gateway
        /// answered; the Display never says "unreachable" then.
        observation: Option<String>,
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
    #[error(
        "no alarm journal profile is configured on this gateway — alarm history has nothing to read"
    )]
    AlarmJournalMissing {
        /// URL of the alarms route request, when known.
        endpoint: Option<String>,
    },

    /// Pruning a LIVE Designer session entry — the gateway's prune
    /// route answers 409 (empty body, wire-verified on 8.3.3,
    /// 06-UAT test 6): prune removes STALE entries only, so the
    /// command is invalid while the Designer is still open. Exit 6 —
    /// target state (additive slug in the frozen taxonomy's
    /// established growth pattern; classify()'s ROUTE-SCOPED 409 arm
    /// constructs this — the first classify-constructed refusal added
    /// since the 02-01 set).
    #[error("designer session {id} is live — the gateway refused the prune")]
    SessionNotPrunable {
        /// The Designer session id that was refused.
        id: String,
        /// URL of the refused DELETE, when known.
        endpoint: Option<String>,
    },

    /// Docker/compose rig failure. Exit 7. Reserved — first used in Phase 4;
    /// trivially constructible so the taxonomy enumerates completely today.
    #[error("rig error: {0}")]
    Rig(String),

    /// The gateway's EAM module is not configured as a controller —
    /// every `/data/eam/api/v1/*` runtime endpoint answers 403 with
    /// "This operation can only be performed when EAM is configured
    /// as a controller" on a stock gateway (live-proven 8.3.3,
    /// 07-RESEARCH): a STATE refusal, not auth — the token is fine,
    /// the module's role is not. Message-classified at the classify
    /// seam, path-scoped to `/data/eam/` so generic 403s elsewhere
    /// cannot shift (the trial_not_expired pattern, classify
    /// edition). Exit 6 — target state.
    #[error(
        "EAM is not configured as a controller on this gateway — every EAM runtime \
         operation refuses until the module's installMode is flipped"
    )]
    EamNotController {
        /// URL of the refused request, when known.
        endpoint: Option<String>,
    },

    /// A provider-ROOT tag path (`[default]` alone, or a bare first
    /// segment that resolves to a provider) met the tagConfig route
    /// — `system.tag.getConfiguration`/`exportTags` need an RPC
    /// context WebDev threads don't carry (live-proven 8.3.3
    /// b2026012009, both gateways; 07-UAT test 12). The route
    /// refuses honestly (pre-call bracket detection + RpcContext
    /// translation for the bare form) instead of surfacing the
    /// IllegalStateException as a generic route error; subtree
    /// paths (`[default]folder`) are the supported form. Exit 6 —
    /// target state: a platform limitation, not a bug.
    #[error(
        "provider-root tag paths are not supported by the deployed route (the gateway needs an \
         RPC context WebDev threads don't carry) — target a subtree like [provider]folder"
    )]
    ProviderRootUnsupported {
        /// URL of the tagConfig route request, when known.
        endpoint: Option<String>,
    },

    /// An `eam task new` whose type is in the REFUSED set —
    /// `eam_restoreBackup`, `eam_installModules`, `eam_remoteUpgrade`
    /// are fleet-destructive (they push backups/modules/upgrades to
    /// every AGENT target), and the CLI refuses them outright over
    /// guard-everything: honest refusal with the v2 scope pointer
    /// (the planner-locked create ladder's top rung). Exit 6 —
    /// target state (additive slug alongside Task 2's).
    #[error(
        "EAM task type {task_type} is fleet-destructive — refused (restore/install/upgrade \
         are EXT-03 (v2) scope; run them from the EAM console)"
    )]
    EamTaskTypeRefused {
        /// The refused `profile.type` token.
        task_type: String,
    },

    /// An `eam task force` whose slot a leftover run occupies — the
    /// force route answers 409 with the gateway's own Jetty page
    /// ("Task 'X (forced)' already exists! It must be completed or
    /// deleted before another task of this type can be force
    /// executed."; live-captured 8.3.3, 07-UAT test 7). Exit 6 —
    /// target state (the `session_not_prunable` precedent,
    /// force-route edition): the gateway's state refused the command,
    /// not a bug; classify()'s path-scoped 409 arm constructs this.
    #[error("EAM task {task} has a run in flight — the gateway refused the force: {detail}")]
    EamTaskInFlight {
        /// The forced task's name (the force URL's last segment).
        task: String,
        /// The gateway's Jetty page message verbatim when sniffed;
        /// the '(forced)' fallback text otherwise.
        detail: String,
        /// URL of the refused force POST, when known.
        endpoint: Option<String>,
    },

    /// The scriptExec route is not configured for this profile — no
    /// webdev secret is persisted, which can only mean `ign webdev
    /// deploy --with-script-exec` has never run (deploy persists the
    /// secret 0600 BEFORE upload, so a deployed route without a
    /// stored secret is not a reachable state). `ign script run`'s
    /// opt-in is STRUCTURAL — the deploy flag — so the verb carries
    /// no `--yes` guard and refuses here instead (07-03, SCRPT-01:
    /// the TrialNotExpired precedent — action-constructed, not
    /// classify). Exit 6 — target state.
    #[error(
        "scriptExec is not configured for profile {profile:?} — the secret-gated route deploys \
         only via the explicit opt-in"
    )]
    ScriptExecNotConfigured {
        /// The profile whose secret store is empty.
        profile: String,
    },

    /// `ign lint` found no `ignition-lint` executable on PATH — the
    /// delegation has nothing to delegate to (07-04, INTR-02). The
    /// hint carries the install command + repo. Exit 6 — target
    /// state (additive slug; the environment lacks the tool, the
    /// command is fine).
    #[error("ignition-lint is not installed (no executable found on PATH)")]
    LintToolAbsent,

    /// `ign e2e init` found no `node` executable on PATH — the scaffold
    /// it would write is unusable without one (QUICK-tg4).
    ///
    /// Exit 6, mirroring [`Self::LintToolAbsent`] line for line: this
    /// is the SAME shape — a local external tool a verb delegates to is
    /// absent, with an install hint. It deliberately does NOT join
    /// exit 7, whose sole slug is `rig_error` for docker/compose rig
    /// failures; putting a missing Node there would force a relabel of
    /// a locked exit class.
    #[error("{tool} is not installed (no executable found on PATH)")]
    NodeToolAbsent {
        /// The tool that was looked for (`node`).
        tool: String,
    },

    /// The gateway answered the api call with a 4xx this CLI does not
    /// curate — the caller's request is the problem, and the body is
    /// theirs to read. Exit 2 (usage class; additive slug — the
    /// poll_interval_too_small precedent: same class, own slug, no new
    /// exit code). Constructed ONLY by the api-call-scoped classify arm
    /// (09-01: the `api_call` parameter) — a curated command's 4xx keeps
    /// its existing classification (the catch-all cannot fire without
    /// the parameter). The body is the gateway's answer VERBATIM,
    /// truncated at [`GATEWAY_CLIENT_BODY_CAP_BYTES`] with the explicit
    /// [`GATEWAY_CLIENT_BODY_TRUNCATION_MARKER`] via [`truncate_api_body`]
    /// at construction.
    #[error("gateway rejected the api call (HTTP {status} from {endpoint}): {body}")]
    GatewayClientError {
        /// HTTP status the gateway answered with.
        status: u16,
        /// Full URL of the rejected request.
        endpoint: String,
        /// The response body VERBATIM, truncated at
        /// [`GATEWAY_CLIENT_BODY_CAP_BYTES`] with an explicit marker.
        body: String,
    },

    /// No diagnostics bundle is available — the status poll ANSWERED
    /// with a captured TERMINAL steady state meaning "no current
    /// bundle" (`Invalid`: live-proven on 8.3.6 rig ign-p9-836,
    /// 2026-09-07 UAT / 09-UAT.md Gap 3 — a `Valid` bundle decays to
    /// `Invalid` within ~2 minutes UNPROMPTED and stays `Invalid`;
    /// only a fresh generate changes it, polling cannot). Exit 6 —
    /// target state (the `ImportDenied` precedent: a gateway-answered
    /// refusal riding its own class, action-constructed by `bundle
    /// wait`'s probe, not classify).
    #[error(
        "no diagnostics bundle available (gateway reports state {state:?}) — run \
         `ign diagnostics bundle generate` first; polling cannot change this state"
    )]
    BundleNotAvailable {
        /// The observed steady state (the captured unavailable
        /// vocabulary, e.g. `Invalid`).
        state: String,
    },

    /// Transport failure reaching a third-party module release feed or
    /// its asset CDN — DNS/connect/timeout/TLS (Phase 15, D-06). Exit 4:
    /// same network class as [`Self::Network`], but a SEPARATE slug and
    /// wording — `Network`'s Display hard-codes "gateway unreachable",
    /// which is actively wrong here (`api.github.com` is not a gateway,
    /// and the user's own gateway may be perfectly reachable). This is
    /// the offline signal `fetch_and_verify` returns when the cache is
    /// empty (SC-4's negative half).
    #[error(
        "module release feed at {url} is unreachable{source_note}",
        source_note = source.as_ref().map(|source| format!(": {source}")).unwrap_or_default()
    )]
    ModuleFeedUnreachable {
        /// The feed or CDN URL that could not be reached.
        url: String,
        #[source]
        source: Option<reqwest::Error>,
    },

    /// No release exists for the pinned tag, or the release carries no
    /// asset of the expected name (Phase 15, SC-1's "unknown version"
    /// refusal). Exit 6 — target state: names BOTH the requested version
    /// and the resolved URL, and (for a missing asset) both the expected
    /// asset name and the names actually present, so the caller never
    /// has to guess which lookup failed or falls back to "latest".
    #[error("module {module:?} version {version:?} not found at {url}: {detail}")]
    ModuleReleaseNotFound {
        /// The [`crate::module::ModuleSpec::id`] being fetched.
        module: String,
        /// The requested version string.
        version: String,
        /// The tag-pinned releases URL that was queried.
        url: String,
        /// What specifically was missing (no release for the tag, or no
        /// matching asset — names the asset actually present).
        detail: String,
    },

    /// Downloaded bytes hash to something other than the release's
    /// published digest — SC-2's hard refusal (Phase 15; this variant is
    /// defined in 15-01, the branch is exercised starting 15-02). Exit 6
    /// — target state: the feed answered and served bytes, but what it
    /// served cannot be trusted. The bytes are discarded (never
    /// persisted to the cache) and BOTH digests are named.
    ///
    /// Payload is BOXED: five inline `String`s put `CoreError` at
    /// exactly clippy's 128-byte `result_large_err` threshold, which
    /// fails `-D warnings` in every crate returning `Result<_,
    /// CoreError>` — `ignition-tui` first. Error paths are cold, so one
    /// allocation on construction is the cheap side of that trade.
    #[error("{0}")]
    ModuleDigestMismatch(Box<ModuleDigestMismatchDetails>),

    /// The module release feed ANSWERED but not usably — rate-limited,
    /// unauthenticated 401/403, 5xx, an unparseable body, an asset with
    /// no published digest, or an insecure downgrade of the asset URL
    /// (Phase 15). Exit 6 — target state: distinct from
    /// [`Self::ModuleFeedUnreachable`] (exit 4 — transport never reached
    /// the feed at all); here the feed DID answer, just not with
    /// something `ign` can safely act on.
    #[error("module release feed at {url} is unusable: {detail}")]
    ModuleFeedUnusable {
        /// The feed or asset URL that answered unusably.
        url: String,
        /// What specifically made the answer unusable.
        detail: String,
    },

    /// A pinned version's upstream digest differs from what's already
    /// cached — D-02's default refusal on `FetchPolicy::Refresh` /
    /// `AcceptUpstreamChange` (Phase 15, 15-02): the feed answered
    /// fine, but what it currently publishes for this version no
    /// longer matches what was verified and cached before. Exit 6 —
    /// target state: names BOTH full digests and the path of the
    /// cached artifact, which is left on disk UNTOUCHED (no path in
    /// `module/` deletes, truncates, or overwrites a cache entry).
    /// `FetchPolicy::AcceptUpstreamChange` is the explicit, documented
    /// override.
    ///
    /// Payload is BOXED for the same reason as
    /// [`Self::ModuleDigestMismatch`] — see that variant.
    #[error("{0}")]
    ModuleDigestChanged(Box<ModuleDigestChangedDetails>),

    /// A declared module id has no matching registry entry (Phase 16,
    /// D-13). A well-formed id that simply isn't registered is a CONFIG
    /// problem, not malformed input (exit 2 belongs to
    /// `validate_module_id`/`validate_version`) and not a Docker
    /// failure (exit 7 is [`Self::Rig`]) — every OTHER module
    /// provisioning refusal (missing artifact, non-file artifact,
    /// relative path, underivable service) stays `CoreError::Rig`.
    /// Exit 3, structurally the [`Self::ProfileNotFound`] shape: names
    /// both the bad id and every id that IS registered.
    #[error("module {id:?} is not registered (known modules: {known:?})")]
    ModuleNotRegistered {
        /// The declared id with no matching [`crate::module::ModuleSpec`].
        id: String,
        /// Every currently registered [`crate::module::ModuleSpec::id`].
        known: Vec<String>,
    },

    /// The gateway ANSWERED `DELETE /data/api/v1/modules/uninstall` with
    /// HTTP 200 but the body says `{"success": false, "failedUninstalls":
    /// {"uninstall": […]}}` — the denial-rides-200 class
    /// ([`Self::ImportDenied`]'s precedent) applied to module uninstall
    /// (Phase 19, D-19-03): live-verified that a stock module, a bogus
    /// id, and a still-mounted module ALL answer 200 with
    /// `success:false`. An unparseable body, a missing `success`, or an
    /// absent `failedUninstalls` all map here too — absence of detail
    /// must never degrade into a reported success. Exit 6 — target
    /// state: names both the registry id and the gateway id that was
    /// sent, plus every failed id the gateway reported (or the sent id
    /// itself when the gateway gave no detail).
    #[error(
        "gateway refused to uninstall module {module_id:?} ({gateway_module_id}): {}",
        failed.join(", ")
    )]
    ModuleUninstallDenied {
        /// The registry slug (`git`) the caller named.
        module_id: String,
        /// The gateway's own id ([`crate::module::ModuleSpec::gateway_module_id`])
        /// sent in the request.
        gateway_module_id: String,
        /// `failedUninstalls.uninstall` verbatim, or `[gateway_module_id]`
        /// when the gateway's body carried no usable detail.
        failed: Vec<String>,
        /// URL of the refused DELETE, when known.
        endpoint: Option<String>,
    },

    /// Another actor holds the gateway's scan-lock. Exit 6 — target
    /// state. Constructed by the route-scoped scan-lock arm in
    /// classify.rs (20-03); `kind` is `config` or `projects`. `actor`
    /// is the holder's opaque identity string when the gateway named
    /// it (`api-token:<name>:<ip>`), echoed and never parsed; `None`
    /// when the refusal did not name one.
    #[error(
        "the {kind} scan-lock is held by {}",
        actor.as_deref().unwrap_or("another actor")
    )]
    ScanLockHeld {
        /// The holder's identity string, when the gateway named it.
        actor: Option<String>,
        /// Which scan-lock: `config` or `projects`.
        kind: String,
        /// URL of the refused request, when known.
        endpoint: Option<String>,
    },

    /// The gateway refused a resource write over its signature. Exit 6 —
    /// target state. Constructed ONLY by the deploy-scoped write helper
    /// in client/deploy_api.rs (20-04), never by classify.rs, so the
    /// v1.1 EAM 500 pin stays exit 1 `internal`. The payload
    /// deliberately carries no signature and no gateway problem text:
    /// 8.3.3 problem messages leak the live signature.
    #[error("gateway refused the write to resource {name:?}: {reason} signature")]
    ResourceSignatureRejected {
        /// The resource name that was being written.
        name: String,
        /// Why the signature was refused.
        reason: SignatureReason,
        /// URL of the refused request, when known.
        endpoint: Option<String>,
    },

    /// A resource create named a resource that already exists. Exit 6 —
    /// target state. Constructed ONLY by the deploy-scoped create helper
    /// in client/deploy_api.rs from the live 409 JSON `problem` body
    /// (20-LIVE-CAPTURES D10), never by classify.rs. The payload carries
    /// no gateway problem text or stacktrace.
    #[error("resource {name:?} already exists")]
    ResourceExists {
        /// The resource name that was being created.
        name: String,
        /// URL of the refused request, when known.
        endpoint: Option<String>,
    },

    /// A deployment-mode create named a mode that is already defined.
    /// Exit 6 — target state. Constructed ONLY by the deploy-scoped mode
    /// create in client/deploy_api.rs from the live 409 (D6).
    #[error("deployment mode {name:?} is already defined")]
    ModeExists {
        /// The mode name that was being created.
        name: String,
        /// URL of the refused request, when known.
        endpoint: Option<String>,
    },

    /// A declared gateway tree failed render validation (21-03): one or
    /// more findings, each naming its file and, for YAML syntax and
    /// schema defects, a line and column. Exit 2 — usage class: the
    /// CALLER authored the tree and must fix it. Render is offline, so
    /// there is no endpoint. Boxed to keep `CoreError` under clippy's
    /// `result_large_err` threshold.
    #[error("deploy tree invalid: {0}")]
    DeployTreeInvalid(Box<DeployTreeInvalidDetails>),

    /// A declared tree carries an Embedded or plaintext secret (21-03).
    /// Exit 2 — usage class. The payload holds JSON paths and other
    /// findings only: no field exists that could carry a secret value.
    #[error("refusing to render an embedded or plaintext secret: {0}")]
    EmbeddedSecret(Box<EmbeddedSecretDetails>),

    /// A deploy host transport could not read (or later write) a node's
    /// gateway data directory (22-01): a missing or unreadable directory, a
    /// hostile live tree (symlink, special file, non-UTF-8 name), a size cap
    /// breach or an ssh failure. Exit 4 — network class: the host could not
    /// be reached or read. A failed read is NEVER an empty result, so a plan
    /// cannot mistake an unreadable tree for deletions. `detail` names paths
    /// and causes only, never file content.
    #[error("transport failed ({operation}) on {node}: {detail}")]
    TransportFailed {
        /// What was being done, e.g. `read`.
        operation: String,
        /// The target node label, e.g. `nodes.master`.
        node: String,
        /// What went wrong; names paths and causes, never file content.
        detail: String,
    },

    /// The on-box declared-names ledger exists but cannot be trusted (22-03,
    /// D-07): unreadable, empty, over its size cap, not strict UTF-8 JSON, of
    /// another schema version, naming another gateway or mode, unsorted or
    /// duplicated, or listing a path plan must never delete. Exit 6 -
    /// target_state class: the on-box state is wrong, not the caller's input.
    /// Plan never falls back to an empty delete set. `problem` names fields
    /// and listed paths only, never other file content.
    #[error("declared-names ledger {location} is unusable: {problem}")]
    DeployLedgerInvalid {
        /// The ledger location relative to the node's data directory.
        location: String,
        /// What is wrong; names fields and listed paths, never file content.
        problem: String,
    },

    /// The plan the caller reviewed is no longer the plan (23-02, DAPL-02):
    /// `--plan-sha256` differs from the recomputed `plan_sha256`, or the
    /// on-box pre-image changed under the scan lock. Exit 6 - target_state
    /// class: the gateway moved since the review. Carries hashes only.
    #[error("plan changed since review: expected plan_sha256 {expected}, now {actual} ({detail})")]
    PlanStale {
        /// The `plan_sha256` the caller reviewed.
        expected: String,
        /// The `plan_sha256` of the plan as it is now.
        actual: String,
        /// Where the mismatch was found, e.g. `review hash`; no file content.
        detail: String,
    },

    /// A destructive or drifted plan was applied without its flag (23-02,
    /// DAPL-02). Exit 2 - usage class: the caller must pass
    /// `--allow-destructive` or `--allow-drift <reason>`. Carries unit keys
    /// and counts only.
    #[error("{}", destructive_change_refused_text(.gate, .keys, *.count))]
    DestructiveChangeRefused {
        /// Which gate refused: `destructive` or `drift`.
        gate: String,
        /// The first [`APPLY_LIST_SHOWN`] offending unit keys, byte-sorted.
        keys: Vec<String>,
        /// How many entries the gate refused in all.
        count: usize,
    },

    /// A removal that cannot take effect (23-02, DAPL-01): the unit's `core`
    /// copy shadows the mode collection, so deleting from the mode collection
    /// changes nothing. No flag permits it. Exit 6.
    #[error("{}", deploy_blocked_text(.keys))]
    DeployBlocked {
        /// The `shadows_removal` unit keys, byte-sorted.
        keys: Vec<String>,
    },

    /// An apply would write, check or delete a unit whose live copy the
    /// host's sftp server cannot read (quick-261029-ms3): a host path of 240
    /// characters or more, so the live copy is unknown and must never be
    /// taken as absent. Raised when an unreadable unit is declared or listed
    /// in the declared-names ledger. No flag permits it. Exit 6.
    #[error("{}", deploy_unreadable_text(.keys))]
    DeployUnreadable {
        /// The unreadable unit keys that are declared or in the ledger,
        /// byte-sorted.
        keys: Vec<String>,
    },

    /// One or more apply preflight checks failed (23-02, DAPL-04). Exit 6.
    /// Nothing was written to the gateway. The payload is BOXED so the large
    /// list stays off every `Result<_, CoreError>`.
    #[error("{}", deploy_preflight_failed_text(.failures))]
    DeployPreflightFailed {
        /// Every failed check, in the order the checks ran.
        failures: Box<[PreflightFailure]>,
    },

    /// An interrupted deploy left its journal on the box (23-02, D-09). Exit
    /// 6. The message, not the hint, carries the recovery command.
    #[error(
        "an interrupted deploy left the journal {location} for run {run_id}; \
         recover it with: ign deploy rollback --run {run_id} --env {env}"
    )]
    DeployJournalPresent {
        /// The interrupted run.
        run_id: String,
        /// The `--env` value the recovery command needs.
        env: String,
        /// The on-box journal location.
        location: String,
    },

    /// Live files changed after the run wrote them, so a rollback would
    /// overwrite someone's edit (23-02, DAPL-07). Exit 6. Carries paths and
    /// counts only.
    #[error("{}", deploy_live_changed_text(.run_id, .paths, *.count))]
    DeployLiveChanged {
        /// The run being rolled back.
        run_id: String,
        /// The first [`APPLY_LIST_SHOWN`] changed paths, byte-sorted.
        paths: Vec<String>,
        /// How many touched paths changed in all.
        count: usize,
    },

    /// A run's record or snapshot cannot be trusted (23-02, D-04). Exit 6.
    /// `problem` names fields and paths only, never file content.
    #[error("deploy run {run_id} is unusable: {problem}")]
    DeployRunInvalid {
        /// The run id asked for.
        run_id: String,
        /// What is wrong.
        problem: String,
    },

    /// A rollback could not restore the pre-apply files (23-02, D-12).
    /// Exit 6. Boxed to keep `CoreError` under clippy's `result_large_err`.
    #[error("{0}")]
    RollbackFailed(Box<RollbackFailure>),

    /// `deploy projects` refused a project the git module still manages
    /// (25-07, DPRJ-04, D-11): the project's `.git` entry exists on the
    /// gateway host. Exit 6 - target_state class: the gateway is not in a
    /// state this deploy may touch. The tool never deletes a `.git`; an
    /// administrator removes it per project. Carries the project name, the
    /// location below the data directory and the kind (`directory`, `file`,
    /// `symlink`, `other`) only.
    #[error(
        "project {project} is still managed by the git module: {location} is present ({kind}); an administrator removes it before this project is deployed"
    )]
    ProjectGitManaged {
        /// The gateway project name.
        project: String,
        /// The `.git` path below the node's data directory.
        location: String,
        /// What the entry is.
        kind: String,
    },

    /// `deploy projects` refused a project whose parent cannot be used
    /// (25-10, DPRJ-02, D-05, K6): the parent named in the repo's
    /// `project.json` is missing on the gateway, closes a cycle, or is not
    /// inheritable. Exit 6 - target_state class: the gateway is not in a
    /// state this deploy may build on. Carries names and the reason word
    /// (`missing`, `cycle`, `not_inheritable`) only. The parent is only read.
    #[error("project {project} cannot be deployed under parent {parent}: {reason}")]
    ProjectParentInvalid {
        /// The gateway project being deployed.
        project: String,
        /// The parent named in the repo's `project.json`.
        parent: String,
        /// `missing`, `cycle` or `not_inheritable`.
        reason: String,
    },

    /// `deploy projects` refused an artifact that is not the one that was
    /// reviewed (25-11, SC-1, D-01, V1): `--artifact-sha256` differs from the
    /// canonical digest of the artifact the checkout builds. Exit 6 -
    /// target_state class: the checkout is not in the state the reviewer saw.
    /// The comparison runs after the local build and before any host or
    /// gateway contact, so nothing was uploaded. Carries the project name and
    /// the two hashes only.
    #[error(
        "artifact hash mismatch for project {project}: expected {expected}, the checkout builds {actual}; nothing was uploaded"
    )]
    ProjectHashMismatch {
        /// The gateway project name.
        project: String,
        /// The `--artifact-sha256` the caller reviewed.
        expected: String,
        /// The canonical digest of the artifact this checkout builds.
        actual: String,
    },
}

/// Items listed inline by the Phase 23 refusal Displays; the rest collapse
/// into `and {m} more`.
pub const APPLY_LIST_SHOWN: usize = 10;

/// One failed apply preflight check (23-02). `detail` carries no value from
/// the gateway, the host or the repo that could be a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightFailure {
    /// The check id, e.g. `mode_master` or `run_dir_not_ignored`.
    pub check: String,
    /// What failed, value-free.
    pub detail: String,
}

/// Payload of [`CoreError::RollbackFailed`] (23-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackFailure {
    /// The run being restored.
    pub run_id: String,
    /// The `--env` value the retry command needs.
    pub env: String,
    /// The local run directory holding the snapshot, when one was made.
    pub run_dir: Option<String>,
    /// The on-box journal path, left in place for a manual recovery.
    pub journal: String,
    /// Why the restore failed; names paths and causes, never file content.
    pub cause: String,
}

impl RollbackFailure {
    /// The on-box directory holding the run's pre-images
    /// (`snapshots/<run>/`), next to the journal.
    pub fn snapshot_dir(&self) -> String {
        match self.journal.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/snapshots/{}/", self.run_id),
            None => format!("snapshots/{}/", self.run_id),
        }
    }
}

impl std::fmt::Display for RollbackFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The run directory's `snapshot/` is made only after a restore has
        // succeeded, so it is never offered here: the intact pre-images are
        // the on-box `snapshots/<run>/` directory and, when one was taken, the
        // `.gwbk`.
        write!(
            f,
            "rollback of run {run} failed ({cause}); the gateway files may be half restored. \
             Retry with: ign deploy rollback --run {run} --env {env} (the journal at {journal} \
             and the snapshot are still on the host). If the retry reports a lost or damaged \
             snapshot, restore the files from the .gwbk backup (when this run took one) or from \
             the on-box snapshot {snapshot}, then delete the journal by hand. \
             run directory: {run_dir}",
            run = self.run_id,
            cause = self.cause,
            env = self.env,
            journal = self.journal,
            snapshot = self.snapshot_dir(),
            run_dir = self.run_dir.as_deref().unwrap_or("none was created"),
        )
    }
}

/// `a, b, c and {m} more`: `shown` capped at [`APPLY_LIST_SHOWN`], then the
/// remainder of `total`.
fn listed(shown: &[String], total: usize) -> String {
    let take = shown.len().min(APPLY_LIST_SHOWN);
    let mut out = shown[..take].join(", ");
    if total > take {
        out.push_str(&format!(" and {} more", total - take));
    }
    out
}

fn destructive_change_refused_text(gate: &str, keys: &[String], count: usize) -> String {
    let (flag, verb) = match gate {
        "drift" => ("--allow-drift <reason>", "apply"),
        "downgrade" => ("--allow-downgrade", "deploy projects"),
        "first_deploy" => ("--allow-first-change <reason>", "deploy projects"),
        _ => ("--allow-destructive", "apply"),
    };
    format!(
        "{gate} change refused for {count} entr{}: {}; {verb} needs {flag}",
        if count == 1 { "y" } else { "ies" },
        listed(keys, count)
    )
}

fn deploy_blocked_text(keys: &[String]) -> String {
    format!(
        "deploy blocked: {} unit(s) are removed from the declared tree but a core copy \
         shadows them, so deleting them from the mode collection cannot take effect: {}",
        keys.len(),
        listed(keys, keys.len())
    )
}

fn deploy_unreadable_text(keys: &[String]) -> String {
    format!(
        "deploy refused: {} unit(s) the deploy would write or delete have a live path the \
         host's sftp server cannot read, so their live copy is unknown: {}; nothing was written",
        keys.len(),
        listed(keys, keys.len())
    )
}

fn deploy_preflight_failed_text(failures: &[PreflightFailure]) -> String {
    let mut out = format!("deploy preflight failed ({} check(s)):", failures.len());
    for failure in failures {
        out.push_str(&format!("\n  {}: {}", failure.check, failure.detail));
    }
    out
}

fn deploy_live_changed_text(run_id: &str, paths: &[String], count: usize) -> String {
    format!(
        "live files changed since run {run_id} wrote them ({count}): {}; restoring the \
         snapshot would overwrite them",
        listed(paths, count)
    )
}

/// Findings shown inline by the deploy error Displays; the rest collapse
/// into `and {m} more`.
pub const DEPLOY_FINDINGS_SHOWN: usize = 20;

/// One defect in a declared deploy tree (21-03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployFinding {
    /// Collection-relative file the finding is about.
    pub file: String,
    /// 1-based line, when the defect has a source position.
    pub line: Option<usize>,
    /// 1-based column, when the defect has a source position.
    pub column: Option<usize>,
    /// What is wrong. Never contains a value read from the tree.
    pub message: String,
}

impl std::fmt::Display for DeployFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                write!(f, "{}:{line}:{column}: {}", self.file, self.message)
            }
            _ => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

/// Payload of [`CoreError::DeployTreeInvalid`], boxed to keep
/// `CoreError` under clippy's `result_large_err` threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployTreeInvalidDetails {
    /// Every finding, in discovery order.
    pub findings: Vec<DeployFinding>,
}

impl std::fmt::Display for DeployTreeInvalidDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let [only] = self.findings.as_slice() {
            return write!(f, "{only}");
        }
        write!(f, "{} findings: ", self.findings.len())?;
        write_findings(f, &self.findings, DEPLOY_FINDINGS_SHOWN)
    }
}

/// Write `findings` joined by `; `, at most `shown` of them, then
/// `; and {m} more` when truncated.
fn write_findings(
    f: &mut std::fmt::Formatter<'_>,
    findings: &[DeployFinding],
    shown: usize,
) -> std::fmt::Result {
    for (i, finding) in findings.iter().take(shown).enumerate() {
        if i > 0 {
            f.write_str("; ")?;
        }
        write!(f, "{finding}")?;
    }
    if findings.len() > shown {
        write!(f, "; and {} more", findings.len() - shown)?;
    }
    Ok(())
}

/// Payload of [`CoreError::EmbeddedSecret`], boxed to keep `CoreError`
/// under clippy's `result_large_err` threshold. Carries locations only:
/// there is deliberately no field that can hold a secret value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedSecretDetails {
    /// Each entry is `<collection-relative file>: <dotted json path>`.
    pub paths: Vec<String>,
    /// Non-secret tree findings discovered in the same render.
    pub other_findings: Vec<DeployFinding>,
}

impl std::fmt::Display for EmbeddedSecretDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "embedded secret at {} path(s): ", self.paths.len())?;
        let shown_paths = self.paths.len().min(DEPLOY_FINDINGS_SHOWN);
        f.write_str(&self.paths[..shown_paths].join("; "))?;
        if self.paths.len() > shown_paths {
            write!(f, "; and {} more", self.paths.len() - shown_paths)?;
        }
        if !self.other_findings.is_empty() {
            write!(
                f,
                "; also {} other tree finding(s): ",
                self.other_findings.len()
            )?;
            // The combined list stays within DEPLOY_FINDINGS_SHOWN entries.
            let room = DEPLOY_FINDINGS_SHOWN.saturating_sub(shown_paths);
            write_findings(f, &self.other_findings, room)?;
        }
        Ok(())
    }
}

/// Why a resource write's signature was refused
/// ([`CoreError::ResourceSignatureRejected`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureReason {
    /// The gateway refused a signature that no longer matches: the
    /// resource changed since it was read.
    Stale,
    /// No usable signature was supplied: either refused before any
    /// request because it was empty, or a gateway refusal that the live
    /// capture shows is distinguishable.
    Missing,
}

impl std::fmt::Display for SignatureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stale => "stale",
            Self::Missing => "missing",
        })
    }
}

/// Payload of [`CoreError::ModuleDigestMismatch`], boxed to keep
/// `CoreError` under clippy's `result_large_err` threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDigestMismatchDetails {
    /// The [`crate::module::ModuleSpec::id`] being fetched.
    pub module: String,
    /// The requested version string.
    pub version: String,
    /// The asset URL the mismatched bytes were downloaded from.
    pub url: String,
    /// The release's published sha256 digest.
    pub expected: String,
    /// The sha256 digest of the bytes actually downloaded.
    pub actual: String,
}

/// Payload of [`CoreError::ModuleDigestChanged`], boxed to keep
/// `CoreError` under clippy's `result_large_err` threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDigestChangedDetails {
    /// The [`crate::module::ModuleSpec::id`] being fetched.
    pub module: String,
    /// The requested version string.
    pub version: String,
    /// The digest of the artifact already cached for this version.
    pub cached_digest: String,
    /// The digest the feed currently publishes for this version.
    pub upstream_digest: String,
    /// Path of the untouched, still-usable cached artifact.
    pub cached_path: String,
}

impl std::fmt::Display for ModuleDigestMismatchDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "module {:?} version {:?} digest mismatch at {}: expected {}, got {}",
            self.module, self.version, self.url, self.expected, self.actual
        )
    }
}

impl std::fmt::Display for ModuleDigestChangedDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "module {:?} version {:?} digest changed upstream: cached {}, \
             upstream now reports {} (cached artifact kept at {})",
            self.module, self.version, self.cached_digest, self.upstream_digest, self.cached_path
        )
    }
}

impl CoreError {
    /// Stable machine slug — public contract, never respell.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Internal(_) => "internal",
            Self::ConfirmationRequired { .. } => "confirmation_required",
            Self::ProfilingRiskNotAccepted { .. } => "profiling_risk_not_accepted",
            Self::InvalidImportFile { .. } => "invalid_import_file",
            Self::ImportDenied { .. } => "import_denied",
            Self::InvalidInput { .. } => "invalid_input",
            Self::ProfileNotFound { .. } => "profile_not_found",
            Self::NoActiveProfile => "no_active_profile",
            Self::SecretUnavailable { .. } => "secret_unavailable",
            Self::PasswordUnavailable { .. } => "password_unavailable",
            Self::ConfigInvalid { .. } => "config_invalid",
            Self::PollIntervalTooSmall { .. } => "poll_interval_too_small",
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
            Self::SessionNotPrunable { .. } => "session_not_prunable",
            Self::Rig(_) => "rig_error",
            Self::EamNotController { .. } => "eam_not_controller",
            Self::ProviderRootUnsupported { .. } => "provider_root_unsupported",
            Self::EamTaskTypeRefused { .. } => "eam_task_type_refused",
            Self::EamTaskInFlight { .. } => "eam_task_in_flight",
            Self::ScriptExecNotConfigured { .. } => "script_exec_not_configured",
            Self::LintToolAbsent => "lint_tool_absent",
            Self::NodeToolAbsent { .. } => "node_tool_absent",
            Self::GatewayClientError { .. } => "gateway_client_error",
            Self::BundleNotAvailable { .. } => "bundle_not_available",
            Self::ModuleFeedUnreachable { .. } => "module_feed_unreachable",
            Self::ModuleReleaseNotFound { .. } => "module_release_not_found",
            Self::ModuleDigestMismatch(..) => "module_digest_mismatch",
            Self::ModuleFeedUnusable { .. } => "module_feed_unusable",
            Self::ModuleDigestChanged(..) => "module_digest_changed",
            Self::ModuleNotRegistered { .. } => "module_not_registered",
            Self::ModuleUninstallDenied { .. } => "module_uninstall_denied",
            Self::ScanLockHeld { .. } => "scan_lock_held",
            Self::ResourceSignatureRejected { .. } => "resource_signature_rejected",
            Self::ResourceExists { .. } => "resource_exists",
            Self::ModeExists { .. } => "mode_exists",
            Self::DeployTreeInvalid(..) => "deploy_tree_invalid",
            Self::EmbeddedSecret(..) => "embedded_secret",
            Self::TransportFailed { .. } => "transport_failed",
            Self::DeployLedgerInvalid { .. } => "deploy_ledger_invalid",
            Self::PlanStale { .. } => "plan_stale",
            Self::DestructiveChangeRefused { .. } => "destructive_change_refused",
            Self::DeployBlocked { .. } => "deploy_blocked",
            Self::DeployUnreadable { .. } => "deploy_unreadable",
            Self::DeployPreflightFailed { .. } => "deploy_preflight_failed",
            Self::DeployJournalPresent { .. } => "deploy_journal_present",
            Self::DeployLiveChanged { .. } => "deploy_live_changed",
            Self::DeployRunInvalid { .. } => "deploy_run_invalid",
            Self::RollbackFailed(..) => "rollback_failed",
            Self::ProjectGitManaged { .. } => "project_git_managed",
            Self::ProjectParentInvalid { .. } => "project_parent_invalid",
            Self::ProjectHashMismatch { .. } => "project_hash_mismatch",
        }
    }

    /// The LOCKED exit-code mapping — the only place exit codes are decided.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Internal(_) => 1,
            Self::ConfirmationRequired { .. }
            | Self::ProfilingRiskNotAccepted { .. }
            | Self::InvalidImportFile { .. }
            | Self::InvalidInput { .. }
            | Self::GatewayClientError { .. }
            | Self::DeployTreeInvalid(..)
            | Self::EmbeddedSecret(..)
            | Self::DestructiveChangeRefused { .. } => 2,
            Self::ProfileNotFound { .. }
            | Self::NoActiveProfile
            | Self::SecretUnavailable { .. }
            | Self::PasswordUnavailable { .. }
            | Self::ConfigInvalid { .. }
            | Self::PollIntervalTooSmall { .. }
            | Self::ModuleNotRegistered { .. } => 3,
            Self::Network { .. }
            | Self::ModuleFeedUnreachable { .. }
            | Self::TransportFailed { .. } => 4,
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
            | Self::AlarmJournalMissing { .. }
            | Self::SessionNotPrunable { .. }
            | Self::ImportDenied { .. }
            | Self::EamNotController { .. }
            | Self::ProviderRootUnsupported { .. }
            | Self::EamTaskTypeRefused { .. }
            | Self::EamTaskInFlight { .. }
            | Self::ScriptExecNotConfigured { .. }
            | Self::LintToolAbsent
            | Self::NodeToolAbsent { .. }
            | Self::BundleNotAvailable { .. }
            | Self::ModuleReleaseNotFound { .. }
            | Self::ModuleDigestMismatch(..)
            | Self::ModuleFeedUnusable { .. }
            | Self::ModuleDigestChanged(..)
            | Self::ModuleUninstallDenied { .. }
            | Self::ScanLockHeld { .. }
            | Self::ResourceSignatureRejected { .. }
            | Self::ResourceExists { .. }
            | Self::ModeExists { .. }
            | Self::DeployLedgerInvalid { .. }
            | Self::PlanStale { .. }
            | Self::DeployBlocked { .. }
            | Self::DeployUnreadable { .. }
            | Self::DeployPreflightFailed { .. }
            | Self::DeployJournalPresent { .. }
            | Self::DeployLiveChanged { .. }
            | Self::DeployRunInvalid { .. }
            | Self::RollbackFailed(..)
            | Self::ProjectGitManaged { .. }
            | Self::ProjectParentInvalid { .. }
            | Self::ProjectHashMismatch { .. } => 6,
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
            Self::ProfilingRiskNotAccepted { .. } => Some(
                "JVM profiling ACTIVELY instruments the gateway: stack sampling \
                   adds CPU overhead proportional to thread count x frequency, \
                   and flight recordings buffer in JVM memory and write to \
                   gateway disk. On a PRODUCTION server this belongs inside an \
                   approved change window with a rollback plan — never as a \
                   casual probe. --yes deliberately does NOT satisfy this gate; \
                   re-run with --i-accept-the-risk after confirming that"
                    .to_string(),
            ),
            Self::InvalidImportFile { .. } => Some(
                "import expects a project-export ZIP (PK\\x03\\x04 magic) of at \
                   most 512 MB — pass a file produced by `ign project export` \
                   via --file (or `-` to pipe one on stdin)"
                    .to_string(),
            ),
            Self::ImportDenied { problem, .. } => Some(format!(
                "the gateway refused the import over a 200 answer — the problem \
                   text above is the gateway's own; `ign project export` of the \
                   current state is the honest baseline for hand-editing ({problem})"
            )),
            Self::InvalidInput { reason } => Some(
                if reason == TUI_TTY_REFUSAL_REASON {
                    // The ONE contextual InvalidInput hint (06-07): the
                    // TTY refusal's fix is terminal-related — the
                    // --file/stdin default is meaningless for a pipe.
                    "run `ign tui` in an interactive terminal (the cockpit \
                     needs a TTY on stdout — not a pipe or redirect)"
                } else if reason == TOP_TTY_REFUSAL_REASON {
                    // itop's sibling refusal: same terminal-contextual
                    // fix, naming the MONITOR's command (a hint naming
                    // `ign tui` would not fix an `ign top` run).
                    "run `ign top` in an interactive terminal (the monitor \
                     needs a TTY on stdout — not a pipe or redirect)"
                } else if reason.starts_with(LOSS_GATE_REFUSAL_REASON_PREFIX) {
                    // The loss-gate refusal (11-07): the message above already names
                    // every finding and ends with the actionable guidance — the hint
                    // restates it for envelope readers instead of the file-read
                    // default, which is meaningless here (the file WAS readable).
                    "the loss report above names what this import would drop or \
                     coerce — re-run with --yes to import anyway"
                } else if reason.starts_with("--plan-sha256 ")
                    || reason.starts_with("--allow-drift ")
                    || reason.starts_with("--artifact-sha256 ")
                    || reason.starts_with("--release ")
                    || reason.starts_with("--allow-first-change ")
                    || reason.starts_with("deploy projects needs --artifact-sha256")
                {
                    // The deploy apply and deploy projects gate flags (23-02,
                    // 25-11): the --file/stdin default is meaningless for a
                    // flag value.
                    "correct the flag value named above and rerun; no host or gateway \
                     was contacted"
                } else {
                    "fix the input source — a readable file path via --file, or `-` \
                     to pipe the content on stdin"
                }
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
            Self::PasswordUnavailable { .. } => Some(
                "ign adopt logs in with the gateway admin password — export \
                 IGNITION_PASSWORD (env-only, never a flag)"
                    .to_string(),
            ),
            Self::ConfigInvalid { .. } => Some(
                "verify the config file is valid TOML with [profiles.NAME] \
                 tables; `ign profile add` writes a known-good one"
                    .to_string(),
            ),
            Self::PollIntervalTooSmall { profile } => Some(format!(
                "set poll_interval_secs to 1 or higher in [profiles.{profile}], \
                 or remove the key to use the default cadence"
            )),
            Self::Network {
                url,
                observation,
                ..
            } => Some(match observation {
                Some(observation) => format!(
                    "the gateway answered but no terminal state arrived before the deadline — \
                     the last observation was: {observation}; address that state, not the \
                     connection ({url})"
                ),
                None => format!("check the gateway is reachable at {url} (host, port, VPN, TLS)"),
            }),
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
            Self::RoutesNotDeployed { route, .. } => Some(
                // CONTENT-ADDRESSED off the route (the
                // TUI_TTY_REFUSAL_REASON / loss-gate prose-prefix
                // pattern, same file): the testing bundle is a
                // SEPARATE deploy with its own flag, so the generic
                // `ign webdev deploy` hint would send the caller to a
                // command that does not install it. No new variant,
                // no new slug — the exit tables are untouched.
                if route.starts_with(TESTING_ROUTE_PREFIX) {
                    "run `ign webdev deploy --with-testing` to install the gateway \
                      testing bundle into the project (or bootstrap it in one shot \
                      with `ign adopt --project NAME --testing`), then retry"
                } else {
                    "run `ign webdev deploy` to install the CLI's WebDev routes into \
                      the gateway, then retry"
                }
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
            Self::SessionNotPrunable { .. } => Some(
                "close the Designer first — prune removes stale entries only".to_string(),
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
            Self::EamNotController { .. } => Some(
                "flip the gateway's EAM role: config-resource PUT on \
                   com.inductiveautomation.eam/module-settings with \
                   installMode \"Controller\" (array body carrying the current \
                   signature) — a manual gateway-role decision this CLI \
                   deliberately does not automate; see the README 'EAM tasks' \
                   section"
                    .to_string(),
            ),
            Self::ProviderRootUnsupported { .. } => Some(
                "target a subtree path like [provider]folder — provider-ROOT \
                   forms ([default] alone, or a bare provider name) need an \
                   RPC context WebDev threads don't carry (8.3.3); subtree \
                   paths are the supported form"
                    .to_string(),
            ),
            Self::EamTaskTypeRefused { .. } => Some(
                "restore/install/upgrade tasks dispatch fleet-wide (every \
                   agent target); run them from the Ignition EAM console — \
                   EXT-03 (v2) will scope them into the CLI"
                    .to_string(),
            ),
            Self::EamTaskInFlight { .. } => Some(
                "complete or delete the leftover '(forced)' run from the EAM \
                   console — no ign verb deletes runs; the slot frees once the \
                   run is resolved"
                    .to_string(),
            ),
            Self::ScriptExecNotConfigured { .. } => Some(
                "run `ign webdev deploy --with-script-exec` to deploy the route and \
                  generate + persist its secret (the deploy flag IS the opt-in — \
                  `ign script run` has no --yes by design)"
                    .to_string(),
            ),
            Self::LintToolAbsent => Some(
                "install the linter: `uv tool install ignition-lint-toolkit` \
                 (or `pip install ignition-lint-toolkit`) — \
                 github.com/TheThoughtagen/ignition-lint; then re-run with \
                 ignition-lint on PATH"
                    .to_string(),
            ),
            // The SAME string `ign e2e doctor`'s node row hands out —
            // a user who hits this refusal and then runs the doctor
            // must not be given two wordings of one instruction.
            Self::NodeToolAbsent { .. } => {
                Some(crate::actions::e2e::NODE_INSTALL_HINT.to_string())
            }
            Self::GatewayClientError { .. } => Some(
                "the gateway rejected this request — the body above is the \
                 gateway's own answer; fix the path/method/body, or use a \
                 curated `ign` command when one exists"
                    .to_string(),
            ),
            Self::BundleNotAvailable { .. } => Some(
                "generate a fresh bundle with `ign diagnostics bundle generate`, \
                 then wait again — the gateway reports no current bundle and \
                 polling cannot produce one"
                    .to_string(),
            ),
            Self::ModuleFeedUnreachable { .. } => Some(
                "check network reachability to the module release feed — an \
                 already-cached version works fully offline, so this only \
                 blocks a first-time fetch of a new version"
                    .to_string(),
            ),
            Self::ModuleReleaseNotFound {
                module, version, ..
            } => Some(format!(
                "check that {module} version {version} exists as a release upstream \
                 (the tag and asset name must match exactly) — this CLI never falls \
                 back to \"latest\""
            )),
            Self::ModuleDigestMismatch(d) => Some(format!(
                "the downloaded bytes for {} version {} do not match \
                 the feed's published digest — do not trust this artifact; nothing \
                 was cached, so re-running the fetch tries again from scratch",
                d.module,
                d.version
            )),
            Self::ModuleFeedUnusable { .. } => Some(
                "the module release feed answered but was not usable — if the \
                 detail names a rate limit, wait and retry; otherwise the feed's \
                 response itself needs investigation"
                    .to_string(),
            ),
            Self::ModuleDigestChanged(d) => Some(format!(
                "upstream re-released this version with different bytes — the cached \
                 artifact at {} is still there and usable offline; re-run with \
                 FetchPolicy::AcceptUpstreamChange to deliberately accept and verify the \
                 new bytes",
                d.cached_path
            )),
            Self::Rig(_) => Some(
                "check Docker is running and inspect the rig containers \
                 (docker ps)"
                    .to_string(),
            ),
            Self::ModuleNotRegistered { known, .. } => Some(if known.is_empty() {
                "no modules are registered in this build of ign".to_string()
            } else {
                format!(
                    "known module ids: {} — check the id passed to --with-module \
                     or declared under [rigs.NAME.modules.*]",
                    known.join(", ")
                )
            }),
            Self::ModuleUninstallDenied { .. } => Some(
                "the gateway refuses to uninstall a module whose .modl is still \
                 mounted — remove it from [rigs.NAME.modules], run `ign rig up` \
                 so the mount goes, then retry; a successful uninstall is NOT \
                 undone by re-declaring the module — recovery today means \
                 `ign rig reset --yes`, which destroys the rig's data volume"
                    .to_string(),
            ),
            Self::ScanLockHeld { .. } => Some(
                "the scan-lock is released when the holder's scan runs or its \
                 holdTimeout expires — wait for that, then retry"
                    .to_string(),
            ),
            Self::ResourceSignatureRejected { .. } => Some(
                "re-read the resource for its current signature and retry — \
                 signatures change on every write"
                    .to_string(),
            ),
            Self::ResourceExists { .. } => Some(
                "a resource of that name is already present — read it for its \
                 current signature and update it, or choose another name"
                    .to_string(),
            ),
            Self::ModeExists { .. } => Some(
                "that deployment mode is already defined — list the modes and \
                 skip the create"
                    .to_string(),
            ),
            Self::DeployTreeInvalid(..) => Some(
                "fix each file named above in the declared gateway tree; render \
                 refuses until every finding is resolved"
                    .to_string(),
            ),
            Self::EmbeddedSecret(..) => Some(
                "store the value in the gateway's secret provider and reference it \
                 as {\"type\":\"Referenced\",\"data\":{\"providerName\":\"…\",\
                 \"secretName\":\"…\"}}; never commit an Embedded or plaintext secret"
                    .to_string(),
            ),
            Self::TransportFailed { .. } => Some(
                "check the node's host access in targets/<env>.yaml: for `local`, that \
                 data_dir exists and is readable; for `ssh`, the pinned host key, the \
                 key and reachability. Nothing was read as absent, so no plan was made"
                    .to_string(),
            ),
            Self::DeployLedgerInvalid { .. } => Some(
                "the delete set is never assumed empty, so no plan was made; inspect or \
                 restore the ledger file named above (only an apply writes it) and rerun"
                    .to_string(),
            ),
            Self::PlanStale { detail, .. } if detail.starts_with("live project") => Some(
                "the live project changed after the plan was drawn; run the check again, \
                 review what it reports and rerun the deploy, nothing was imported"
                    .to_string(),
            ),
            Self::PlanStale { .. } => Some(
                "the plan changed after it was reviewed; review the new plan and rerun \
                 with its plan_sha256, nothing was written"
                    .to_string(),
            ),
            Self::DestructiveChangeRefused { gate, .. } => Some(match gate.as_str() {
                "drift" => "live files differ from both the base render and the declared tree; \
                     review the drift, then rerun with --allow-drift and a reason of at most \
                     500 characters"
                    .to_string(),
                "downgrade" => "the gateway last received a newer release of this project; \
                     deploy the newer release, or rerun with --allow-downgrade when the older \
                     release must replace it"
                    .to_string(),
                "first_deploy" => "this is the first deploy of the project by this tool and it \
                     would change what the gateway runs; release what the gateway runs first, \
                     or rerun with --allow-first-change and a reason"
                    .to_string(),
                _ => "review the destructive entries named above, then rerun with \
                     --allow-destructive and the plan_sha256 you reviewed"
                    .to_string(),
            }),
            Self::DeployBlocked { .. } => Some(
                "a core copy still shadows each unit named above, so no flag permits the \
                 removal; keep the unit in the declared tree, or remove the core copy on \
                 the gateway first"
                    .to_string(),
            ),
            Self::DeployUnreadable { .. } => Some(
                "the live path of each unit named above is too long for the host's sftp \
                 server; shorten it on the gateway (rename the folder), or, when the gateway \
                 owns the provider, list the provider in exclude_tag_providers and stop \
                 declaring it; nothing was written"
                    .to_string(),
            ),
            Self::DeployPreflightFailed { .. } => Some(
                "fix each failed check named above and rerun; no gateway file was written"
                    .to_string(),
            ),
            Self::DeployJournalPresent { .. } => Some(
                "an earlier deploy was interrupted and its journal is still on the box; run \
                 the recovery command in the message, then rerun; plan and apply refuse \
                 until the journal is cleared"
                    .to_string(),
            ),
            Self::DeployLiveChanged { .. } => Some(
                "someone changed the files named above after the run wrote them; review \
                 those changes, then rerun the rollback with --discard-live-changes to \
                 restore the snapshot over them"
                    .to_string(),
            ),
            Self::DeployRunInvalid { .. } => Some(
                "the run directory or the on-box record is missing or damaged; restore the \
                 files from the .gwbk backup or the run-directory snapshot instead"
                    .to_string(),
            ),
            Self::RollbackFailed(..) => Some(
                "do not apply again until the files are restored: run the retry command in \
                 the message first, because the journal and the snapshot are usually intact; \
                 only if it reports a lost or damaged snapshot restore from the .gwbk backup \
                 or the on-box snapshot named there, and then delete the journal at the path \
                 in the message by hand"
                    .to_string(),
            ),
            Self::ProjectGitManaged { .. } => Some(
                "follow the README cutover runbook: remove this project's .git on the \
                 gateway (the deploy tool never deletes it), then run the check again"
                    .to_string(),
            ),
            Self::ProjectParentInvalid { reason, .. } => Some(
                match reason.as_str() {
                    "missing" => "deploy the parent project from its own repo first, then run \
                                  the check again",
                    "cycle" => "fix the parent in project.json so the inheritance has no cycle",
                    _ => "the parent is not inheritable; fix the parent in project.json or \
                          make the parent inheritable",
                }
                .to_string(),
            ),
            Self::ProjectHashMismatch { .. } => Some(
                "the checkout is not the reviewed one; rerun the check on this commit and \
                 pass its artifact_sha256"
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
            | Self::AlarmJournalMissing { endpoint }
            | Self::SessionNotPrunable { endpoint, .. }
            | Self::ImportDenied { endpoint, .. }
            | Self::EamNotController { endpoint }
            | Self::ProviderRootUnsupported { endpoint }
            | Self::EamTaskInFlight { endpoint, .. }
            | Self::ModuleUninstallDenied { endpoint, .. }
            | Self::ScanLockHeld { endpoint, .. }
            | Self::ResourceSignatureRejected { endpoint, .. }
            | Self::ResourceExists { endpoint, .. }
            | Self::ModeExists { endpoint, .. } => endpoint.clone(),
            Self::ModuleFeedUnreachable { url, .. }
            | Self::ModuleReleaseNotFound { url, .. }
            | Self::ModuleFeedUnusable { url, .. } => Some(url.clone()),
            // Boxed payload, so it cannot share the or-pattern above.
            Self::ModuleDigestMismatch(d) => Some(d.url.clone()),
            _ => None,
        }
    }

    /// The deploy slugs (21-03), in the order the README row lists them.
    pub const DEPLOY_SLUGS: &'static [&'static str] = &[
        "deploy_tree_invalid",
        "embedded_secret",
        "transport_failed",
        "deploy_ledger_invalid",
        "plan_stale",
        "destructive_change_refused",
        "deploy_blocked",
        "deploy_preflight_failed",
        "deploy_journal_present",
        "deploy_live_changed",
        "deploy_run_invalid",
        "rollback_failed",
        "project_git_managed",
        "project_parent_invalid",
        "project_hash_mismatch",
        "deploy_unreadable",
    ];

    /// A single positioned (or file-only, when `line`/`column` are
    /// `None`) tree finding as a [`Self::DeployTreeInvalid`].
    pub fn deploy_tree_invalid(
        file: impl Into<String>,
        line: Option<usize>,
        column: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self::deploy_findings(vec![DeployFinding {
            file: file.into(),
            line,
            column,
            message: message.into(),
        }])
    }

    /// Several tree findings as one [`Self::DeployTreeInvalid`]. An empty
    /// vector is a programming error: debug builds assert, release builds
    /// still return one generic finding so the error is never empty.
    pub fn deploy_findings(findings: Vec<DeployFinding>) -> Self {
        debug_assert!(!findings.is_empty(), "deploy_findings needs a finding");
        let findings = if findings.is_empty() {
            vec![DeployFinding {
                file: String::new(),
                line: None,
                column: None,
                message: "the declared tree is invalid".to_string(),
            }]
        } else {
            findings
        };
        Self::DeployTreeInvalid(Box::new(DeployTreeInvalidDetails { findings }))
    }

    /// An embedded or plaintext secret at each of `paths` (locations
    /// only, never values), plus any non-secret findings from the same
    /// render.
    pub fn embedded_secret(paths: Vec<String>, other_findings: Vec<DeployFinding>) -> Self {
        Self::EmbeddedSecret(Box::new(EmbeddedSecretDetails {
            paths,
            other_findings,
        }))
    }

    /// The `ign tui` TTY refusal (06-07): InvalidInput carrying the
    /// ONE reason whose hint is terminal-contextual — see
    /// [`TUI_TTY_REFUSAL_REASON`]. Same slug, same exit 2 (frozen
    /// taxonomy; only the hint differs).
    pub fn tui_tty_refusal() -> Self {
        Self::InvalidInput {
            reason: TUI_TTY_REFUSAL_REASON.to_string(),
        }
    }

    /// The `ign top` TTY refusal (itop): the tui refusal's sibling at
    /// one command over — same slug, same exit 2, the hint naming the
    /// monitor's command. See [`TOP_TTY_REFUSAL_REASON`].
    pub fn top_tty_refusal() -> Self {
        Self::InvalidInput {
            reason: TOP_TTY_REFUSAL_REASON.to_string(),
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
    use super::{
        CoreError, DeployFinding, ErrorBody, ErrorEnvelope, GATEWAY_CLIENT_BODY_CAP_BYTES,
        GATEWAY_CLIENT_BODY_TRUNCATION_MARKER, LOSS_GATE_REFUSAL_REASON_PREFIX,
        ModuleDigestChangedDetails, ModuleDigestMismatchDetails, PreflightFailure, RollbackFailure,
        SignatureReason, TESTING_ROUTE_PREFIX, truncate_api_body,
    };

    /// Build a real `reqwest::Error` for the Network variant: a request to
    /// an unroutable loopback port fails at connect time (instant refusal —
    /// `reqwest::Error` has no public constructor).
    fn network_error() -> CoreError {
        // reqwest is built with `rustls-no-provider`, so a raw
        // `reqwest::get` panics unless the process-wide provider is
        // installed. In a full lib-test run some other test happens to
        // install it first; running the error tests ALONE (which the
        // exit-table verify command does) hit the panic. The install is
        // idempotent and documented for exactly this use.
        crate::client::install_crypto_provider();
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
            observation: None,
        }
    }

    /// The [`CoreError::ModuleFeedUnreachable`] twin of [`network_error`]
    /// — a real `reqwest::Error` from an unroutable loopback port, for
    /// the offline-module-feed exit-4 case.
    fn module_feed_unreachable_error() -> CoreError {
        crate::client::install_crypto_provider();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let url = "http://127.0.0.1:1";
        let source = rt
            .block_on(reqwest::get(url))
            .expect_err("request to an unroutable port must fail");
        CoreError::ModuleFeedUnreachable {
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
                CoreError::ProfilingRiskNotAccepted {
                    operation: "jvm sample".into(),
                },
                2,
                "profiling_risk_not_accepted",
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
                CoreError::PasswordUnavailable {
                    profile: "dev".into(),
                },
                3,
                "password_unavailable",
            ),
            (
                CoreError::ConfigInvalid {
                    reason: "bad toml".into(),
                },
                3,
                "config_invalid",
            ),
            (
                CoreError::PollIntervalTooSmall {
                    profile: "dev".into(),
                },
                3,
                "poll_interval_too_small",
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
            (
                CoreError::ImportDenied {
                    project: "PlantFloor".into(),
                    problem: "resource already exists: ResourceId{resourcePath=com.example, collectionName=views}".into(),
                    endpoint: Some("http://gw:8088/data/api/v1/projects/import/PlantFloor?overwrite=true".into()),
                },
                6,
                "import_denied",
            ),
            (
                CoreError::SessionNotPrunable {
                    id: "d-live-1".into(),
                    endpoint: Some("http://gw:8088/data/api/v1/designer/d-live-1".into()),
                },
                6,
                "session_not_prunable",
            ),
            (
                CoreError::EamNotController {
                    endpoint: Some("http://gw:8088/data/eam/api/v1/eam-tasks/history".into()),
                },
                6,
                "eam_not_controller",
            ),
            (
                CoreError::ProviderRootUnsupported {
                    endpoint: Some("/system/webdev/ign-cli/cli/tagConfig".into()),
                },
                6,
                "provider_root_unsupported",
            ),
            (
                CoreError::EamTaskTypeRefused {
                    task_type: "eam_remoteUpgrade".into(),
                },
                6,
                "eam_task_type_refused",
            ),
            (
                CoreError::EamTaskInFlight {
                    task: "cli-research-backup".into(),
                    detail: "Task 'cli-research-backup (forced)' already exists! It must be completed or deleted before another task of this type can be force executed.".into(),
                    endpoint: Some(
                        "http://gw:8088/data/eam/api/v1/eam-tasks/force/eam/cli-research-backup"
                            .into(),
                    ),
                },
                6,
                "eam_task_in_flight",
            ),
            (
                CoreError::ScriptExecNotConfigured {
                    profile: "dev".into(),
                },
                6,
                "script_exec_not_configured",
            ),
            (CoreError::LintToolAbsent, 6, "lint_tool_absent"),
            (
                CoreError::NodeToolAbsent {
                    tool: "node".into(),
                },
                6,
                "node_tool_absent",
            ),
            (
                CoreError::BundleNotAvailable {
                    state: "Invalid".into(),
                },
                6,
                "bundle_not_available",
            ),
            (
                CoreError::GatewayClientError {
                    status: 400,
                    endpoint: "http://gw:8088/data/api/v1/nonexistent".into(),
                    body: r#"{"error":{"code":"NOT_FOUND"}}"#.into(),
                },
                2,
                "gateway_client_error",
            ),
            (CoreError::Rig("compose up failed".into()), 7, "rig_error"),
            (
                module_feed_unreachable_error(),
                4,
                "module_feed_unreachable",
            ),
            (
                CoreError::ModuleReleaseNotFound {
                    module: "git".into(),
                    version: "99.99.99".into(),
                    url: "https://api.github.com/repos/WhiskeyHouse/ignition-git-module/releases/tags/v99.99.99".into(),
                    detail: "no release exists for tag \"v99.99.99\"".into(),
                },
                6,
                "module_release_not_found",
            ),
            (
                CoreError::ModuleDigestMismatch(Box::new(ModuleDigestMismatchDetails {
                    module: "git".into(),
                    version: "2.3.4".into(),
                    url: "https://release-assets.githubusercontent.com/Git-2.3.4-signed.modl".into(),
                    expected: "b".repeat(64),
                    actual: "c".repeat(64),
                })),
                6,
                "module_digest_mismatch",
            ),
            (
                CoreError::ModuleFeedUnusable {
                    url: "https://api.github.com/repos/WhiskeyHouse/ignition-git-module/releases/tags/v2.3.4".into(),
                    detail: "rate limit exceeded".into(),
                },
                6,
                "module_feed_unusable",
            ),
            (
                CoreError::ModuleDigestChanged(Box::new(ModuleDigestChangedDetails {
                    module: "git".into(),
                    version: "2.3.4".into(),
                    cached_digest: "a".repeat(64),
                    upstream_digest: "b".repeat(64),
                    cached_path: "/cache/modules/git/2.3.4-aaaa.modl".into(),
                })),
                6,
                "module_digest_changed",
            ),
            (
                CoreError::ModuleNotRegistered {
                    id: "bogus".into(),
                    known: vec!["git".into()],
                },
                3,
                "module_not_registered",
            ),
            (
                CoreError::ModuleUninstallDenied {
                    module_id: "git".into(),
                    gateway_module_id: "com.axone_io.ignition.git".into(),
                    failed: vec!["com.axone_io.ignition.git".into()],
                    endpoint: Some(
                        "http://gw:8088/data/api/v1/modules/uninstall".into(),
                    ),
                },
                6,
                "module_uninstall_denied",
            ),
            (
                CoreError::ScanLockHeld {
                    actor: Some("api-token:cap-a:172.17.0.1".into()),
                    kind: "config".into(),
                    endpoint: Some("http://gw:8088/data/api/v1/scan-lock/config".into()),
                },
                6,
                "scan_lock_held",
            ),
            (
                CoreError::ResourceSignatureRejected {
                    name: "DB_A".into(),
                    reason: SignatureReason::Stale,
                    endpoint: Some(
                        "http://gw:8088/data/api/v1/resources/find/ignition/database-connection/DB_A"
                            .into(),
                    ),
                },
                6,
                "resource_signature_rejected",
            ),
            (
                CoreError::ResourceExists {
                    name: "DB_A".into(),
                    endpoint: Some("http://gw:8088/data/api/v1/resources/ignition/database-connection".into()),
                },
                6,
                "resource_exists",
            ),
            (
                CoreError::ModeExists {
                    name: "production".into(),
                    endpoint: Some("http://gw:8088/data/api/v1/mode".into()),
                },
                6,
                "mode_exists",
            ),
            (
                CoreError::deploy_tree_invalid("manifest.yaml", Some(2), Some(1), "unknown field"),
                2,
                "deploy_tree_invalid",
            ),
            (
                CoreError::embedded_secret(vec!["a.json: settings.password".into()], vec![]),
                2,
                "embedded_secret",
            ),
            (
                CoreError::TransportFailed {
                    operation: "read".into(),
                    node: "nodes.master".into(),
                    detail: "cannot read directory config/resources (NotFound)".into(),
                },
                4,
                "transport_failed",
            ),
            (
                CoreError::DeployLedgerInvalid {
                    location: "ign-deploy/gw/dev/ledger.json".into(),
                    problem: "files is not byte-sorted".into(),
                },
                6,
                "deploy_ledger_invalid",
            ),
            (
                CoreError::PlanStale {
                    expected: "a".repeat(64),
                    actual: "b".repeat(64),
                    detail: "review hash".into(),
                },
                6,
                "plan_stale",
            ),
            (
                CoreError::DestructiveChangeRefused {
                    gate: "destructive".into(),
                    keys: vec!["ignition/a/x".into()],
                    count: 1,
                },
                2,
                "destructive_change_refused",
            ),
            (
                CoreError::DeployBlocked {
                    keys: vec!["ignition/a/x".into()],
                },
                6,
                "deploy_blocked",
            ),
            (
                CoreError::DeployPreflightFailed {
                    failures: vec![PreflightFailure {
                        check: "mode_master".into(),
                        detail: "the gateway is not in the declared mode".into(),
                    }]
                    .into(),
                },
                6,
                "deploy_preflight_failed",
            ),
            (
                CoreError::DeployJournalPresent {
                    run_id: "r1".into(),
                    env: "prod".into(),
                    location: "ign-deploy/gw/m/journal.json".into(),
                },
                6,
                "deploy_journal_present",
            ),
            (
                CoreError::DeployLiveChanged {
                    run_id: "r1".into(),
                    paths: vec!["a/b.json".into()],
                    count: 1,
                },
                6,
                "deploy_live_changed",
            ),
            (
                CoreError::DeployRunInvalid {
                    run_id: "r1".into(),
                    problem: "record.json is missing".into(),
                },
                6,
                "deploy_run_invalid",
            ),
            (
                CoreError::RollbackFailed(Box::new(RollbackFailure {
                    run_id: "r1".into(),
                    env: "dev".into(),
                    run_dir: Some(".ign/deploy/r1".into()),
                    journal: "ign-deploy/gw/m/journal.json".into(),
                    cause: "write_failed".into(),
                })),
                6,
                "rollback_failed",
            ),
            (
                CoreError::ProjectGitManaged {
                    project: "CapApp".into(),
                    location: "projects/CapApp/.git".into(),
                    kind: "directory".into(),
                },
                6,
                "project_git_managed",
            ),
            (
                CoreError::ProjectParentInvalid {
                    project: "CapApp".into(),
                    parent: "WHK-Global".into(),
                    reason: "missing".into(),
                },
                6,
                "project_parent_invalid",
            ),
            (
                CoreError::ProjectHashMismatch {
                    project: "CapApp".into(),
                    expected: "0".repeat(64),
                    actual: "1".repeat(64),
                },
                6,
                "project_hash_mismatch",
            ),
            (
                CoreError::DeployUnreadable {
                    keys: vec!["ignition/tag-definition/default/Deep".into()],
                },
                6,
                "deploy_unreadable",
            ),
        ];
        for (err, code, slug) in cases {
            assert_eq!(err.exit_code(), code, "wrong exit code for: {err}");
            assert_eq!(err.code(), slug, "unstable slug for: {err}");
        }
    }

    /// The (exit_code, slug) literal table the Three-Place rule syncs.
    /// DUPLICATED from [`exit_code_mapping_enumerated`]'s triple list by
    /// design: that test is the literal source of truth (enum ↔ literals);
    /// this flat table is what the README cross-check runs against, so a
    /// drift inside error.rs surfaces as a disagreement instead of silently
    /// re-shuffling both sides. Keep the two lists in lockstep — the
    /// enumerated test fails if the enum respells/renumbers, and a stale
    /// copy here fails the README check below until updated.
    const EXIT_SLUG_LITERALS: &[(u8, &str)] = &[
        (1, "internal"),
        (2, "confirmation_required"),
        (2, "profiling_risk_not_accepted"),
        (2, "invalid_import_file"),
        (2, "invalid_input"),
        (2, "gateway_client_error"),
        (2, "deploy_tree_invalid"),
        (2, "embedded_secret"),
        (2, "destructive_change_refused"),
        (3, "profile_not_found"),
        (3, "no_active_profile"),
        (3, "secret_unavailable"),
        (3, "password_unavailable"),
        (3, "config_invalid"),
        (3, "poll_interval_too_small"),
        (3, "module_not_registered"),
        (4, "network_error"),
        (5, "auth_rejected"),
        (6, "gateway_too_old"),
        (6, "gateway_not_commissioned"),
        (6, "gateway_restarting"),
        (6, "not_found"),
        (6, "project_exists"),
        (6, "resource_binary"),
        (6, "trial_not_expired"),
        (6, "provider_not_found"),
        (6, "routes_not_deployed"),
        (6, "webdev_unlicensed"),
        (6, "route_version_mismatch"),
        (6, "webdev_route_error"),
        (6, "tag_collision"),
        (6, "alarm_journal_missing"),
        (6, "import_denied"),
        (6, "session_not_prunable"),
        (6, "eam_not_controller"),
        (6, "eam_task_type_refused"),
        (6, "eam_task_in_flight"),
        (6, "script_exec_not_configured"),
        (6, "lint_tool_absent"),
        (6, "node_tool_absent"),
        (6, "provider_root_unsupported"),
        (6, "bundle_not_available"),
        (4, "module_feed_unreachable"),
        (4, "transport_failed"),
        (6, "module_release_not_found"),
        (6, "module_digest_mismatch"),
        (6, "module_feed_unusable"),
        (6, "module_digest_changed"),
        (6, "module_uninstall_denied"),
        (6, "scan_lock_held"),
        (6, "resource_signature_rejected"),
        (6, "resource_exists"),
        (6, "mode_exists"),
        (6, "deploy_ledger_invalid"),
        (6, "plan_stale"),
        (6, "deploy_blocked"),
        (6, "deploy_preflight_failed"),
        (6, "deploy_journal_present"),
        (6, "deploy_live_changed"),
        (6, "deploy_run_invalid"),
        (6, "rollback_failed"),
        (6, "project_git_managed"),
        (6, "project_parent_invalid"),
        (6, "project_hash_mismatch"),
        (6, "deploy_unreadable"),
        (7, "rig_error"),
    ];

    /// The two deploy slugs (20-02) never carry a signature value or
    /// gateway problem text, and render their payloads sensibly.
    #[test]
    fn deploy_slugs_never_carry_signature_or_problem_text() {
        const SAMPLE_SIG: &str = "d80c81d167362252bf389893ea0f3a81479521ba0bb68ee3bfc46e94bc2540b7";

        let held = CoreError::ScanLockHeld {
            actor: Some("api-token:cap-a:172.17.0.1".into()),
            kind: "config".into(),
            endpoint: Some("http://gw:8088/data/api/v1/scan-lock/config".into()),
        };
        assert_eq!(held.code(), "scan_lock_held");
        assert_eq!(held.exit_code(), 6);
        let shown = held.to_string();
        assert!(shown.contains("api-token:cap-a:172.17.0.1"), "{shown}");
        assert!(shown.contains("config"), "{shown}");
        assert_eq!(
            held.endpoint().as_deref(),
            Some("http://gw:8088/data/api/v1/scan-lock/config")
        );

        let anonymous = CoreError::ScanLockHeld {
            actor: None,
            kind: "config".into(),
            endpoint: None,
        };
        let shown = anonymous.to_string();
        assert!(shown.contains("another actor"), "{shown}");
        assert!(!shown.contains("None"), "{shown}");

        for reason in [SignatureReason::Stale, SignatureReason::Missing] {
            let err = CoreError::ResourceSignatureRejected {
                name: "DB_A".into(),
                reason,
                endpoint: Some("http://gw:8088/data/api/v1/resources/x".into()),
            };
            assert_eq!(err.code(), "resource_signature_rejected");
            assert_eq!(err.exit_code(), 6);
            let shown = err.to_string();
            assert!(shown.contains("DB_A"), "{shown}");
            assert!(shown.contains(&reason.to_string()), "{shown}");
            assert!(!shown.contains(SAMPLE_SIG), "{shown}");
            let hint = err.hint().expect("hint required");
            assert!(!hint.contains(SAMPLE_SIG), "{hint}");
            let endpoint = err.endpoint().expect("endpoint");
            assert!(!endpoint.contains(SAMPLE_SIG), "{endpoint}");
        }
        assert_eq!(SignatureReason::Stale.to_string(), "stale");
        assert_eq!(SignatureReason::Missing.to_string(), "missing");
    }

    /// Parse the README's exit-code table: rows `| <exit> | class | meaning |
    /// \`slug\`, ... |` where `<exit>` is 1–7 (row 0 is the success row, no
    /// slugs). Parsing is SCOPED to the `## Exit codes` section — the README
    /// contains unrelated tables whose rows coincidentally begin `| 5 |`,
    /// `| 3 |`, etc. Std-only string ops: split on `|`, trim, take the LAST
    /// non-empty cell as the slug column (the meaning column can carry
    /// backticked non-slugs like `--yes`), and keep the backtick-delimited
    /// tokens that start with an ASCII letter and contain no spaces.
    fn parse_readme_exit_table(readme: &str) -> Vec<(u8, Vec<String>)> {
        let section = readme.split("## Exit codes").nth(1).unwrap_or_default();
        let mut rows = Vec::new();
        let table_lines = section
            .lines()
            .skip_while(|line| !line.trim_start().starts_with('|'));
        for line in table_lines {
            if !line.trim_start().starts_with('|') {
                break; // table ended
            }
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let Some(exit) = cells.get(1).and_then(|cell| cell.parse::<u8>().ok()) else {
                continue;
            };
            if !(1..=7).contains(&exit) {
                continue;
            }
            let slug_cell = cells
                .iter()
                .rev()
                .find(|cell| !cell.is_empty())
                .copied()
                .unwrap_or_default();
            let slugs: Vec<String> = slug_cell
                .split('`')
                .enumerate()
                .filter(|(idx, _)| idx % 2 == 1)
                .map(|(_, token)| token.trim().to_string())
                .filter(|token| {
                    token.starts_with(|c: char| c.is_ascii_alphabetic()) && !token.contains(' ')
                })
                .collect();
            rows.push((exit, slugs));
        }
        rows
    }

    /// CORE-11, the Three-Place slug rule made executable: the exit-code
    /// table exists in the enum ([`CoreError::exit_code`]), this file's
    /// literal triples, and the README — and the README side is now
    /// machine-checked. The README is parsed verbatim via `include_str!`
    /// and cross-checked against the literal table in BOTH directions:
    /// (a) every literal slug appears under its exit code (a README row
    /// that lost or misspelled a slug fails), (b) every README slug token
    /// exists in the literal table (a stale/deleted row fails). Exit 6
    /// carries 23 slugs, so the full cross-check is the value — not the
    /// happy-path smoke.
    #[test]
    fn readme_exit_table_agreement() {
        let readme = include_str!("../../../README.md");
        let rows = parse_readme_exit_table(readme);
        assert!(
            rows.len() >= 7,
            "README exit-code table not found — the parser must see all 7 \
             failure-class rows (found {})",
            rows.len()
        );

        // Direction (a): every literal (exit, slug) is present in the
        // README row matching its exit code.
        for (exit, slug) in EXIT_SLUG_LITERALS {
            let row = rows
                .iter()
                .find(|(readme_exit, _)| readme_exit == exit)
                .unwrap_or_else(|| panic!("README table has no row for exit {exit}"));
            assert!(
                row.1.iter().any(|s| s == slug),
                "README exit-{exit} row is missing slug {slug:?} (row: {:?})",
                row.1
            );
        }

        // Direction (b): every README slug token exists in the literal
        // table under the same exit code — catches stale/deleted rows.
        for (exit, slugs) in &rows {
            for slug in slugs {
                assert!(
                    EXIT_SLUG_LITERALS
                        .iter()
                        .any(|(lit_exit, lit_slug)| lit_exit == exit && lit_slug == slug),
                    "README exit-{exit} row carries slug {slug:?} that no \
                     CoreError variant emits — stale table row or slug/exit \
                     drift between README and error.rs"
                );
            }
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

        // The live-designer prune refusal (06-07): the hint names the
        // action — close the Designer — and the stale-only semantics.
        let prunable = CoreError::SessionNotPrunable {
            id: "d-live-1".into(),
            endpoint: None,
        };
        let hint = prunable.hint().expect("hint required");
        assert!(
            hint.contains("close the Designer"),
            "prune hint must name the action: {hint}"
        );
        assert!(
            hint.contains("stale entries"),
            "prune hint must name stale-only semantics: {hint}"
        );

        // The scan-lock refusal (20-02): the hint names the release
        // conditions and no ign command.
        let held = CoreError::ScanLockHeld {
            actor: None,
            kind: "config".into(),
            endpoint: None,
        };
        let hint = held.hint().expect("hint required");
        assert!(
            hint.contains("holdTimeout") && hint.contains("scan runs"),
            "scan-lock hint names the release conditions: {hint}"
        );
        assert!(
            !hint.contains("ign "),
            "scan-lock hint names no ign command: {hint}"
        );

        // The signature refusal (20-02): re-read and retry.
        let rejected = CoreError::ResourceSignatureRejected {
            name: "DB_A".into(),
            reason: SignatureReason::Stale,
            endpoint: None,
        };
        let hint = rejected.hint().expect("hint required");
        assert!(
            hint.contains("re-read") && hint.contains("signatures change on every write"),
            "signature hint names the action: {hint}"
        );

        // The duplicate-create refusals (20-REVIEW WR-01): each names the
        // next step and carries no gateway problem text.
        let exists = CoreError::ResourceExists {
            name: "DB_A".into(),
            endpoint: None,
        };
        let hint = exists.hint().expect("hint required");
        assert!(
            hint.contains("update it"),
            "resource_exists hint names the action: {hint}"
        );
        assert_eq!(exists.to_string(), "resource \"DB_A\" already exists");
        let mode_exists = CoreError::ModeExists {
            name: "production".into(),
            endpoint: None,
        };
        let hint = mode_exists.hint().expect("hint required");
        assert!(
            hint.contains("skip the create"),
            "mode_exists hint names the action: {hint}"
        );

        // The TTY refusal's hint is contextual (06-07): the ONE
        // InvalidInput reason whose fix is terminal-related. Every
        // other reason keeps the --file/stdin resource-put hint (pinned
        // here so the special case can never leak or regress).
        let tty = CoreError::tui_tty_refusal();
        assert_eq!(tty.code(), "invalid_input", "slug unchanged");
        assert_eq!(tty.exit_code(), 2, "usage class unchanged");
        let hint = tty.hint().expect("hint required");
        assert!(
            hint.contains("interactive terminal"),
            "TTY hint must name the fix: {hint}"
        );
        assert!(
            !hint.contains("--file"),
            "TTY hint must not carry the resource-put hint: {hint}"
        );

        // The itop sibling (ign top): the SAME usage class and slug,
        // but the refusal/hint name the MONITOR's command — a hint
        // naming `ign tui` would not fix an `ign top` run (the
        // RoutesNotDeployed rule at the TTY pair).
        let top = CoreError::top_tty_refusal();
        assert_eq!(top.code(), "invalid_input", "slug unchanged");
        assert_eq!(top.exit_code(), 2, "usage class unchanged");
        let top_hint = top.hint().expect("hint required");
        assert!(
            top_hint.contains("`ign top`"),
            "the top refusal's hint names ign top: {top_hint}"
        );
        assert!(
            !top_hint.contains("ign tui"),
            "the top refusal never names the cockpit command: {top_hint}"
        );
        assert!(
            top_hint.contains("interactive terminal"),
            "the top refusal keeps the terminal-contextual fix: {top_hint}"
        );
        let put = CoreError::InvalidInput {
            reason: "cannot read put.json".into(),
        };
        let hint = put.hint().expect("hint required");
        assert!(
            hint.contains("--file") && hint.contains("stdin"),
            "resource-put hint unchanged: {hint}"
        );

        // The EAM controller state gate (07-02): the hint names the
        // manual flip recipe + the README section (role decisions
        // stay one config-PUT away from the CLI, never one flag).
        let controller = CoreError::EamNotController { endpoint: None };
        let hint = controller.hint().expect("hint required");
        assert!(
            hint.contains("installMode"),
            "controller hint names the flip: {hint}"
        );
        assert!(
            hint.contains("Controller"),
            "controller hint names the target role: {hint}"
        );
        assert!(
            hint.contains("README"),
            "controller hint points at the README section: {hint}"
        );

        // The refused task-type ladder top (07-02 Task 3): the hint
        // names the EAM console + the v2 scope pointer.
        let refused = CoreError::EamTaskTypeRefused {
            task_type: "eam_restoreBackup".into(),
        };
        assert_eq!(refused.exit_code(), 6, "target state");
        let hint = refused.hint().expect("hint required");
        assert!(
            hint.contains("EAM console"),
            "refused hint names where to run it: {hint}"
        );
        assert!(
            hint.contains("fleet-wide") || hint.contains("EXT-03"),
            "refused hint names the fleet consequence / scope: {hint}"
        );

        // The force-route 409 (07-06 gap 4): the hint names the
        // resolution — complete or delete the leftover '(forced)'
        // run via the EAM console (no ign verb deletes runs).
        let in_flight = CoreError::EamTaskInFlight {
            task: "cli-research-backup".into(),
            detail: "the previous '(forced)' run must be completed or deleted first".into(),
            endpoint: None,
        };
        assert_eq!(in_flight.exit_code(), 6, "target state");
        let hint = in_flight.hint().expect("hint required");
        assert!(
            hint.contains("EAM console"),
            "in-flight hint names where to resolve it: {hint}"
        );
        assert!(
            hint.contains("complete or delete"),
            "in-flight hint names the resolution: {hint}"
        );

        // The scriptExec structural gate (07-03): the hint names the
        // deploy flag VERBATIM — the flag IS the opt-in (no --yes
        // exists on script run).
        let unconfigured = CoreError::ScriptExecNotConfigured {
            profile: "dev".into(),
        };
        assert_eq!(unconfigured.exit_code(), 6, "target state");
        let hint = unconfigured.hint().expect("hint required");
        assert!(
            hint.contains("ign webdev deploy --with-script-exec"),
            "the hint names the deploy flag verbatim: {hint}"
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

    /// The api-call body cap (09-01): a body at or under
    /// [`GATEWAY_CLIENT_BODY_CAP_BYTES`] rides verbatim (no marker); a
    /// larger body is truncated to the cap on a UTF-8 char boundary with
    /// the exact ASCII marker appended — and the cap is enforced at
    /// CONSTRUCTION, so the variant always carries its final form.
    #[test]
    fn truncates_at_cap_with_marker() {
        // Under the cap: byte-identical passthrough.
        let short = r#"{"error":{"code":"NOT_FOUND"}}"#;
        assert_eq!(truncate_api_body(short), short);
        assert!(!short.contains(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER));

        // Exactly at the cap: still verbatim (the marker only joins an
        // OVER-cap body — the boundary is <=, not <).
        let exact = "x".repeat(GATEWAY_CLIENT_BODY_CAP_BYTES);
        assert_eq!(truncate_api_body(&exact), exact);

        // One byte over: truncated, marker appended, total size bounded.
        let over = "x".repeat(GATEWAY_CLIENT_BODY_CAP_BYTES + 1);
        let truncated = truncate_api_body(&over);
        assert!(
            truncated.ends_with(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER),
            "truncated body must end with the explicit marker"
        );
        assert!(
            truncated.len()
                <= GATEWAY_CLIENT_BODY_CAP_BYTES + GATEWAY_CLIENT_BODY_TRUNCATION_MARKER.len(),
            "truncated body must stay within cap + marker: {}",
            truncated.len()
        );

        // A multi-byte character straddling the cap boundary does not
        // panic (the cut backs up to a char boundary) and still carries
        // the marker.
        let multibyte = "é".repeat(GATEWAY_CLIENT_BODY_CAP_BYTES); // 2 bytes each
        let truncated = truncate_api_body(&multibyte);
        assert!(truncated.ends_with(GATEWAY_CLIENT_BODY_TRUNCATION_MARKER));
        assert!(
            truncated
                .is_char_boundary(truncated.len() - GATEWAY_CLIENT_BODY_TRUNCATION_MARKER.len())
        );
    }

    /// The loss-gate hint override (11-07 gap closure): a reason with the
    /// [`LOSS_GATE_REFUSAL_REASON_PREFIX`] sentinel carries the --yes
    /// hint, while every OTHER InvalidInput reason (generic file-read)
    /// and the TTY-refusal precedent keep their hints byte-identically.
    /// The loss-gate reason is built from the CONST via format! — a
    /// string literal of the sentinel text here would be a third
    /// production-literal hit and break the prefix-uniqueness gate the
    /// plan's verify pins.
    #[test]
    fn loss_gate_hint_override_and_neighbors_unchanged() {
        // Sentinel-prefixed reason (dynamic prose after the header):
        // the --yes hint, never the file-read default.
        let loss_gate = CoreError::InvalidInput {
            reason: format!(
                "{}xml): the scan reports 2 finding(s) before the import:\nre-run \
                 with --yes to import anyway",
                LOSS_GATE_REFUSAL_REASON_PREFIX
            ),
        };
        assert_eq!(loss_gate.code(), "invalid_input", "slug unchanged");
        assert_eq!(loss_gate.exit_code(), 2, "usage class unchanged");
        let hint = loss_gate.hint().expect("hint required");
        assert!(
            hint.contains("--yes"),
            "loss-gate hint must name the --yes re-run: {hint}"
        );
        assert!(
            hint.contains("loss report above"),
            "loss-gate hint must point back at the report: {hint}"
        );
        assert!(
            !hint.contains("fix the input source"),
            "the file-read default must never leak onto the loss-gate path: {hint}"
        );

        // Generic InvalidInput: the file-read default UNCHANGED (the
        // regression pin for every other raise site).
        let generic = CoreError::InvalidInput {
            reason: "x is not valid JSON: expected value at line 1".into(),
        };
        let hint = generic.hint().expect("hint required");
        assert!(
            hint.contains("--file") && hint.contains("stdin"),
            "resource-put hint unchanged: {hint}"
        );
        assert!(
            !hint.contains("--yes"),
            "the --yes hint must not leak onto generic reasons: {hint}"
        );

        // The TTY-refusal precedent intact (06-07).
        let tty = CoreError::tui_tty_refusal();
        let hint = tty.hint().expect("hint required");
        assert!(
            hint.contains("interactive terminal"),
            "TTY hint unchanged: {hint}"
        );
        assert!(
            !hint.contains("--yes"),
            "the --yes hint must not leak onto the TTY path: {hint}"
        );
    }

    /// The testing-scoped `routes_not_deployed` hint (QUICK-p0g): a
    /// `testing/`-prefixed route names the TESTING install commands,
    /// while every other route keeps the generic `ign webdev deploy`
    /// text BYTE-identically. Both branches are pinned so a future
    /// edit cannot silently merge them — sending a caller to
    /// `ign webdev deploy` for an absent testing bundle is a hint
    /// that does not fix the error.
    #[test]
    fn routes_not_deployed_hint_is_testing_scoped() {
        let testing = CoreError::RoutesNotDeployed {
            project: "Flux".into(),
            route: format!("{TESTING_ROUTE_PREFIX}run"),
            endpoint: None,
        };
        assert_eq!(testing.code(), "routes_not_deployed", "slug unchanged");
        assert_eq!(testing.exit_code(), 6, "target-state class unchanged");
        let hint = testing.hint().expect("hint required");
        assert!(
            hint.contains("ign webdev deploy --with-testing"),
            "testing hint must name the deploy flag: {hint}"
        );
        assert!(
            hint.contains("ign adopt") && hint.contains("--testing"),
            "testing hint must name the adopt path: {hint}"
        );

        // Every non-testing route: the generic hint, unchanged.
        let generic = CoreError::RoutesNotDeployed {
            project: "ign-cli".into(),
            route: "tags".into(),
            endpoint: None,
        };
        let hint = generic.hint().expect("hint required");
        assert_eq!(
            hint,
            "run `ign webdev deploy` to install the CLI's WebDev routes into \
             the gateway, then retry",
            "the generic branch is byte-identical to its pre-QUICK-p0g text"
        );
    }

    fn finding(file: &str, line: Option<usize>, column: Option<usize>, msg: &str) -> DeployFinding {
        DeployFinding {
            file: file.into(),
            line,
            column,
            message: msg.into(),
        }
    }

    #[test]
    fn deploy_finding_display_names_file_line_column() {
        let positioned = finding("targets/prod.yaml", Some(3), Some(1), "unknown field `x`");
        assert_eq!(
            positioned.to_string(),
            "targets/prod.yaml:3:1: unknown field `x`"
        );
        let bare = finding("targets/prod.yaml", None, None, "missing field `gateway`");
        assert_eq!(
            bare.to_string(),
            "targets/prod.yaml: missing field `gateway`"
        );
        // A half-positioned finding never claims a position.
        let half = finding("m.yaml", Some(3), None, "m");
        assert_eq!(half.to_string(), "m.yaml: m");
    }

    #[test]
    fn deploy_findings_cap_at_twenty() {
        let many: Vec<DeployFinding> = (0..25)
            .map(|i| finding("manifest.yaml", None, None, &format!("problem {i}")))
            .collect();
        let shown = CoreError::deploy_findings(many).to_string();
        assert!(shown.contains("25 findings: "), "{shown}");
        assert!(shown.contains("problem 19"), "{shown}");
        assert!(!shown.contains("problem 20"), "{shown}");
        assert!(shown.ends_with("; and 5 more"), "{shown}");

        let single = CoreError::deploy_tree_invalid("m.yaml", Some(2), Some(1), "bad");
        assert_eq!(single.to_string(), "deploy tree invalid: m.yaml:2:1: bad");
    }

    #[test]
    fn embedded_secret_display_lists_paths_only() {
        let err = CoreError::embedded_secret(
            vec![
                "ignition/database-connection/A/config.json: settings.password".into(),
                "ignition/database-connection/B/config.json: settings.password".into(),
            ],
            vec![finding("manifest.yaml", None, None, "duplicate type")],
        );
        let shown = err.to_string();
        assert!(shown.contains("2 path(s)"), "{shown}");
        assert!(
            shown.contains("A/config.json: settings.password"),
            "{shown}"
        );
        assert!(
            shown.contains("B/config.json: settings.password"),
            "{shown}"
        );
        assert!(shown.contains("also 1 other tree finding(s)"), "{shown}");
        assert!(shown.contains("manifest.yaml: duplicate type"), "{shown}");

        // The combined list stays within the cap.
        let paths: Vec<String> = (0..25).map(|i| format!("f{i}.json: a.b")).collect();
        let shown =
            CoreError::embedded_secret(paths, vec![finding("m.yaml", None, None, "x")]).to_string();
        assert!(shown.contains("25 path(s)"), "{shown}");
        assert!(shown.contains("and 5 more"), "{shown}");
        assert!(shown.contains("f19.json: a.b"), "{shown}");
        assert!(!shown.contains("f20.json: a.b"), "{shown}");
    }

    #[test]
    fn deploy_slugs_have_their_exit_codes() {
        let errs = [
            (
                CoreError::deploy_tree_invalid("m.yaml", None, None, "bad"),
                2,
            ),
            (
                CoreError::embedded_secret(vec!["a.json: p".into()], vec![]),
                2,
            ),
            (
                CoreError::TransportFailed {
                    operation: "read".into(),
                    node: "nodes.master".into(),
                    detail: "data_dir is missing".into(),
                },
                4,
            ),
            (
                CoreError::DeployLedgerInvalid {
                    location: "ign-deploy/gw/dev/ledger.json".into(),
                    problem: "the file is empty".into(),
                },
                6,
            ),
            (
                CoreError::PlanStale {
                    expected: "a".repeat(64),
                    actual: "b".repeat(64),
                    detail: "review hash".into(),
                },
                6,
            ),
            (
                CoreError::DestructiveChangeRefused {
                    gate: "destructive".into(),
                    keys: vec!["ignition/a/x".into()],
                    count: 1,
                },
                2,
            ),
            (
                CoreError::DeployBlocked {
                    keys: vec!["ignition/a/x".into()],
                },
                6,
            ),
            (
                CoreError::DeployPreflightFailed {
                    failures: vec![PreflightFailure {
                        check: "mode_master".into(),
                        detail: "the gateway is not in the declared mode".into(),
                    }]
                    .into(),
                },
                6,
            ),
            (
                CoreError::DeployJournalPresent {
                    run_id: "20261015T142233Z-a1b2c3".into(),
                    env: "prod".into(),
                    location: "ign-deploy/gw/m/journal.json".into(),
                },
                6,
            ),
            (
                CoreError::DeployLiveChanged {
                    run_id: "r1".into(),
                    paths: vec!["a/b.json".into()],
                    count: 1,
                },
                6,
            ),
            (
                CoreError::DeployRunInvalid {
                    run_id: "r1".into(),
                    problem: "record.json is missing".into(),
                },
                6,
            ),
            (
                CoreError::RollbackFailed(Box::new(RollbackFailure {
                    run_id: "r1".into(),
                    env: "dev".into(),
                    run_dir: Some(".ign/deploy/r1".into()),
                    journal: "ign-deploy/gw/m/journal.json".into(),
                    cause: "write_failed".into(),
                })),
                6,
            ),
            (
                CoreError::ProjectGitManaged {
                    project: "CapApp".into(),
                    location: "projects/CapApp/.git".into(),
                    kind: "directory".into(),
                },
                6,
            ),
            (
                CoreError::ProjectParentInvalid {
                    project: "CapApp".into(),
                    parent: "WHK-Global".into(),
                    reason: "cycle".into(),
                },
                6,
            ),
            (
                CoreError::ProjectHashMismatch {
                    project: "CapApp".into(),
                    expected: "0".repeat(64),
                    actual: "1".repeat(64),
                },
                6,
            ),
            (
                CoreError::DeployUnreadable {
                    keys: vec!["ignition/tag-definition/default/Deep".into()],
                },
                6,
            ),
        ];
        let codes: Vec<&str> = errs.iter().map(|(e, _)| e.code()).collect();
        assert_eq!(codes, CoreError::DEPLOY_SLUGS);
        for (err, exit) in &errs {
            assert_eq!(err.exit_code(), *exit, "{err}");
            assert_eq!(err.endpoint(), None, "deploy errors name no URL: {err}");
            let hint = err.hint().expect("hint required");
            assert!(
                !hint.contains("`ign ") && !hint.contains("ign "),
                "hint must not name an ign subcommand: {hint}"
            );
        }
    }

    #[test]
    fn apply_gate_flag_refusals_do_not_get_the_file_hint() {
        for reason in [
            "--plan-sha256 must be 64 lowercase hex characters",
            "--allow-drift reason must not be empty",
            "--artifact-sha256 must be 64 lowercase hex characters",
            "--release <tag>: not a semver tag (for example v1.4.0)",
            "--allow-first-change <reason>: the reason is 1 to 500 characters",
            "deploy projects needs --artifact-sha256 when --json is given",
        ] {
            let hint = CoreError::InvalidInput {
                reason: reason.into(),
            }
            .hint()
            .expect("hint");
            assert!(!hint.contains("--file"), "{hint}");
            assert!(hint.contains("flag value"), "{hint}");
        }
    }

    /// The exact ordered deploy slug list. Verdicts (`deploy_rolled_back`,
    /// `deploy_backup_unsynced`, `deploy_project_unverified`) are payload
    /// slugs on a success envelope and never `CoreError` codes. Later plans
    /// extend this list in README row order (25-09, 25-10, 25-11).
    #[test]
    fn deploy_slugs_are_pinned_and_never_a_verdict() {
        assert_eq!(
            CoreError::DEPLOY_SLUGS,
            &[
                "deploy_tree_invalid",
                "embedded_secret",
                "transport_failed",
                "deploy_ledger_invalid",
                "plan_stale",
                "destructive_change_refused",
                "deploy_blocked",
                "deploy_preflight_failed",
                "deploy_journal_present",
                "deploy_live_changed",
                "deploy_run_invalid",
                "rollback_failed",
                "project_git_managed",
                "project_parent_invalid",
                "project_hash_mismatch",
                "deploy_unreadable",
            ]
        );
        assert_eq!(CoreError::DEPLOY_SLUGS.len(), 16);
        for verdict in [
            "deploy_rolled_back",
            "deploy_backup_unsynced",
            "deploy_project_unverified",
        ] {
            assert!(!CoreError::DEPLOY_SLUGS.contains(&verdict), "{verdict}");
        }
    }

    /// 25-07: the four-place rule for `project_git_managed` (variant, code,
    /// exit class 6, hint) and the message names what the operator needs.
    #[test]
    fn project_git_managed_four_places() {
        let err = CoreError::ProjectGitManaged {
            project: "CapApp".into(),
            location: "projects/CapApp/.git".into(),
            kind: "directory".into(),
        };
        assert_eq!(err.code(), "project_git_managed");
        assert_eq!(err.exit_code(), 6);
        let message = err.to_string();
        for needle in [
            "CapApp",
            "projects/CapApp/.git",
            "directory",
            "administrator",
        ] {
            assert!(message.contains(needle), "{needle}: {message}");
        }
        let hint = err.hint().expect("hint required");
        assert!(!hint.contains("ign "), "{hint}");
        assert!(hint.contains(".git"), "{hint}");
        assert!(CoreError::DEPLOY_SLUGS.contains(&"project_git_managed"));
    }

    /// 25-10: the four-place rule for `project_parent_invalid` (variant,
    /// code, exit class 6, hint per reason) and the message names the
    /// project, the parent and the reason.
    #[test]
    fn project_parent_invalid_four_places() {
        for (reason, needle) in [
            ("missing", "its own repo"),
            ("cycle", "no cycle"),
            ("not_inheritable", "inheritable"),
        ] {
            let err = CoreError::ProjectParentInvalid {
                project: "CapApp".into(),
                parent: "WHK-Global".into(),
                reason: reason.into(),
            };
            assert_eq!(err.code(), "project_parent_invalid");
            assert_eq!(err.exit_code(), 6);
            let message = err.to_string();
            for part in ["CapApp", "WHK-Global", reason] {
                assert!(message.contains(part), "{part}: {message}");
            }
            let hint = err.hint().expect("hint required");
            assert!(!hint.contains("ign "), "{hint}");
            assert!(hint.contains(needle), "{reason}: {hint}");
            assert_eq!(err.endpoint(), None);
        }
        assert!(CoreError::DEPLOY_SLUGS.contains(&"project_parent_invalid"));
    }

    /// 25-11: the four-place rule for `project_hash_mismatch` (variant, code,
    /// exit class 6, hint). The message names the project and both hashes and
    /// says nothing was uploaded; nothing else rides it (SC-1, D-01).
    #[test]
    fn project_hash_mismatch_four_places() {
        let err = CoreError::ProjectHashMismatch {
            project: "CapApp".into(),
            expected: "0".repeat(64),
            actual: "1".repeat(64),
        };
        assert_eq!(err.code(), "project_hash_mismatch");
        assert_eq!(err.exit_code(), 6);
        let message = err.to_string();
        for part in [
            "CapApp",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "nothing was uploaded",
        ] {
            assert!(message.contains(part), "{part}: {message}");
        }
        let hint = err.hint().expect("hint required");
        assert!(!hint.contains("ign "), "{hint}");
        assert!(hint.contains("artifact_sha256"), "{hint}");
        assert_eq!(err.endpoint(), None);
        assert!(CoreError::DEPLOY_SLUGS.contains(&"project_hash_mismatch"));
    }

    /// quick-261029-ms3: the four-place rule for `deploy_unreadable`
    /// (variant, code, exit class 6, hint). The message names every unit,
    /// says the host's sftp server cannot read its live path and that nothing
    /// was written; the hint names the two fixes and no `ign` subcommand.
    #[test]
    fn deploy_unreadable_four_places() {
        let keys = vec![
            "ignition/database-translator/OLD".to_string(),
            "ignition/tag-definition/default/Deep".to_string(),
        ];
        let err = CoreError::DeployUnreadable { keys: keys.clone() };
        assert_eq!(err.code(), "deploy_unreadable");
        assert_eq!(err.exit_code(), 6);
        assert_eq!(err.endpoint(), None);
        let message = err.to_string();
        for part in keys.iter().map(String::as_str).chain([
            "sftp server cannot read",
            "nothing was written",
            "2 unit(s)",
        ]) {
            assert!(message.contains(part), "{part}: {message}");
        }
        let hint = err.hint().expect("hint required");
        assert!(!hint.contains("ign "), "{hint}");
        assert!(hint.contains("exclude_tag_providers"), "{hint}");
        assert!(hint.contains("shorten"), "{hint}");
        assert!(CoreError::DEPLOY_SLUGS.contains(&"deploy_unreadable"));
    }

    /// 25-11 (K15): a stale live project is told apart from a stale plan
    /// hash, and neither hint names an `ign` subcommand.
    #[test]
    fn plan_stale_hint_names_the_live_project_case() {
        let live = CoreError::PlanStale {
            expected: "a".repeat(64),
            actual: "b".repeat(64),
            detail: "live project changed since the plan".into(),
        };
        let hint = live.hint().expect("hint");
        assert!(hint.contains("live project"), "{hint}");
        assert!(hint.contains("nothing was imported"), "{hint}");
        assert!(!hint.contains("ign "), "{hint}");
        let review = CoreError::PlanStale {
            expected: "a".repeat(64),
            actual: "b".repeat(64),
            detail: "review hash".into(),
        };
        assert!(
            review.hint().expect("hint").contains("plan_sha256"),
            "the plan-hash case keeps its hint"
        );
    }

    #[test]
    fn journal_present_message_carries_the_recovery_command() {
        let err = CoreError::DeployJournalPresent {
            run_id: "20261015T142233Z-a1b2c3".into(),
            env: "prod".into(),
            location: "ign-deploy/gw/m/journal.json".into(),
        };
        let shown = err.to_string();
        assert!(
            shown.contains("ign deploy rollback --run 20261015T142233Z-a1b2c3 --env prod"),
            "{shown}"
        );
        assert!(shown.contains("ign-deploy/gw/m/journal.json"), "{shown}");
        let hint = err.hint().expect("hint");
        assert!(
            !hint.contains("ign deploy"),
            "D-09: the command lives in Display: {hint}"
        );
    }

    #[test]
    fn rollback_failed_message_names_run_dir_journal_and_the_guidance() {
        let err = CoreError::RollbackFailed(Box::new(RollbackFailure {
            run_id: "r-7".into(),
            env: "prod".into(),
            run_dir: Some(".ign/deploy/r-7".into()),
            journal: "ign-deploy/gw/m/journal.json".into(),
            cause: "write_failed".into(),
        }));
        let shown = err.to_string();
        for needle in [
            "r-7",
            ".ign/deploy/r-7",
            "ign-deploy/gw/m/journal.json",
            ".gwbk",
            "snapshot",
            "by hand",
            // The retry comes first, with the real env, and the intact
            // pre-images are the on-box directory (review WR-02).
            "ign deploy rollback --run r-7 --env prod",
            "ign-deploy/gw/m/snapshots/r-7/",
        ] {
            assert!(shown.contains(needle), "{needle} missing from: {shown}");
        }
        assert!(
            shown.find("Retry with").expect("retry") < shown.find("by hand").expect("by hand"),
            "the retry precedes the hand deletion: {shown}"
        );
        assert!(
            !shown.contains("run-directory snapshot"),
            "the run directory has no snapshot to offer: {shown}"
        );
        let hint = err.hint().expect("hint");
        assert!(
            hint.contains("retry") && !hint.contains("run-directory snapshot"),
            "{hint}"
        );
        let none = CoreError::RollbackFailed(Box::new(RollbackFailure {
            run_id: "r-7".into(),
            env: "prod".into(),
            run_dir: None,
            journal: "j".into(),
            cause: "c".into(),
        }));
        assert!(!none.to_string().contains("None"), "{none}");
    }

    #[test]
    fn preflight_failed_lists_every_failure_in_order() {
        let err = CoreError::DeployPreflightFailed {
            failures: vec![
                PreflightFailure {
                    check: "mode_master".into(),
                    detail: "first".into(),
                },
                PreflightFailure {
                    check: "gwbk".into(),
                    detail: "second".into(),
                },
            ]
            .into(),
        };
        let shown = err.to_string();
        let first = shown.find("mode_master: first").expect("first");
        let second = shown.find("gwbk: second").expect("second");
        assert!(first < second, "{shown}");
        assert_eq!(shown.lines().count(), 3, "{shown}");
    }

    #[test]
    fn apply_refusal_lists_cap_at_ten() {
        let keys: Vec<String> = (0..10).map(|i| format!("u{i}")).collect();
        let shown = CoreError::DestructiveChangeRefused {
            gate: "destructive".into(),
            keys: keys.clone(),
            count: 13,
        }
        .to_string();
        assert!(shown.contains("13 entries"), "{shown}");
        assert!(shown.contains("and 3 more"), "{shown}");
        assert!(shown.contains("--allow-destructive"), "{shown}");
        let drift = CoreError::DestructiveChangeRefused {
            gate: "drift".into(),
            keys: keys[..1].to_vec(),
            count: 1,
        }
        .to_string();
        assert!(drift.contains("--allow-drift"), "{drift}");
        assert!(drift.contains("1 entry"), "{drift}");
        let downgrade = CoreError::DestructiveChangeRefused {
            gate: "downgrade".into(),
            keys: vec!["CapApp".into()],
            count: 1,
        };
        let shown = downgrade.to_string();
        assert!(shown.contains("downgrade"), "{shown}");
        assert!(shown.contains("CapApp"), "{shown}");
        assert!(
            shown.contains("deploy projects needs --allow-downgrade"),
            "{shown}"
        );
        assert!(!shown.contains("apply needs"), "{shown}");
        let first = CoreError::DestructiveChangeRefused {
            gate: "first_deploy".into(),
            keys: vec!["CapApp".into()],
            count: 1,
        };
        let shown = first.to_string();
        assert!(shown.contains("first_deploy"), "{shown}");
        assert!(
            shown.contains("deploy projects needs --allow-first-change <reason>"),
            "{shown}"
        );
        // each gate has its own hint, and none names an `ign` command
        let hints: Vec<String> = [&downgrade, &first]
            .iter()
            .map(|e| e.hint().expect("hint"))
            .collect();
        assert!(hints[0].contains("--allow-downgrade"), "{}", hints[0]);
        assert!(hints[1].contains("--allow-first-change"), "{}", hints[1]);
        for hint in &hints {
            assert!(!hint.contains("ign "), "{hint}");
            assert!(!hint.contains("--allow-destructive"), "{hint}");
        }

        let many: Vec<String> = (0..25).map(|i| format!("a/{i:02}")).collect();
        let blocked = CoreError::DeployBlocked { keys: many.clone() }.to_string();
        assert!(blocked.contains("and 15 more"), "{blocked}");
        assert!(!blocked.contains("a/10"), "{blocked}");
        let live = CoreError::DeployLiveChanged {
            run_id: "r".into(),
            paths: many[..10].to_vec(),
            count: 12,
        }
        .to_string();
        assert!(live.contains("and 2 more"), "{live}");
    }
}
