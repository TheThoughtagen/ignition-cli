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
//!    singleton: ADD the key's level root to read+write AnyOf lists
//!    (never remove — the replace-semantics + []-is-Public pitfalls),
//!    signature riding verbatim.
//! 4. `probe` — gateway-info with the freshly minted name:key; the
//!    live end-to-end proof (mint-to-200 same second, live-observed).
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
use crate::client::adopt::put_security_properties_via_session;
use crate::client::adopt::security_properties_via_session;
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
            let token = mint(&flow, &session, &opts.key_name, &level_path).await?;
            steps.push(CheckResult {
                name: "key".into(),
                status: CheckStatus::Ok,
                detail: format!(
                    "minted API key {:?} at {} (secure connections off)",
                    opts.key_name,
                    opts.level.join("/")
                ),
                hint: None,
            });
            Some(token)
        }
    };

    // 3. PERMISSIONS — add-if-missing in read+write (merge-only; the
    //    PUT replaces the whole singleton, the signature rides
    //    verbatim). Runs in BOTH paths: a found key can still be
    //    locked out by permissions (doctor's part-2 finding).
    let singleton = security_properties_via_session(&flow, &session).await?;
    let mut config = singleton.config.clone();
    let level = level_tree(&level_path);
    let changed_read = merge_level_into(&mut config, "readPermissions", &level);
    let changed_write = merge_level_into(&mut config, "writePermissions", &level);
    if changed_read || changed_write {
        let body = build_security_put_body(&singleton, &config);
        let answer = put_security_properties_via_session(&flow, &session, &body).await?;
        if !answer.success {
            return Err(CoreError::Internal(format!(
                "security-properties write refused: {:?}",
                answer.problem
            )));
        }
        steps.push(CheckResult {
            name: "permissions".into(),
            status: CheckStatus::Ok,
            detail: format!(
                "wired {} into gateway read/write permissions ({}added)",
                opts.level.join("/"),
                if changed_read && changed_write {
                    "both "
                } else {
                    ""
                }
            ),
            hint: None,
        });
    } else {
        steps.push(CheckResult {
            name: "permissions".into(),
            status: CheckStatus::Skip,
            detail: format!(
                "gateway permissions already admit {} — nothing written",
                opts.level.join("/")
            ),
            hint: None,
        });
    }

    // 4. PROBE — only provable with a plaintext in hand (the mint
    //    path); a pre-existing key is probed by the profile's own
    //    credential elsewhere (`ign doctor`).
    let mut token_for_persist: Option<String> = None;
    match minted {
        Some(token) => {
            let url: url::Url = base_url
                .parse()
                .map_err(|err| CoreError::Internal(format!("invalid gateway URL: {err}")))?;
            let probe = crate::session::Session::for_url(
                url,
                Some(Credential::Token(Secret::new(token.clone()))),
                true,
            )?;
            probe.api().gateway_info().await?;
            steps.push(CheckResult {
                name: "probe".into(),
                status: CheckStatus::Ok,
                detail: "gateway-info answered 200 with the fresh name:key — the key works".into(),
                hint: None,
            });
            token_for_persist = Some(token);
        }
        None => steps.push(CheckResult {
            name: "probe".into(),
            status: CheckStatus::Skip,
            detail: "key pre-existed — probe with `ign doctor` on this profile".into(),
            hint: None,
        }),
    }

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

/// Steps 2's mint: generate the key material, create the resource,
/// verify the gateway's own verdict. The plaintext rides the return
/// value to the ONE exposure site (persist / result).
async fn mint(
    flow: &IdpLoginFlow,
    session: &GatewaySession,
    key_name: &str,
    level_path: &[&str],
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
    let body = build_token_create_body(
        key_name,
        &level_tree(level_path),
        &generated.hash,
        timestamp_ms,
    );
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
    use super::{DEFAULT_KEY_NAME, DEFAULT_LEVEL};

    /// The locked defaults — Administrator, the CLI-owned key name.
    #[test]
    fn defaults_are_the_administrator_posture() {
        assert_eq!(DEFAULT_KEY_NAME, "ign-cli");
        assert_eq!(DEFAULT_LEVEL, &["Authenticated", "Roles", "Administrator"]);
    }
}
