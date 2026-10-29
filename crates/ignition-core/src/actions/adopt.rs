//! The adopt action (ADOPT-02) — one idempotent verb that walks a
//! commissioned-but-unadopted gateway to a fully working profile:
//! native login → API-key mint (idempotent by name) → permissions
//! wiring → live probe → credential persistence.
//!
//! POSTURE (the proposal's contract, doctor-flavored): each step
//! reports as a data row — `{name, status, detail, hint}` on the
//! doctor's `CheckResult` shape, so rendering comes free — but unlike
//! doctor this verb MUTATES: a step that fails is a hard error
//! (the taxonomy's exit codes), never a skipped-ahead table. Steps
//! already satisfied report `skip`; adopt re-run on an adopted
//! gateway is a four-row all-green no-op.
//!
//! THE BOOTSTRAP ORDER (why session-tier): adopt runs BEFORE any API
//! token exists, so every gateway call rides the native OIDC login
//! (`client::idp` — the trial-reset machinery) — the only credential
//! path that can mint the first key. Wire shapes + pitfalls:
//! `client/adopt.rs` + `.planning/research/ADOPT-RESEARCH.md`.
//!
//! STEP MAP (ADOPT-01's capture):
//!
//! 1. `login` — the OIDC dance → `GatewaySession` (cookie + CSRF).
//! 2. `key` — find-by-name in the api-token resource list; mint when
//!    absent (`generate` → `create`), skip when present and sane,
//!    refuse honestly when present but misconfigured (a secure-only
//!    key on an http gateway cannot be repaired — the plaintext is
//!    unrecoverable by construction).
//! 3. `permissions` — read-modify-write the security-properties
//!    singleton: ADD the bare root of the key's level to the read AnyOf
//!    list, and ADD exactly the write levels (never the bare root, never
//!    a bare `Authenticated`: G-25-11) to the write list. Removal happens
//!    only with `--restrict-write`, and only of the bare `Authenticated`
//!    write entry (never leaving the list empty: `[]` is Public).
//!    Signature riding verbatim. A gateway whose write list still admits
//!    every key reports the row as warn.
//! 4. `probe` — gateway-info plus the harmless scan/projects rescan with
//!    the freshly minted name:key: the key must read AND write (or, with
//!    `--read-only`, read and be refused write with a 403). The live
//!    end-to-end proof (mint-to-200 same second, live-observed).
//! 5. `persist` — keyring first (`auth = { keyring = "profile:…" }`,
//!    zero exposure — better than webdev's 0600 config slot), env-var
//!    fallback (`IGNITION_TOKEN_<PROFILE>` + the one-time `token`
//!    exposure in the result — the ONLY place the plaintext ever
//!    prints, and only when no keyring exists to hold it).
//!
//! Steps 3–5 only run in the mint path; a found key means the
//! profile's existing credential already works (that's how the find
//! got there) — adopt never touches a satisfied profile's auth.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::actions::doctor::CheckResult;
use crate::actions::doctor::CheckStatus;
use crate::client::GatewayApi;
use crate::client::adopt::ResourceMutationWire;
use crate::client::adopt::api_tokens_via_session;
use crate::client::adopt::build_security_put_body;
use crate::client::adopt::build_token_create_body;
use crate::client::adopt::create_api_token_via_session;
use crate::client::adopt::generate_api_key_via_session;
use crate::client::adopt::level_tree;
use crate::client::adopt::merge_level_into;
use crate::client::adopt::merge_write_level_into;
use crate::client::adopt::put_security_properties_via_session;
use crate::client::adopt::remove_bare_authenticated_from_write;
use crate::client::adopt::security_properties_via_session;
use crate::client::adopt::write_admits_every_key;
use crate::client::idp;
use crate::client::idp::GatewaySession;
use crate::client::idp::IdpLoginFlow;
use crate::config;
use crate::config::AuthRef;
use crate::config::Credential;
use crate::config::KeyringStore;
use crate::config::Secret;
use crate::error::CoreError;

/// Adopt options — the CLI seam's defaults land here.
#[derive(Debug, Clone)]
pub struct AdoptOptions {
    /// The gateway login user (the OIDC dance's `username`).
    pub username: String,
    /// The API-key name — the idempotency key AND half of the auth
    /// header value (`<name>:<key>`).
    pub key_name: String,
    /// The granted security-level PATH segments, root first —
    /// `["Authenticated", "Roles", "Administrator"]` by CLI default.
    pub level: Vec<String>,
    /// Deploy the CLI's WebDev routes (scriptExec on) into this
    /// project after the bootstrap (`--project`).
    pub project: Option<String>,
    /// Ship the embedded TESTING bundle with the routes (`--testing`)
    /// — the Jython framework + testing/run|tags routes + the smoke
    /// sentinel; the step ASSERTS discover ≥1 module and a green
    /// smoke run before reporting (the empty-suite trap). Requires
    /// `--project`.
    pub testing: bool,
    /// Check out every ENABLED project into `<dir>/<project>`
    /// (`--checkout`) — scripts decoded, the grep/lint posture.
    pub checkout: Option<std::path::PathBuf>,
    /// Download a roaming gwbk to this path after everything else
    /// landed (`--bake`) — the just-reset keeps key + routes.
    pub bake: Option<std::path::PathBuf>,
    /// The levels written into the gateway's write permissions, one
    /// slash path each (`--write-level`, repeatable). Empty means "the
    /// decision's default" ([`WRITE_ADMISSION`]). A bare `Authenticated`
    /// or `Public` is refused: it would let every API key write.
    pub write_levels: Vec<Vec<String>>,
    /// Mint a key at `--level` that is refused write (`--read-only`).
    /// Never touches write permissions; refuses before minting when the
    /// write permissions already admit every key.
    pub read_only: bool,
    /// Remove the bare `Authenticated` entries from the write
    /// permissions once the write levels are in place
    /// (`--restrict-write`). Opt-in; never empties the list.
    pub restrict_write: bool,
}

/// The adopt result — step rows + (only when a key was minted AND no
/// keyring exists) the one-time token exposure.
#[derive(Debug, Serialize)]
pub struct AdoptResult {
    /// The API-key name adopted under.
    pub key_name: String,
    /// `"name:key"` — THE one-time plaintext exposure, present ONLY
    /// when a key was minted and no keyring could hold it (headless
    /// hosts). The deliberate, single-site exception to the Secret
    /// discipline: the env-var fallback route requires the user to
    /// learn the string exactly once, here. Never logged, never
    /// persisted by the CLI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// How the profile authenticates after adopt: `"keyring"` or
    /// `"token_env:IGNITION_TOKEN_<PROFILE>"` (the fallback the
    /// result's token feeds). `None` when nothing was changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored: Option<String>,
    /// The step rows, execution order (login, key, permissions,
    /// probe, persist).
    pub steps: Vec<CheckResult>,
}

/// The default API-key name — the CLI's key, clear ownership on any
/// gateway it lands on.
pub const DEFAULT_KEY_NAME: &str = "ign-cli";

/// The default granted level path — Administrator (the proposal's
/// posture; gateway-default read/write permissions include it).
pub const DEFAULT_LEVEL: &[&str] = &["Authenticated", "Roles", "Administrator"];

/// How adopt grants write access, as MEASURED (plan 25-24 Task 1,
/// `25-LIVE-CAPTURES.md` `### API key write admission (G-25-11)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAdmission {
    /// A nested write entry for the key's own `--level` admits that key.
    NestedExact,
    /// A dedicated write level (the path) goes into the write list as an
    /// exact tree and onto the minted key as well.
    Dedicated(&'static [&'static str]),
}

/// The write model the capture selected: `dedicated-nested`, identical
/// on `inductiveautomation/ignition:8.3.6` and `:8.3.3`. In both images a
/// nested write entry for the key's exact three-level path admitted no key
/// (so `nested-exact` fails), while the nested `Authenticated>IgnCliWrite`
/// style entry admitted exactly the keys granted that path. Hence the
/// default write level is `Authenticated/IgnCliWrite`, granted to the
/// minted key too.
pub const WRITE_ADMISSION: WriteAdmission =
    WriteAdmission::Dedicated(&["Authenticated", "IgnCliWrite"]);

/// The write levels an adopt run writes (and, when it is not read-only,
/// grants to the key), after input validation. Pure.
pub fn resolve_write_levels(opts: &AdoptOptions) -> Result<Vec<Vec<String>>, CoreError> {
    resolve_write_levels_for(opts, WRITE_ADMISSION)
}

fn resolve_write_levels_for(
    opts: &AdoptOptions,
    admission: WriteAdmission,
) -> Result<Vec<Vec<String>>, CoreError> {
    if opts.read_only {
        if !opts.write_levels.is_empty() || opts.restrict_write {
            return Err(CoreError::InvalidInput {
                reason: "--read-only cannot be combined with --write-level or --restrict-write"
                    .into(),
            });
        }
        return Ok(Vec::new());
    }
    let levels: Vec<Vec<String>> = if opts.write_levels.is_empty() {
        match admission {
            WriteAdmission::NestedExact => vec![opts.level.clone()],
            WriteAdmission::Dedicated(path) => {
                vec![path.iter().map(|segment| segment.to_string()).collect()]
            }
        }
    } else {
        opts.write_levels.clone()
    };
    for level in &levels {
        if level.is_empty() || level.iter().any(|segment| segment.trim().is_empty()) {
            return Err(CoreError::InvalidInput {
                reason: "--write-level must be a slash path of non-empty segments, e.g. \
                         Authenticated/IgnCliWrite"
                    .into(),
            });
        }
        if let [only] = level.as_slice()
            && (only.eq_ignore_ascii_case("authenticated") || only.eq_ignore_ascii_case("public"))
        {
            return Err(CoreError::InvalidInput {
                reason: format!(
                    "--write-level {only} would let every API key write; name a narrower level"
                ),
            });
        }
    }
    Ok(levels)
}

/// The levels the minted key holds: `--level` first, then each write
/// level not already listed. Pure.
pub fn key_levels(opts: &AdoptOptions, write_levels: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut out = vec![opts.level.clone()];
    for level in write_levels {
        if !out.contains(level) {
            out.push(level.clone());
        }
    }
    out
}

/// The adopt walk. `base_url` is the profile's gateway URL; the
/// config rewrite lands in `config_path`'s file.
/// `compose_credential` feeds the post-bootstrap steps
/// (`--project`/`--checkout`/`--bake`) ONLY in the skip path — the
/// mint path rides the fresh key it just proved.
pub async fn adopt(
    base_url: &str,
    profile_name: &str,
    config_path: &Path,
    password: &Secret,
    compose_credential: Option<&Credential>,
    opts: &AdoptOptions,
) -> Result<AdoptResult, CoreError> {
    let mut steps: Vec<CheckResult> = Vec::new();
    let level_path: Vec<&str> = opts.level.iter().map(String::as_str).collect();

    // 0. INPUT — the level path must be non-empty segments BEFORE any
    //    login or gateway I/O (review round: `path.split('/')` keeps
    //    empties, so `--level ""`/`/Authenticated`/`A//B` would mint a
    //    token tree with blank names). InvalidInput (exit 2), the
    //    pre-resolution refusal class.
    if level_path.is_empty() || level_path.iter().any(|segment| segment.trim().is_empty()) {
        return Err(CoreError::InvalidInput {
            reason: "--level must be a slash path of non-empty segments, e.g. \
                     Authenticated/Roles/Administrator"
                .into(),
        });
    }

    // 0b. WRITE LEVELS — resolved and validated before any login, the same
    //     InvalidInput (exit 2) class: a bare Authenticated/Public write
    //     level would let every API key write (G-25-11).
    let write_levels = resolve_write_levels(opts)?;
    check_read_only_key_name(opts)?;
    let granted = key_levels(opts, &write_levels);

    // 1. LOGIN — the OIDC dance. Hard error on failure (adopt cannot
    //    proceed without the bootstrap credential).
    let flow = IdpLoginFlow::new(base_url)?;
    let (flow, session) = idp::login(flow, &opts.username, password).await?;
    steps.push(CheckResult {
        name: "login".into(),
        status: CheckStatus::Ok,
        detail: format!("native OIDC session as {}", opts.username),
        hint: None,
    });

    // 1b. READ-ONLY PRE-CHECK — a read-only key is only meaningful when
    //     the write list does not already admit every key. Refuse BEFORE
    //     minting so no orphan key is left behind.
    if opts.read_only {
        let singleton = security_properties_via_session(&flow, &session).await?;
        if let Some(reason) = write_admits_every_key(&singleton.config) {
            return Err(CoreError::Internal(format!(
                "--read-only cannot mint a read-only key on this gateway: {reason}; run \
                 ign adopt --restrict-write with the administrator profile first"
            )));
        }
    }

    // 2. KEY — find-by-name, mint when absent.
    let tokens = api_tokens_via_session(&flow, &session).await?;
    let existing = tokens.iter().find(|record| record.name == opts.key_name);
    let minted: Option<String> = match existing {
        Some(record) => {
            // Present: the honest idempotent path refuses a key we
            // cannot verify usable — its plaintext is unrecoverable
            // by construction, so a secure-only or disabled key is a
            // delete-and-rerun, never a silent skip.
            if !record.enabled {
                return Err(CoreError::Internal(format!(
                    "API key {:?} exists but is disabled — delete it in the gateway UI \
                     (Platform → Security → API Keys) and re-run adopt",
                    opts.key_name
                )));
            }
            if record.config.profile.secure_channel_required {
                return Err(CoreError::Internal(format!(
                    "API key {:?} exists but requires secure connections — it cannot \
                     authenticate over http; delete it in the gateway UI and re-run adopt",
                    opts.key_name
                )));
            }
            let level_names: Vec<String> = record
                .config
                .profile
                .security_levels
                .iter()
                .map(crate::client::adopt::level_path_string)
                .collect();
            // Review round: a same-name key at a DIFFERENT level than
            // requested must refuse, not silently ride — the caller
            // asked for one posture and would get another.
            let requested: Vec<String> = granted.iter().map(|path| path.join("/")).collect();
            if !requested
                .iter()
                .all(|want| level_names.iter().any(|have| have == want))
            {
                return Err(CoreError::Internal(format!(
                    "API key {:?} exists at level {:?} but {:?} was requested — \
                     pass --level (and --write-level) to match, or delete the key \
                     and re-run to mint the requested levels",
                    opts.key_name, level_names, requested
                )));
            }
            steps.push(CheckResult {
                name: "key".into(),
                status: CheckStatus::Skip,
                detail: format!(
                    "API key {:?} already present (level {:?}, secure off) — nothing minted",
                    opts.key_name, level_names
                ),
                hint: None,
            });
            None
        }
        None => {
            let trees: Vec<Value> = granted
                .iter()
                .map(|path| {
                    let segments: Vec<&str> = path.iter().map(String::as_str).collect();
                    level_tree(&segments)
                })
                .collect();
            let token = mint(&flow, &session, &opts.key_name, &trees).await?;
            steps.push(CheckResult {
                name: "key".into(),
                status: CheckStatus::Ok,
                detail: format!(
                    "minted API key {:?} at {} (secure connections off)",
                    opts.key_name,
                    granted
                        .iter()
                        .map(|path| path.join("/"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                hint: None,
            });
            Some(token)
        }
    };

    // 3. PERMISSIONS — add-if-missing: the bare root of --level into read,
    //    exactly the write levels into write (never the bare root: the root
    //    of any level under Authenticated admits every key, G-25-11). The
    //    PUT replaces the whole singleton, the signature rides verbatim.
    //    Runs in BOTH paths: a found key can still be locked out by
    //    permissions (doctor's part-2 finding). Removal only on request.
    let singleton = security_properties_via_session(&flow, &session).await?;
    let mut config = singleton.config.clone();
    let level = level_tree(&level_path);
    let changed_read = merge_level_into(&mut config, "readPermissions", &level);
    let mut added_write: Vec<String> = Vec::new();
    if !opts.read_only {
        for write_level in &write_levels {
            let segments: Vec<&str> = write_level.iter().map(String::as_str).collect();
            if merge_write_level_into(&mut config, &level_tree(&segments)) {
                added_write.push(write_level.join("/"));
            }
        }
    }
    let removed_bare = if opts.restrict_write {
        remove_bare_authenticated_from_write(&mut config).map_err(CoreError::Internal)?
    } else {
        false
    };
    let changed = changed_read || !added_write.is_empty() || removed_bare;
    let mut permissions_row = if changed {
        let body = build_security_put_body(&singleton, &config);
        let answer = put_security_properties_via_session(&flow, &session, &body).await?;
        if !answer.success {
            return Err(CoreError::Internal(format!(
                "security-properties write refused: {:?}",
                answer.problem
            )));
        }
        let mut parts: Vec<String> = Vec::new();
        if changed_read {
            parts.push(format!("read: added {}", opts.level[0]));
        }
        if !added_write.is_empty() {
            parts.push(format!("write: added {}", added_write.join(", ")));
        }
        if removed_bare {
            parts.push("write: removed the bare Authenticated entry".to_string());
        }
        CheckResult {
            name: "permissions".into(),
            status: CheckStatus::Ok,
            detail: format!("wired gateway permissions ({})", parts.join("; ")),
            hint: None,
        }
    } else {
        CheckResult {
            name: "permissions".into(),
            status: CheckStatus::Skip,
            detail: if opts.read_only {
                format!(
                    "gateway read permissions already admit {} — nothing written",
                    opts.level.join("/")
                )
            } else {
                format!(
                    "gateway permissions already admit {} (write: {}) — nothing written",
                    opts.level.join("/"),
                    write_levels
                        .iter()
                        .map(|path| path.join("/"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            hint: None,
        }
    };
    if !opts.read_only
        && let Some(reason) = write_admits_every_key(&config)
    {
        permissions_row.status = CheckStatus::Warn;
        permissions_row.detail = format!("{}; {reason}", permissions_row.detail);
        permissions_row.hint = Some(
            "re-run with --restrict-write to remove it; other API keys that relied on it \
             lose write access"
                .into(),
        );
    }
    steps.push(permissions_row);

    // 4. PROBE — with the minted token (the mint path), or with the
    //    CLI-resolved profile credential (the skip path — the review
    //    round: "key pre-existed" must not skip the liveness check
    //    when a credential IS in hand; only a credential-less skip
    //    defers to `ign doctor`).
    // Only TOKEN credentials can pass /data auth (Basic never does on
    // 8.3) — a Basic-only resolution is not a probeable credential.
    let probe_credential: Option<Credential> = minted
        .as_ref()
        .map(|token| Credential::Token(Secret::new(token.clone())))
        .or_else(|| {
            compose_credential
                .cloned()
                .filter(|credential| matches!(credential, Credential::Token(_)))
        });
    let token_for_persist: Option<String> = match probe_credential {
        Some(credential) => {
            let url: url::Url = base_url
                .parse()
                .map_err(|err| CoreError::Internal(format!("invalid gateway URL: {err}")))?;
            let probe = crate::session::Session::for_url(url, Some(credential), true)?;
            probe.api().gateway_info().await?;
            let via = if minted.is_some() {
                "the fresh name:key"
            } else {
                "the profile's resolved credential"
            };
            // The write half: the same harmless scan/projects rescan
            // doctor's --check-write uses. 2xx = the key writes, 403 =
            // the key is refused write.
            //
            // The permissions PUT just above can take a moment to apply, so a
            // stale 403 straight after it is retried (WR-01). Without it the
            // minted key would be orphaned by a race.
            let write_probe =
                probe_write_with_retry(changed && !opts.read_only, WRITE_PROBE_RETRY_DELAY, || {
                    probe.api().scan_projects()
                })
                .await;
            let write_names = write_levels
                .iter()
                .map(|path| path.join("/"))
                .collect::<Vec<_>>()
                .join(", ");
            let detail = match write_probe_verdict(
                opts.read_only,
                write_probe,
                via,
                &write_names,
                &opts.key_name,
            ) {
                Ok(detail) => detail,
                // WR-01: the plaintext of a key minted by THIS run exists
                // nowhere else, so keep it before the error leaves. The
                // re-run then finds the key, has a credential, and repairs
                // in place instead of dead-ending on SecretUnavailable.
                Err((err, keep_token)) => {
                    return Err(keep_minted_token(
                        err,
                        &opts.key_name,
                        minted.as_deref(),
                        keep_token,
                        |token| persist_after_failed_probe(config_path, profile_name, token),
                    ));
                }
            };
            steps.push(CheckResult {
                name: "probe".into(),
                status: CheckStatus::Ok,
                detail,
                hint: None,
            });
            minted
        }
        None => {
            // Review round: a pre-existing key with NO usable
            // credential is the minted-then-failed re-run trap (run 1
            // minted, persistence failed — e.g. a denied keychain;
            // run 2 "succeeds" all-skip while the profile is
            // unusable). That state must NOT report success.
            return Err(CoreError::SecretUnavailable {
                profile: profile_name.to_string(),
            });
        }
    };

    // 5. PERSIST — keyring first; env fallback prints the token ONCE.
    let (stored, exposed): (Option<String>, Option<String>) = match &token_for_persist {
        None => (None, None),
        Some(token) => {
            let keyring = KeyringStore;
            match keyring.set(profile_name, &Secret::new(token.clone())) {
                Ok(()) => {
                    rewrite_profile_auth(
                        config_path,
                        profile_name,
                        AuthRef::Keyring {
                            keyring: format!("profile:{profile_name}"),
                        },
                    )?;
                    (Some("keyring".into()), None)
                }
                Err(err) => {
                    // Headless hosts (no D-Bus keyring) are EXPECTED —
                    // the documented fallback: env var + one print.
                    tracing::debug!(error = %err, "keyring unavailable; env fallback");
                    let var = format!("IGNITION_TOKEN_{}", super_profile_env_suffix(profile_name));
                    rewrite_profile_auth(
                        config_path,
                        profile_name,
                        AuthRef::TokenEnv {
                            token_env: var.clone(),
                        },
                    )?;
                    (Some(format!("token_env:{var}")), Some(token.clone()))
                }
            }
        }
    };
    match &stored {
        Some(how) if token_for_persist.is_some() => steps.push(CheckResult {
            name: "persist".into(),
            status: CheckStatus::Ok,
            detail: match exposed {
                Some(_) => format!(
                    "profile now expects {} (no keyring on this host) — export it \
                     with the token from this run's output",
                    how
                ),
                None => format!("credential stored in the OS keyring — profile {how:?} wired"),
            },
            hint: exposed
                .as_ref()
                .map(|_| "export the token now; it is never shown again".into()),
        }),
        _ => steps.push(CheckResult {
            name: "persist".into(),
            status: CheckStatus::Skip,
            detail: "profile auth untouched (key pre-existed)".into(),
            hint: None,
        }),
    }

    // 6-8. COMPOSITION (`--project` / `--checkout` / `--bake`): the
    //     post-bootstrap steps ride a WORKING credential — the fresh
    //     key in the mint path, the CLI-resolved profile credential
    //     in the skip path (bootstrap resolved DEGRADED; the mint is
    //     what makes a credential exist). The verbs themselves are
    //     the existing actions, composed verbatim.
    let composing =
        opts.project.is_some() || opts.checkout.is_some() || opts.bake.is_some() || opts.testing;
    if composing {
        let credential: Option<Credential> = match &token_for_persist {
            Some(token) => Some(Credential::Token(Secret::new(token.clone()))),
            None => compose_credential.cloned(),
        };
        let Some(credential) = credential else {
            // Skip path + no resolvable profile credential + explicit
            // composition flags: the honest mutating-verb refusal.
            return Err(CoreError::SecretUnavailable {
                profile: profile_name.to_string(),
            });
        };
        let url: url::Url = base_url
            .parse()
            .map_err(|err| CoreError::Internal(format!("invalid gateway URL: {err}")))?;
        let session = crate::session::Session::for_url(url, Some(credential), true)?;
        let api = session.api();

        // 6. ROUTES — the embedded WebDev bundle, scriptExec on, the
        //    existing secret lifecycle (persist-before-upload); the
        //    testing bundle rides the SAME import when `--testing`.
        if let Some(project) = &opts.project {
            let deployed = crate::actions::webdev::webdev_deploy(
                api,
                project,
                true,
                false,
                config_path,
                profile_name,
                opts.testing,
            )
            .await?;
            steps.push(CheckResult {
                name: "routes".into(),
                status: CheckStatus::Ok,
                detail: format!(
                    "deployed {} WebDev routes into {project:?} (scriptExec on, secret {}{})",
                    deployed.routes.len(),
                    if deployed.secret_rotated {
                        "generated"
                    } else {
                        "reused"
                    },
                    if opts.testing {
                        ", testing bundle on"
                    } else {
                        ""
                    }
                ),
                hint: None,
            });

            // 6b. TESTING — the smoke assertions (the empty-suite
            //     trap): discover must list ≥1 module (the sentinel
            //     ships with the bundle — zero means the discovery
            //     walk broke, which is a FAILED deploy, not a green
            //     one), and the sentinel suite must run green through
            //     the run route (POST exercises doPost end-to-end).
            if opts.testing {
                // First-touch warmup (live-pinned, both on the reset
                // rig and the fresh-gateway run): a freshly deployed
                // route can answer 500 on its FIRST request while
                // WebDev lazily compiles the module — the second hit
                // answers. One bounded retry, then the error is real.
                let discover = |()| crate::client::webdev::testing_discover(api, project);
                let discovered = match discover(()).await {
                    Ok(value) => value,
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        discover(()).await?
                    }
                };
                let count = discovered.get("count").and_then(Value::as_i64).unwrap_or(0);
                let modules = discovered
                    .get("discovered_modules")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if count < 1 || modules.is_empty() {
                    return Err(CoreError::Internal(format!(
                        "testing deploy is NOT green: ?discover=true listed {} modules \
                         (expected ≥1 — the testing.__tests__ sentinel ships with the \
                         bundle; zero means the discovery walk found nothing, the \
                         empty-suite trap)",
                        modules.len()
                    )));
                }
                let run = crate::client::webdev::testing_run(api, project, &serde_json::json!({}))
                    .await?;
                let passed = run.get("passed").and_then(Value::as_i64).unwrap_or(0);
                let failed = run.get("failed").and_then(Value::as_i64).unwrap_or(-1);
                let errors = run.get("errors").and_then(Value::as_i64).unwrap_or(-1);
                if failed != 0 || errors != 0 || passed < 1 {
                    return Err(CoreError::Internal(format!(
                        "testing smoke run is NOT green: passed={passed} failed={failed} \
                         errors={errors} (the testing.__tests__ sentinel must pass)"
                    )));
                }
                steps.push(CheckResult {
                    name: "testing".into(),
                    status: CheckStatus::Ok,
                    detail: format!(
                        "framework live: {} module(s) discovered, smoke run {passed} passed",
                        modules.len()
                    ),
                    hint: None,
                });
            }
        }

        // 7. CHECKOUT — every ENABLED project into <dir>/<project>.
        //    An existing target SKIPS that project (the re-checkout
        //    clobber refusal is the checkout's own contract — adopt's
        //    re-run idempotency means leaving it alone).
        if let Some(dir) = &opts.checkout {
            let page = api
                .projects(&crate::client::query::ListQuery::default())
                .await?;
            let mut checked: Vec<String> = Vec::new();
            let mut skipped: Vec<String> = Vec::new();
            for record in page.items.iter().filter(|record| record.enabled) {
                let target = dir.join(&record.name);
                if target.exists() {
                    skipped.push(record.name.clone());
                    continue;
                }
                crate::actions::workspace::workspace_checkout(
                    api,
                    &record.name,
                    &target,
                    profile_name,
                    true,
                )
                .await?;
                checked.push(record.name.clone());
            }
            let mut detail = if checked.is_empty() && skipped.is_empty() {
                "no enabled projects on the gateway".to_string()
            } else {
                format!("{} into {}", checked.len(), dir.display())
            };
            if !skipped.is_empty() {
                detail.push_str(&format!(
                    ", {} skipped (already checked out)",
                    skipped.len()
                ));
            }
            steps.push(CheckResult {
                name: "checkout".into(),
                status: CheckStatus::Ok,
                detail,
                hint: None,
            });
        }

        // 8. BAKE — a roaming gwbk AFTER everything landed: a just
        //    reset restored from this file keeps the key, the wiring,
        //    and the routes (live-proven, ADOPT-RESEARCH §--bake).
        if let Some(file) = &opts.bake {
            crate::actions::backup::backup_download(
                api,
                Some(file),
                profile_name,
                crate::client::backup::BackupType::Roaming,
            )
            .await?;
            steps.push(CheckResult {
                name: "bake".into(),
                status: CheckStatus::Ok,
                detail: format!("roaming gwbk at {}", file.display()),
                hint: None,
            });
        }
    }

    Ok(AdoptResult {
        key_name: opts.key_name.clone(),
        token: exposed,
        stored,
        steps,
    })
}

/// How many times the write probe is retried on a 403 right after the
/// permissions PUT, and how long to wait between tries (WR-01: the gateway
/// applies the new permission lists asynchronously; the live capture waits
/// 3 s after every PUT).
const WRITE_PROBE_RETRIES: u32 = 4;
const WRITE_PROBE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// Run the write probe, retrying a 403 up to [`WRITE_PROBE_RETRIES`] times
/// when `retry` is set (the permissions step just wrote). Any other outcome
/// ends the loop at once.
async fn probe_write_with_retry<F, Fut>(
    retry: bool,
    delay: std::time::Duration,
    mut probe: F,
) -> Result<(), CoreError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), CoreError>>,
{
    let mut outcome = probe().await;
    if retry {
        for _ in 0..WRITE_PROBE_RETRIES {
            if !matches!(outcome, Err(CoreError::Auth { status: 403, .. })) {
                break;
            }
            tokio::time::sleep(delay).await;
            outcome = probe().await;
        }
    }
    outcome
}

/// Map the write probe's outcome to the probe row's detail, or to the error
/// plus whether the minted token should be KEPT before that error leaves.
/// The token is kept when the key itself is fine and only the gateway's
/// write permissions are in the way (the re-run repairs in place); it is not
/// kept when the key is the wrong one (a `--read-only` key that can write).
fn write_probe_verdict(
    read_only: bool,
    write_probe: Result<(), CoreError>,
    via: &str,
    write_names: &str,
    key_name: &str,
) -> Result<String, (CoreError, bool)> {
    match (read_only, write_probe) {
        (false, Ok(())) => Ok(format!(
            "gateway-info answered 200 and scan/projects 2xx with {via}: the key reads and writes"
        )),
        (false, Err(CoreError::Auth { status: 403, .. })) => Err((
            CoreError::Internal(format!(
                "the key reads but cannot write: scan/projects answered 403 — \
                 the gateway's writePermissions does not admit the write levels \
                 ({write_names}); check Platform > Security > General Settings > \
                 Gateway Write Permissions"
            )),
            true,
        )),
        (true, Err(CoreError::Auth { status: 403, .. })) => Ok(format!(
            "gateway-info answered 200 and scan/projects answered 403 with {via}: the key is read-only"
        )),
        (true, Ok(())) => Err((
            CoreError::Internal(format!(
                "the key {key_name:?} can write although --read-only was asked; writePermissions \
                 admits it: run ign adopt --restrict-write with the administrator profile, \
                 delete the key {key_name:?} in the gateway (Platform > Security > API Keys) \
                 and re-run with a --key-name of its own (a key minted by this run is not \
                 stored: its plaintext cannot be recovered)"
            )),
            false,
        )),
        (_, Err(err)) => Err((err, true)),
    }
}

/// Persist a key that was minted by this run but failed its probe, then hand
/// back the error. `persist` returns whether the token was kept. A token
/// that is not kept (or a run that minted nothing) leaves the error
/// untouched. An `Internal` error gains a note about what happened to the
/// key; other variants keep their type (their exit code is a contract).
fn keep_minted_token(
    err: CoreError,
    key_name: &str,
    minted: Option<&str>,
    keep_token: bool,
    persist: impl FnOnce(&str) -> bool,
) -> CoreError {
    let Some(token) = minted.filter(|_| keep_token) else {
        return err;
    };
    let note = if persist(token) {
        "the key minted by this run was saved to the keyring and the profile points \
         at it: fix the cause and re-run adopt to finish"
            .to_string()
    } else {
        format!(
            "the key minted by this run could not be saved (no usable keyring) and its \
             plaintext cannot be recovered: delete the key {key_name:?} in the gateway \
             (Platform > Security > API Keys) and re-run adopt"
        )
    };
    match err {
        CoreError::Internal(message) => CoreError::Internal(format!("{message}; {note}")),
        other => {
            tracing::warn!("{note}");
            other
        }
    }
}

/// Keyring-only persistence for the failure path: the env fallback would
/// have to print the plaintext inside an error, which the secret discipline
/// forbids. `true` only when the keyring holds the token AND the profile
/// points at it.
fn persist_after_failed_probe(config_path: &Path, profile_name: &str, token: &str) -> bool {
    if let Err(err) = KeyringStore.set(profile_name, &Secret::new(token.to_string())) {
        tracing::debug!(error = %err, "keyring unavailable; minted key not kept");
        return false;
    }
    rewrite_profile_auth(
        config_path,
        profile_name,
        AuthRef::Keyring {
            keyring: format!("profile:{profile_name}"),
        },
    )
    .is_ok()
}

/// `--read-only` mints a key that is proven refused write, so it must not
/// share the default name with the working write key (WR-05): the found-key
/// check would accept that key (its levels are a superset), the probe would
/// then see it write, and the failure text would point at the live deploy
/// key. Pure; runs before any login.
fn check_read_only_key_name(opts: &AdoptOptions) -> Result<(), CoreError> {
    if opts.read_only && opts.key_name == DEFAULT_KEY_NAME {
        return Err(CoreError::InvalidInput {
            reason: format!(
                "--read-only needs its own --key-name: {:?} is the default name of the \
                 working write key, so a read-only key cannot live under it; pass a \
                 distinct name, e.g. --key-name ign-ci-check",
                opts.key_name
            ),
        });
    }
    Ok(())
}

/// Steps 2's mint: generate the key material, create the resource,
/// verify the gateway's own verdict. The plaintext rides the return
/// value to the ONE exposure site (persist / result).
async fn mint(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
    key_name: &str,
    levels: &[Value],
) -> Result<String, CoreError> {
    let generated = generate_api_key_via_session(flow, session).await?;
    if generated.key.is_empty() || generated.hash.is_empty() {
        return Err(CoreError::Internal(
            "api-token generate answered an empty key or hash".into(),
        ));
    }
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the unix epoch")
        .as_millis() as i64;
    let body = build_token_create_body(key_name, levels, &generated.hash, timestamp_ms);
    let answer: ResourceMutationWire = create_api_token_via_session(flow, session, &body).await?;
    if !answer.success
        || !answer.changes.iter().any(|change| {
            change.name == key_name && change.kind == crate::client::adopt::API_TOKEN_TYPE
        })
    {
        return Err(CoreError::Internal(format!(
            "api-token create refused: success={} problem={:?}",
            answer.success, answer.problem
        )));
    }
    Ok(format!("{key_name}:{}", generated.key))
}

/// Rewrite one profile's `auth` in the config file (0600 re-asserted
/// by the save path — the webdev-secret precedent).
fn rewrite_profile_auth(
    config_path: &Path,
    profile_name: &str,
    auth: AuthRef,
) -> Result<(), CoreError> {
    let mut config = config::load(config_path)?;
    let profile = config.profiles.get_mut(profile_name).ok_or_else(|| {
        CoreError::Internal(format!(
            "profile {profile_name:?} vanished from the config mid-adopt"
        ))
    })?;
    profile.auth = auth;
    config::save(config_path, &config)
}

/// The env-suffix rule (config::secret's mapping, verbatim import —
/// one home, never restated): profile uppercased, non-alphanumeric
/// → `_`.
fn super_profile_env_suffix(profile: &str) -> String {
    config::secret::profile_env_suffix(profile)
}

#[cfg(test)]
mod tests {
    use super::{
        AdoptOptions, DEFAULT_KEY_NAME, DEFAULT_LEVEL, WRITE_ADMISSION, WRITE_PROBE_RETRIES,
        WriteAdmission, check_read_only_key_name, keep_minted_token, key_levels,
        probe_write_with_retry, resolve_write_levels, resolve_write_levels_for,
        write_probe_verdict,
    };
    use crate::error::CoreError;
    use std::cell::Cell;
    use std::time::Duration;

    /// The locked defaults — Administrator, the CLI-owned key name.
    #[test]
    fn defaults_are_the_administrator_posture() {
        assert_eq!(DEFAULT_KEY_NAME, "ign-cli");
        assert_eq!(DEFAULT_LEVEL, &["Authenticated", "Roles", "Administrator"]);
        // The capture's decision (25-LIVE-CAPTURES.md, G-25-11): dedicated-nested.
        assert_eq!(
            WRITE_ADMISSION,
            WriteAdmission::Dedicated(&["Authenticated", "IgnCliWrite"])
        );
    }

    fn path(text: &str) -> Vec<String> {
        text.split('/').map(str::to_string).collect()
    }

    fn opts() -> AdoptOptions {
        AdoptOptions {
            username: "admin".into(),
            key_name: DEFAULT_KEY_NAME.into(),
            level: DEFAULT_LEVEL.iter().map(|s| s.to_string()).collect(),
            project: None,
            testing: false,
            checkout: None,
            bake: None,
            write_levels: Vec::new(),
            read_only: false,
            restrict_write: false,
        }
    }

    fn invalid(err: CoreError) -> String {
        match err {
            CoreError::InvalidInput { reason } => reason,
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn resolve_write_levels_follows_the_decision() {
        let base = opts();
        // The default follows the decision.
        assert_eq!(
            resolve_write_levels_for(&base, WriteAdmission::NestedExact).expect("ok"),
            vec![path("Authenticated/Roles/Administrator")]
        );
        assert_eq!(
            resolve_write_levels_for(
                &base,
                WriteAdmission::Dedicated(&["Authenticated", "IgnCliWrite"])
            )
            .expect("ok"),
            vec![path("Authenticated/IgnCliWrite")]
        );
        assert_eq!(
            resolve_write_levels(&base).expect("ok"),
            vec![path("Authenticated/IgnCliWrite")],
            "the shipped default is the recorded decision"
        );

        // Explicit levels are returned as given.
        let mut explicit = opts();
        explicit.write_levels = vec![path("Authenticated/Roles/Deployer"), path("Ops/Write")];
        assert_eq!(
            resolve_write_levels(&explicit).expect("ok"),
            explicit.write_levels
        );

        // A bare Authenticated or Public write level is refused, any case.
        for bad in ["Authenticated", "public", "AUTHENTICATED"] {
            let mut o = opts();
            o.write_levels = vec![path(bad)];
            let reason = invalid(resolve_write_levels(&o).expect_err("refused"));
            assert!(reason.contains("--write-level"), "{reason}");
            assert!(reason.contains("every API key"), "{reason}");
        }
        // Under nested-exact the default must not become a bare Authenticated.
        let mut bare_level = opts();
        bare_level.level = path("Authenticated");
        invalid(
            resolve_write_levels_for(&bare_level, WriteAdmission::NestedExact)
                .expect_err("refused"),
        );

        // An empty segment is refused.
        let mut empty = opts();
        empty.write_levels = vec![path("Authenticated//X")];
        invalid(resolve_write_levels(&empty).expect_err("refused"));

        // read_only gives an empty list; combined with the others, InvalidInput.
        let mut ro = opts();
        ro.read_only = true;
        assert!(resolve_write_levels(&ro).expect("ok").is_empty());
        let mut ro_levels = ro.clone();
        ro_levels.write_levels = vec![path("A/B")];
        let reason = invalid(resolve_write_levels(&ro_levels).expect_err("refused"));
        assert!(reason.contains("--read-only"), "{reason}");
        let mut ro_restrict = ro;
        ro_restrict.restrict_write = true;
        invalid(resolve_write_levels(&ro_restrict).expect_err("refused"));
    }

    #[test]
    fn key_levels_dedups_and_keeps_order() {
        let mut o = opts();
        o.level = path("Authenticated/Roles/IgnCheckRead");
        let writes = vec![
            path("Authenticated/IgnCliWrite"),
            path("Authenticated/Roles/IgnCheckRead"),
            path("Authenticated/IgnCliWrite"),
        ];
        assert_eq!(
            key_levels(&o, &writes),
            vec![
                path("Authenticated/Roles/IgnCheckRead"),
                path("Authenticated/IgnCliWrite")
            ]
        );
        assert_eq!(
            key_levels(&o, &[]),
            vec![path("Authenticated/Roles/IgnCheckRead")]
        );
    }

    fn forbidden() -> CoreError {
        CoreError::Auth {
            status: 403,
            endpoint: None,
        }
    }

    /// WR-01: a stale 403 straight after the permissions PUT is retried and
    /// the eventual success is returned.
    #[tokio::test]
    async fn write_probe_retries_a_stale_403_then_succeeds() {
        let calls = Cell::new(0u32);
        let outcome = probe_write_with_retry(true, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            let answer = if calls.get() < 3 {
                Err(forbidden())
            } else {
                Ok(())
            };
            async move { answer }
        })
        .await;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(calls.get(), 3);
    }

    /// WR-01: a persistent 403 is retried a bounded number of times, then
    /// reported as the 403 it is.
    #[tokio::test]
    async fn write_probe_gives_up_after_the_bounded_retries() {
        let calls = Cell::new(0u32);
        let outcome = probe_write_with_retry(true, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            async { Err(forbidden()) }
        })
        .await;
        assert!(matches!(outcome, Err(CoreError::Auth { status: 403, .. })));
        assert_eq!(calls.get(), 1 + WRITE_PROBE_RETRIES);
    }

    /// WR-01: no retry when the permissions step wrote nothing (and for
    /// `--read-only`, where the 403 is the wanted answer), and never for a
    /// non-403 error.
    #[tokio::test]
    async fn write_probe_does_not_retry_without_a_fresh_permissions_write() {
        let calls = Cell::new(0u32);
        let outcome = probe_write_with_retry(false, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            async { Err(forbidden()) }
        })
        .await;
        assert!(outcome.is_err());
        assert_eq!(calls.get(), 1, "retry off: exactly one probe");

        let calls = Cell::new(0u32);
        let outcome = probe_write_with_retry(true, Duration::ZERO, || {
            calls.set(calls.get() + 1);
            async { Err(CoreError::Internal("boom".into())) }
        })
        .await;
        assert!(matches!(outcome, Err(CoreError::Internal(_))));
        assert_eq!(calls.get(), 1, "a non-403 error is not retried");
    }

    /// WR-01: which probe failures keep the minted token. A key that only
    /// lacks write permission is kept (the re-run repairs it); a
    /// `--read-only` key that can write is not.
    #[test]
    fn write_probe_verdict_says_when_to_keep_the_minted_token() {
        let (err, keep) = write_probe_verdict(false, Err(forbidden()), "via", "A/B", "ign-cli")
            .expect_err("a write refusal is an error");
        assert!(
            keep,
            "the key is fine; only write permissions are in the way"
        );
        assert!(matches!(err, CoreError::Internal(_)));

        let (_, keep) = write_probe_verdict(
            false,
            Err(CoreError::Internal("net".into())),
            "via",
            "A/B",
            "ign-cli",
        )
        .expect_err("other errors pass through");
        assert!(keep);

        let (err, keep) = write_probe_verdict(true, Ok(()), "via", "", "ign-ci-check")
            .expect_err("a read-only key that writes is an error");
        assert!(!keep, "that key is the wrong one; never persist it");
        assert!(
            err.to_string().contains("ign-ci-check"),
            "names the key: {err}"
        );

        assert!(write_probe_verdict(true, Err(forbidden()), "via", "", "k").is_ok());
        assert!(write_probe_verdict(false, Ok(()), "via", "A/B", "k").is_ok());
    }

    /// WR-01: persist-before-error. The token is handed to `persist` before
    /// the error is returned, and the error says what happened to the key.
    #[test]
    fn a_minted_key_is_persisted_before_the_probe_error_leaves() {
        let persisted = Cell::new(None::<String>);
        let err = keep_minted_token(
            CoreError::Internal("the key reads but cannot write".into()),
            "ign-cli",
            Some("ign-cli:secret"),
            true,
            |token| {
                persisted.set(Some(token.to_string()));
                true
            },
        );
        assert_eq!(persisted.take().as_deref(), Some("ign-cli:secret"));
        let text = err.to_string();
        assert!(text.contains("the key reads but cannot write"), "{text}");
        assert!(text.contains("saved to the keyring"), "{text}");
        assert!(
            !text.contains("secret"),
            "the plaintext never rides an error"
        );

        // No keyring: the error says the key is lost and names it.
        let err = keep_minted_token(
            CoreError::Internal("x".into()),
            "ign-cli",
            Some("ign-cli:secret"),
            true,
            |_| false,
        );
        let text = err.to_string();
        assert!(text.contains("delete the key \"ign-cli\""), "{text}");
        assert!(!text.contains("secret"));

        // A non-Internal error keeps its type (exit-code contract).
        let err = keep_minted_token(forbidden(), "ign-cli", Some("t"), true, |_| true);
        assert!(matches!(err, CoreError::Auth { status: 403, .. }));
    }

    /// WR-01: nothing is persisted when no key was minted, or when the
    /// verdict says the key must not be kept.
    #[test]
    fn nothing_is_persisted_for_found_or_unkeepable_keys() {
        for (minted, keep) in [(None, true), (Some("t"), false)] {
            let called = Cell::new(false);
            let err = keep_minted_token(CoreError::Internal("e".into()), "k", minted, keep, |_| {
                called.set(true);
                true
            });
            assert!(!called.get());
            assert_eq!(err.to_string(), CoreError::Internal("e".into()).to_string());
        }
    }

    /// WR-05: `--read-only` under the default key name is refused as
    /// `invalid_input`, and the message names the key.
    #[test]
    fn read_only_refuses_the_default_key_name() {
        let mut o = opts();
        o.read_only = true;
        let reason = invalid(check_read_only_key_name(&o).expect_err("refused"));
        assert!(reason.contains("--key-name"), "{reason}");
        assert!(reason.contains(&format!("{:?}", o.key_name)), "{reason}");

        o.key_name = "ign-ci-check".into();
        assert!(check_read_only_key_name(&o).is_ok());

        // The write path keeps the default name.
        assert!(check_read_only_key_name(&opts()).is_ok());
    }
}
