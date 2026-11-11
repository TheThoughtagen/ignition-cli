//! The WebDev deploy/status actions (05-03, WEB-01 + WEB-02) — the
//! hinge layer every tag command in 05-04..06 rides.
//!
//! `webdev_deploy` installs the embedded route bundle
//! ([`crate::webdev`], 05-01) into the dedicated project through the
//! 03-02 import machinery with `overwrite=true` — the CLI owns the
//! project wholesale, so replace-not-merge is CORRECT here (research
//! deploy guidance) and deploy is deliberately NOT `--yes`-guarded.
//! The project is born from the first deploy zip: NO pre-flight
//! create (Pitfall 10's one-shot "resource already exists" quirk).
//!
//! `webdev_status` probes every route's `version` action and reports
//! the per-route matrix ({present, absent, unlicensed, auth_gated,
//! secret_mismatch, version_mismatch}) — a READ: exit 0 whenever the
//! sweep completes, degradation is data (the doctor precedent).
//!
//! `webdev_precondition` is the cheap refusal every
//! WebDev-DEPENDENT command runs first (05-04+): absent routes or a
//! version mismatch refuse exit 6 naming `ign webdev deploy` — the
//! roadmap's actionable-error criterion, no auto-upgrade magic.
//!
//! The scriptExec secret lifecycle lives HERE: deploy generates a
//! 32-byte hex secret from `/dev/urandom` (zero-dep — the workspace
//! has no `rand`; unix-only is fine, no Windows CI is locked),
//! persists it in the profile config at 0600 (the ONE
//! value-carrying exception on [`crate::config::Profile`], documented
//! there), and bakes it into the route zip BEFORE upload. The secret
//! NEVER appears in any action result, log, or JSON envelope — the
//! redaction test below pins that.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::webdev::{self as seam, RouteProbe};
use crate::config;
use crate::error::{CoreError, TAG_DELETE_DROP_REFUSAL_PREFIX};
use crate::webdev as bundle;

/// The secret-bearing scriptExec route name (deploy/status append it
/// only when explicitly requested / configured). `pub(crate)` since
/// 07-03: the script action reuses the EXACT spelling (one constant,
/// no drift).
pub(crate) const SCRIPT_EXEC_ROUTE: &str = "scriptExec";

/// The header the scriptExec gate compares (case-insensitive
/// server-side; the CLI sends the canonical form). `pub(crate)` since
/// 07-03 — the script action's calls carry it too.
pub(crate) const SECRET_HEADER: &str = "X-Ignition-CLI-Secret";

/// The tagDelete route name (26-17), appended to `routes` when it ships.
pub(crate) const TAG_DELETE_ROUTE: &str = crate::error::TAG_DELETE_ROUTE_NAME;

/// A request to install the tagDelete route (26-17, D-13) with the deploy.
/// The provider allowlist is the union of the repos' `deploy/project.yaml`
/// `tag_providers`, in repo order.
#[derive(Debug, Clone)]
pub struct TagDeleteRequest {
    /// The project repos whose `deploy/project.yaml` name the owned
    /// providers (at least one).
    pub repos: Vec<PathBuf>,
    /// The WebDev user source the route authenticates against.
    pub user_source: String,
    /// The role WebDev requires of the caller.
    pub role: String,
    /// Regenerate the route secret even when one is stored.
    pub rotate_secret: bool,
    /// The route caller's credential (user, password), the profile's env
    /// pair, when set. It is used for one `version` handshake that reads the
    /// live allowlist before the install replaces it (26-REVIEW-D WR-D3).
    /// `None`: the live allowlist cannot be read, and the install warns.
    pub caller: Option<(config::Secret, config::Secret)>,
}

/// `ign webdev deploy` result — ALL keys always (the agent shape);
/// the import answer rides as the opaque-success object verbatim.
/// The secret appears in NONE of them (redaction).
#[derive(Debug, Serialize)]
pub struct WebdevDeployResult {
    /// The project the bundle deployed into.
    pub project: String,
    /// Route folder names deployed, manifest order (+ scriptExec when
    /// it shipped).
    pub routes: Vec<String>,
    /// Whether scriptExec rode the deploy.
    pub script_exec: bool,
    /// Whether a NEW secret was generated and persisted (first
    /// scriptExec deploy or `--rotate-secret`).
    pub secret_rotated: bool,
    /// The import endpoint's opaque answer (the 03-02
    /// `{"status":"success"}`-normalized object).
    pub import: serde_json::Value,
    /// Whether the tagDelete route rode the deploy (26-17).
    pub tag_delete: bool,
    /// Whether a NEW tagDelete secret was generated and persisted (first
    /// tag-delete deploy or its rotate).
    pub tag_delete_secret_rotated: bool,
    /// The tagDelete route's provider allowlist (empty when it did not
    /// ship).
    pub tag_delete_providers: Vec<String>,
    /// Warnings for the operator (26-REVIEW-D WR-D4: the tagDelete route
    /// and its stored secret were removed). Empty when there are none.
    pub warnings: Vec<String>,
}

/// One route's status-sweep row — ALL keys always.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteStatusRow {
    /// Route folder name.
    pub route: String,
    /// The per-route matrix verdict.
    pub status: RouteStatus,
    /// The route's answered `routeVersion` (absent states → null).
    pub deployed_version: Option<String>,
    /// The embedded bundle version this CLI expects (always known).
    pub expected_version: Option<String>,
}

/// The per-route status matrix (WebDev-dependent commands refuse on
/// the same discrimination; status reports it as data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStatus {
    /// Deployed, answering, version matches the embedded bundle.
    Present,
    /// 405 — not deployed (the 8.3 absent marker).
    Absent,
    /// 402 — the WebDev module is unlicensed (trial-expired gateway).
    Unlicensed,
    /// 401/403 — present but rejecting the credential.
    AuthGated,
    /// The scriptExec gate refused the configured secret (deployed
    /// elsewhere / stale) — redeploy or `--rotate-secret`.
    SecretMismatch,
    /// Deployed but the handshake version differs from the bundle's.
    VersionMismatch,
}

/// `ign webdev status` result — ALL keys always.
#[derive(Debug, Serialize)]
pub struct WebdevStatusResult {
    /// The probed project.
    pub project: String,
    /// One row per always-on route (+ scriptExec when a secret is
    /// configured — unprobed otherwise, per the plan's conditional).
    pub routes: Vec<RouteStatusRow>,
    /// True only when every ALWAYS-ON route is present with matching
    /// versions (scriptExec never gates `ok` — it deploys on explicit
    /// request only).
    pub ok: bool,
}

/// `ign webdev deploy` — pack the embedded bundle, import it
/// overwrite-style, own the secret lifecycle.
///
/// Secret rules (the plan's LOCKED posture): a fresh secret is
/// generated + persisted 0600 when (a) `--with-script-exec` finds no
/// stored secret, or (b) `--rotate-secret` asks regardless; a plain
/// scriptExec deploy reuses the stored secret unchanged. Persisting
/// happens BEFORE the upload: a failed import then leaves a stored
/// secret the NEXT deploy packs as-is (self-healing), and a broken
/// config store refuses before any gateway I/O.
pub async fn webdev_deploy(
    api: &dyn GatewayApi,
    project: &str,
    with_script_exec: bool,
    rotate_secret: bool,
    config_path: &Path,
    profile_name: &str,
    with_testing: bool,
) -> Result<WebdevDeployResult, CoreError> {
    webdev_deploy_with(
        api,
        project,
        with_script_exec,
        rotate_secret,
        config_path,
        profile_name,
        with_testing,
        None,
    )
    .await
}

/// [`webdev_deploy`] plus, with `tag_delete`, the tagDelete route (26-17,
/// D-13) in the same import. The allowlist is the repo's
/// `deploy/project.yaml` `tag_providers`; the route secret follows the
/// scriptExec rules independently (generated when none is stored or on its
/// rotate, persisted 0600 as `webdev_tag_delete_secret` before the upload,
/// never output). Every install value is checked before any secret is
/// stored and before any gateway request.
#[allow(clippy::too_many_arguments)]
pub async fn webdev_deploy_with(
    api: &dyn GatewayApi,
    project: &str,
    with_script_exec: bool,
    rotate_secret: bool,
    config_path: &Path,
    profile_name: &str,
    with_testing: bool,
    tag_delete: Option<TagDeleteRequest>,
) -> Result<WebdevDeployResult, CoreError> {
    // Secret lifecycle first — refuse on an unwritable config store
    // BEFORE touching the gateway.
    let mut config = config::load(config_path)?;
    let profile = config.profiles.get(profile_name);
    let existing = profile.and_then(|profile| profile.webdev_secret.clone());
    let existing_tag_delete = profile.and_then(|profile| profile.webdev_tag_delete_secret.clone());

    let (pack_secret, secret_rotated) = if rotate_secret || (with_script_exec && existing.is_none())
    {
        (Some(generate_secret()?), true)
    } else if with_script_exec {
        (existing, false)
    } else {
        (None, false)
    };

    let mut warnings = Vec::new();

    // The tagDelete install: allowlist from the repos, secret stored or
    // fresh, every value validated before anything is persisted.
    let (install, tag_delete_secret_rotated) = match &tag_delete {
        None => (None, false),
        Some(request) => {
            // The route secret travels inside the import: only over https
            // or to a loopback host, with `ssl_verify` on for a remote one,
            // the route client's own rule (26-REVIEW WR-07). The URL is the
            // one the import's client sends to (26-REVIEW-D WR-D2), not a
            // fresh read of `IGNITION_URL`; a double without one falls back
            // to the stored profile.
            let transport = api.gateway_transport().or_else(|| {
                config
                    .profiles
                    .get(profile_name)
                    .map(|profile| (profile.url.clone(), profile.ssl_verify))
            });
            if let Some((url, ssl_verify)) = &transport
                && let Some(refusal) =
                    crate::deploy::tag_delete::transport_refusal(url, *ssl_verify)
            {
                return Err(CoreError::TagDeleteNotConfigured {
                    profile: profile_name.to_string(),
                    missing: vec![refusal],
                });
            }
            let mut providers: Vec<String> = Vec::new();
            for repo in &request.repos {
                for provider in crate::deploy::projects::load_project_file(repo)?.tag_providers {
                    if !providers.contains(&provider) {
                        providers.push(provider);
                    }
                }
            }
            let (secret, rotated) = match existing_tag_delete.clone() {
                Some(secret) if !request.rotate_secret => (secret, false),
                _ => (generate_secret()?, true),
            };
            let install = seam::TagDeleteInstall {
                secret,
                user_source: request.user_source.clone(),
                role: request.role.clone(),
                providers,
            };
            seam::validate_tag_delete_install(&install)?;
            // 26-REVIEW-D WR-D3: the overwrite replaces the live allowlist.
            // With a stored secret a route may be installed: read its
            // allowlist once and refuse an install that would drop a
            // provider allowed now. Unreadable: install and warn.
            if let Some(stored) = &existing_tag_delete {
                match read_live_allowlist(
                    profile_name,
                    transport.as_ref(),
                    project,
                    request.caller.clone(),
                    stored,
                )
                .await
                {
                    LiveAllowlist::Read(live) => {
                        refuse_dropped_providers(project, &live, &install.providers)?;
                    }
                    LiveAllowlist::Absent => {}
                    LiveAllowlist::Unreadable(reason) => warnings.push(format!(
                        "could not read the live tagDelete allowlist on project {project} \
                         ({reason}); the install replaced it with [{}], so check that no \
                         other repo's providers were dropped",
                        install.providers.join(", ")
                    )),
                }
            }
            (Some(install), rotated)
        }
    };

    // 26-REVIEW-D WR-D4: the overwrite import replaces the project, so a
    // deploy into the route's project without the tag-delete request drops
    // the tagDelete route. The stored secret goes with it, before the
    // upload, so the profile reads not configured (a run with removals then
    // refuses `tag_delete_not_configured`, a rollback reports no route)
    // instead of waiting on a route that is gone. The deploy is not refused:
    // adopt and the TUI deploy this way.
    let tag_delete_removed =
        tag_delete.is_none() && project == seam::DEFAULT_PROJECT && existing_tag_delete.is_some();
    if tag_delete_removed {
        warnings.push(format!(
            "tagDelete route removed from project {project}: this deploy did not include \
             --with-tag-delete, so the overwrite import dropped it; the stored tagDelete \
             secret was removed from profile {profile_name:?}, and tag-owning runs refuse \
             tag_delete_not_configured until `ign webdev deploy --with-tag-delete` reinstalls it"
        ));
    }

    if secret_rotated || tag_delete_secret_rotated || tag_delete_removed {
        let entry = config.profiles.get_mut(profile_name).ok_or_else(|| {
            CoreError::Internal(format!(
                "profile {profile_name:?} vanished from the config between dispatch and deploy"
            ))
        })?;
        if secret_rotated {
            entry.webdev_secret = pack_secret.clone();
        }
        if tag_delete_secret_rotated {
            entry.webdev_tag_delete_secret = install.as_ref().map(|install| install.secret.clone());
        }
        if tag_delete_removed {
            entry.webdev_tag_delete_secret = None;
        }
        config::save(config_path, &config)?; // re-asserts 0600
    }

    // Pack (scriptExec only when flagged — build_deploy_zip's
    // fail-closed guard covers the (true, None) bug case) and import
    // overwrite=true through the 03-02 machinery. NO pre-flight
    // project create (Pitfall 10).
    let zip = seam::build_deploy_zip_with(
        project,
        with_script_exec,
        pack_secret.as_deref(),
        with_testing,
        install.as_ref(),
    )?;
    let mut routes = seam::always_on_routes();
    if with_script_exec {
        routes.push(SCRIPT_EXEC_ROUTE.to_string());
    }
    if install.is_some() {
        routes.push(TAG_DELETE_ROUTE.to_string());
    }
    if with_testing {
        routes.extend(
            crate::webdev::testing::TESTING_ROUTES
                .iter()
                .map(|route| format!("testing/{route}")),
        );
    }
    let import = api.project_import(project, zip, true).await?;

    Ok(WebdevDeployResult {
        project: project.to_string(),
        routes,
        script_exec: with_script_exec,
        secret_rotated,
        import: import.response,
        tag_delete: install.is_some(),
        tag_delete_secret_rotated,
        tag_delete_providers: install.map(|install| install.providers).unwrap_or_default(),
        warnings,
    })
}

/// The live tagDelete allowlist as read before an install (26-REVIEW-D
/// WR-D3).
enum LiveAllowlist {
    /// The route answered its handshake with this allowlist.
    Read(Vec<String>),
    /// The route is not deployed (405): nothing to keep.
    Absent,
    /// The allowlist could not be read; the reason names no secret.
    Unreadable(String),
}

/// Read the live route's allowlist through one `version` handshake with the
/// stored route secret and the caller's credential. Never waits or retries.
async fn read_live_allowlist(
    profile_name: &str,
    transport: Option<&(url::Url, bool)>,
    project: &str,
    caller: Option<(config::Secret, config::Secret)>,
    stored_secret: &str,
) -> LiveAllowlist {
    use crate::deploy::tag_delete::{TagDeleteClient, TagDeleteRoute};
    let Some((url, ssl_verify)) = transport else {
        return LiveAllowlist::Unreadable("no gateway URL is known for the profile".to_string());
    };
    let Some((user, password)) = caller else {
        let (user, password) = config::secret::tag_delete_env_names(profile_name);
        return LiveAllowlist::Unreadable(format!("{user} and {password} are not both set"));
    };
    let client = match TagDeleteClient::new(
        profile_name,
        url,
        *ssl_verify,
        project,
        user,
        password,
        config::Secret::new(stored_secret),
    ) {
        Ok(client) => client,
        Err(err) => return LiveAllowlist::Unreadable(err.code().to_string()),
    };
    match client.handshake().await {
        Ok(handshake) => LiveAllowlist::Read(handshake.providers),
        Err(CoreError::RoutesNotDeployed { .. }) => LiveAllowlist::Absent,
        Err(err) => LiveAllowlist::Unreadable(format!("the handshake answered {}", err.code())),
    }
}

/// Refuse an install whose allowlist `new` lacks a provider of `live`.
fn refuse_dropped_providers(
    project: &str,
    live: &[String],
    new: &[String],
) -> Result<(), CoreError> {
    let dropped: Vec<&str> = live
        .iter()
        .filter(|provider| !new.contains(provider))
        .map(String::as_str)
        .collect();
    if dropped.is_empty() {
        return Ok(());
    }
    Err(CoreError::InvalidInput {
        reason: format!(
            "{TAG_DELETE_DROP_REFUSAL_PREFIX} {} from the live tagDelete allowlist on project \
             {project} (allowed now: {}; this install: {}); nothing was stored or imported. \
             Re-run naming every repo whose providers the route must keep (repeat \
             --tag-delete-repo), or replace the allowlist deliberately: run `ign webdev \
             deploy` without --with-tag-delete first (it removes the route and its stored \
             secret), then install",
            dropped.join(", "),
            live.join(", "),
            new.join(", ")
        ),
    })
}

/// `ign webdev status` — the version-handshake sweep. The 4 always-on
/// routes always ride; scriptExec's version action is probed ONLY
/// when a secret is configured (its header rides along — research OQ4:
/// AuthGated → auth_gated, secret denials → secret_mismatch).
pub async fn webdev_status(
    api: &dyn GatewayApi,
    project: &str,
    secret: Option<&str>,
) -> Result<WebdevStatusResult, CoreError> {
    let mut routes = Vec::new();
    let mut ok = true;
    for route in seam::always_on_routes() {
        let probe = api.webdev_route_probe(project, &route, &[]).await?;
        let row = classify_probe(&route, probe);
        ok &= row.status == RouteStatus::Present;
        routes.push(row);
    }
    if let Some(secret) = secret {
        let probe = api
            .webdev_route_probe(project, SCRIPT_EXEC_ROUTE, &[(SECRET_HEADER, secret)])
            .await?;
        // scriptExec never gates `ok` — it ships on explicit request.
        routes.push(classify_probe(SCRIPT_EXEC_ROUTE, probe));
    }
    Ok(WebdevStatusResult {
        project: project.to_string(),
        routes,
        ok,
    })
}

/// The cheap precondition every WebDev-DEPENDENT command runs first
/// (05-04's tags family onward): probe the canonical `tags` route and
/// refuse with the actionable matrix — absent → `routes_not_deployed`
/// (exit 6, hint names `ign webdev deploy`), version mismatch →
/// `route_version_mismatch` (hint direction-aware: redeploy or update
/// ign), unlicensed → `webdev_unlicensed`. No auto-upgrade magic.
pub async fn webdev_precondition(api: &dyn GatewayApi, project: &str) -> Result<(), CoreError> {
    const ROUTE: &str = "tags";
    let endpoint = seam::route_url(project, ROUTE);
    match api.webdev_route_probe(project, ROUTE, &[]).await? {
        RouteProbe::Present { route_version } if route_version == bundle::ROUTE_BUNDLE_VERSION => {
            Ok(())
        }
        RouteProbe::Present { route_version } => Err(CoreError::RouteVersionMismatch {
            route: ROUTE.to_string(),
            deployed: route_version,
            expected: bundle::ROUTE_BUNDLE_VERSION.to_string(),
            endpoint: Some(endpoint),
        }),
        RouteProbe::Absent => Err(CoreError::RoutesNotDeployed {
            project: project.to_string(),
            route: ROUTE.to_string(),
            endpoint: Some(endpoint),
        }),
        RouteProbe::Unlicensed => Err(CoreError::WebdevUnlicensed {
            endpoint: Some(endpoint),
        }),
        RouteProbe::AuthGated => Err(CoreError::Auth {
            status: 401,
            endpoint: Some(endpoint),
        }),
        RouteProbe::Denied {
            code,
            message,
            traceback,
        } => {
            let mut full = message;
            if let Some(traceback) = traceback {
                full.push_str("\nroute traceback: ");
                full.push_str(&traceback);
            }
            Err(CoreError::WebdevRouteError {
                code,
                message: full,
                endpoint: Some(endpoint),
            })
        }
    }
}

/// Map one probe onto a status row (the matrix the sweep reports and
/// the precondition refuses on).
fn classify_probe(route: &str, probe: RouteProbe) -> RouteStatusRow {
    let expected = bundle::ROUTE_BUNDLE_VERSION;
    match probe {
        RouteProbe::Present { route_version } => {
            let status = if route_version == expected {
                RouteStatus::Present
            } else {
                RouteStatus::VersionMismatch
            };
            RouteStatusRow {
                route: route.to_string(),
                status,
                deployed_version: Some(route_version),
                expected_version: Some(expected.to_string()),
            }
        }
        RouteProbe::Denied { code, .. } => {
            // With the shipped routes the version action's only
            // denials are the secret gate's (the gate runs before
            // dispatch); any OTHER code means the path holds a route
            // this CLI does not recognize — the redeploy advice that
            // version_mismatch carries is the honest fix either way.
            let status = if code == "secret_required" || code == "secret_mismatch" {
                RouteStatus::SecretMismatch
            } else {
                RouteStatus::VersionMismatch
            };
            absent_row(route, status)
        }
        RouteProbe::Absent => absent_row(route, RouteStatus::Absent),
        RouteProbe::Unlicensed => absent_row(route, RouteStatus::Unlicensed),
        RouteProbe::AuthGated => absent_row(route, RouteStatus::AuthGated),
    }
}

/// A row for every probe state that answered no version.
fn absent_row(route: &str, status: RouteStatus) -> RouteStatusRow {
    RouteStatusRow {
        route: route.to_string(),
        status,
        deployed_version: None,
        expected_version: Some(bundle::ROUTE_BUNDLE_VERSION.to_string()),
    }
}

/// 32 random bytes, hex-encoded (64 chars) — via the `getrandom`
/// crate (already in the tree via ring; 0.2 pinned to dedupe).
/// The pre-Windows `/dev/urandom` read is gone with the reversal of
/// the no-Windows decision: getrandom rides the OS CSPRNG everywhere
/// (BCryptGenRandom on Windows, getrandom(2)/urandom on unix).
fn generate_secret() -> Result<String, CoreError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|err| CoreError::Internal(format!("cannot generate the deploy secret: {err}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::{
        RouteProbe, RouteStatus, TagDeleteRequest, WebdevDeployResult, generate_secret,
        webdev_deploy, webdev_deploy_with, webdev_precondition, webdev_status,
    };
    use crate::client::GatewayApi;
    use crate::client::projects::ImportOutcome;
    use crate::error::CoreError;
    use crate::webdev::ROUTE_BUNDLE_VERSION as BUNDLE_VERSION;
    use std::path::PathBuf;

    /// A scripted double: probes answer from a lookup, the import
    /// callback packs the zip for inspection (recorded through a
    /// Mutex so the closure stays `Fn` — `&self` callable — and the
    /// rig `Sync`, both required by `&dyn GatewayApi`). Everything
    /// else is unreachable (the established action-double shape).
    struct WebdevRig {
        probe: fn(&str) -> Result<RouteProbe, CoreError>,
        /// What [`GatewayApi::gateway_transport`] answers (`None`: a double
        /// with no URL of its own).
        transport: Option<(url::Url, bool)>,
        import: Box<dyn Fn(Vec<u8>, bool) -> Result<ImportOutcome, CoreError> + Send + Sync>,
    }

    fn present(version: &str) -> Result<RouteProbe, CoreError> {
        Ok(RouteProbe::Present {
            route_version: version.to_string(),
        })
    }

    fn ok_import() -> ImportOutcome {
        ImportOutcome {
            response: serde_json::json!({"success": true}),
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for WebdevRig {
        fn gateway_transport(&self) -> Option<(url::Url, bool)> {
            self.transport.clone()
        }
        async fn thread_dump(
            &self,
        ) -> Result<crate::client::threads::FormattedThreadDump, CoreError> {
            unreachable!("not part of this action")
        }
        async fn thread_deadlocks(
            &self,
        ) -> Result<crate::client::threads::DeadlocksWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn uninstall_module(
            &self,
            _gateway_module_id: &str,
            _module_id: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn bundle_generate(
            &self,
        ) -> Result<crate::client::diagnostics::BundleStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn bundle_status(
            &self,
        ) -> Result<crate::client::diagnostics::BundleStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn bundle_download(
            &self,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_list(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::tags::TagProviderRecord>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn tag_provider_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::tags::TagProviderRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_create(
            &self,
            _body: &[crate::client::tags::TagProviderCreate],
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn tag_provider_delete(
            &self,
            _name: &str,
            _signature: &str,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_probe(
            &self,
            _project: &str,
            route: &str,
            _extra_headers: &[(&str, &str)],
        ) -> Result<RouteProbe, CoreError> {
            (self.probe)(route)
        }
        async fn project_import(
            &self,
            _name: &str,
            zip: Vec<u8>,
            overwrite: bool,
        ) -> Result<ImportOutcome, CoreError> {
            (self.import)(zip, overwrite)
        }
        async fn gateway_info(&self) -> Result<crate::client::version::GatewayInfo, CoreError> {
            unreachable!("not part of this action")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unreachable!("not part of this action")
        }
        async fn status_ping(&self) -> Result<crate::client::status::StatusPing, CoreError> {
            unreachable!("not part of this action")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::status::ModuleInfo>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn metrics_current(
            &self,
        ) -> Result<crate::client::metrics::CurrentGauges, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_historic(
            &self,
        ) -> Result<crate::client::metrics::PerformanceCharts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_threads(&self) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn designers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::DesignerInfo>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::PerspectiveSession>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::sessions::VisionClient>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn database_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::connections::GatewayConnection>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LogEntry>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::logs::LoggerInfo>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn restart(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn scan_projects(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn security_properties(
            &self,
        ) -> Result<crate::client::restart::SecurityProperties, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_status(&self, _route: &str) -> Result<u16, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_call(
            &self,
            _project: &str,
            _route: &str,
            _body: &serde_json::Value,
            _extra_headers: &[(&str, &str)],
        ) -> Result<serde_json::Value, CoreError> {
            unreachable!("not part of this action")
        }
        async fn projects(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<
            crate::client::query::ListEnvelope<crate::client::projects::ProjectRecord>,
            CoreError,
        > {
            unreachable!("not part of this action")
        }
        async fn project_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::projects::ProjectRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_create(
            &self,
            _body: &crate::client::projects::ProjectCreate,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_copy(&self, _from: &str, _to: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_rename(&self, _name: &str, _new_name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_modify(
            &self,
            _name: &str,
            _body: &crate::client::projects::ProjectModify,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_delete(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_export_to_file(
            &self,
            _name: &str,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_download(
            &self,
            _out: &std::path::Path,
            _backup_type: crate::client::backup::BackupType,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_restore(&self, _gwbk: &std::path::Path) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_history(
            &self,
            _limit: Option<u32>,
            _search: Option<&str>,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamHistoryItem>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_definitions(
            &self,
        ) -> Result<crate::client::query::ListEnvelope<crate::client::eam::EamTaskRecord>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn eam_task_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::eam::EamTaskRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_create(&self, _definition: &serde_json::Value) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_force(&self, _owner: &str, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_suspend(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_resume(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_cancel(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_tasks_scheduled(
            &self,
            _running: bool,
        ) -> Result<Vec<crate::client::eam::EamScheduledTask>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_modify(
            &self,
            _definition: &serde_json::Value,
        ) -> Result<Option<crate::client::eam::ModifyOutcome>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn eam_task_delete(
            &self,
            _name: &str,
            _signature: &str,
            _confirm: bool,
        ) -> Result<crate::client::eam::DeleteOutcome, CoreError> {
            unreachable!("not part of this action")
        }
        async fn api_call(
            &self,
            _call: &crate::client::apicall::ApiCallRequest,
        ) -> Result<crate::client::apicall::ApiCallData, CoreError> {
            unreachable!("not part of this action")
        }
        async fn license_status(
            &self,
        ) -> Result<crate::client::license::LicenseStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn redundancy_status(
            &self,
        ) -> Result<crate::client::redundancy::RedundancyStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn gan_status(&self) -> Result<crate::client::gan::GanStatusWire, CoreError> {
            unreachable!("not part of this action")
        }
    }

    /// Temp config with one `dev` profile (no auth — the action never
    /// resolves credentials itself).
    fn temp_config() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "active = \"dev\"\n\n[profiles.dev]\nurl = \"http://localhost:9088/\"\n",
        )
        .expect("write config");
        (dir, path)
    }

    /// Read the stored secret back out of a config (test eyes only).
    fn stored_secret(path: &std::path::Path) -> Option<String> {
        crate::config::load(path)
            .expect("config reloads")
            .profiles
            .get("dev")
            .and_then(|profile| profile.webdev_secret.clone())
    }

    /// A rig whose import always succeeds and records nothing.
    fn importing_rig() -> WebdevRig {
        WebdevRig {
            probe: |_| unreachable!("deploy never probes"),
            transport: None,
            import: Box::new(|_zip, _overwrite| Ok(ok_import())),
        }
    }

    /// Deploy WITHOUT --with-script-exec never ships the route and
    /// never stores a secret.
    #[tokio::test]
    async fn deploy_without_script_exec_ships_only_the_always_on_bundle() {
        let (dir, config) = temp_config();
        let seen_zip = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = std::sync::Arc::clone(&seen_zip);
        let rig = WebdevRig {
            probe: |_| unreachable!("deploy never probes"),
            transport: None,
            import: Box::new(move |zip, overwrite| {
                assert!(overwrite, "deploy ALWAYS overwrite-imports");
                *recorder.lock().expect("zip lock") = zip;
                Ok(ok_import())
            }),
        };
        let result = webdev_deploy(&rig, "ign-cli", false, false, &config, "dev", false)
            .await
            .expect("plain deploy");
        assert_eq!(
            result.routes,
            vec!["tags", "tagConfig", "alarms", "tagHistory"]
        );
        assert!(!result.script_exec);
        assert!(!result.secret_rotated);
        assert_eq!(result.import["success"], true);

        // The uploaded zip carries NO scriptExec member and no secret
        // landed in the config.
        let names = member_names(&seen_zip.lock().expect("zip lock"));
        assert!(names.iter().all(|name| !name.contains("scriptExec")));
        assert_eq!(stored_secret(&config), None);
        let _ = dir; // keeps the tempdir alive for the asserts above
    }

    /// Deploy --with-script-exec with NO stored secret: generates,
    /// persists 0600, bakes into the zip — and the SERIALIZED result
    /// never carries the value (the redaction pin).
    #[tokio::test]
    async fn deploy_with_script_exec_generates_and_redacts_the_secret() {
        let (dir, config) = temp_config();
        let seen_zip = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = std::sync::Arc::clone(&seen_zip);
        let rig = WebdevRig {
            probe: |_| unreachable!("deploy never probes"),
            transport: None,
            import: Box::new(move |zip, _| {
                *recorder.lock().expect("zip lock") = zip;
                Ok(ok_import())
            }),
        };
        let result = webdev_deploy(&rig, "ign-cli", true, false, &config, "dev", false)
            .await
            .expect("scriptExec deploy");
        assert_eq!(
            result.routes,
            vec!["tags", "tagConfig", "alarms", "tagHistory", "scriptExec"]
        );
        assert!(result.script_exec && result.secret_rotated);

        let secret = stored_secret(&config).expect("secret persisted");
        assert_eq!(secret.len(), 64, "32 bytes hex-encoded");
        assert!(
            secret.chars().all(|c| c.is_ascii_hexdigit()),
            "hex alphabet: {secret}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&config)
                .expect("config stat")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the save path re-asserts 0600");
        }

        // The baked route carries the secret; the serialized result
        // does NOT (redaction — Phase 1's canary pattern).
        let do_post = member(
            &seen_zip.lock().expect("zip lock"),
            "com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py",
        );
        assert!(String::from_utf8_lossy(&do_post).contains(&secret));
        let serialized = serde_json::to_string(&result).expect("result serializes");
        assert!(!serialized.contains(&secret), "redaction: {serialized}");
        let envelope_check: WebdevDeployResult = result;
        let again = serde_json::to_string(&envelope_check).expect("serializes");
        assert!(!again.contains(&secret));
        let _ = dir;
    }

    /// --rotate-secret regenerates even when a secret exists; a plain
    /// re-deploy reuses the stored one unchanged.
    #[tokio::test]
    async fn rotate_regenerates_and_plain_redeploy_reuses() {
        let (dir, config) = temp_config();
        // Seed a stored secret.
        let mut seeded = crate::config::load(&config).expect("load");
        seeded.profiles.get_mut("dev").unwrap().webdev_secret = Some("aa11".into());
        crate::config::save(&config, &seeded).expect("seed save");

        // Plain scriptExec redeploy: reuses `aa11`, rotated=false.
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = std::sync::Arc::clone(&seen);
        let rig = WebdevRig {
            probe: |_| unreachable!(),
            transport: None,
            import: Box::new(move |zip, _| {
                *recorder.lock().expect("lock") = zip;
                Ok(ok_import())
            }),
        };
        let result = webdev_deploy(&rig, "ign-cli", true, false, &config, "dev", false)
            .await
            .expect("reuse deploy");
        assert!(!result.secret_rotated);
        assert_eq!(stored_secret(&config).as_deref(), Some("aa11"));
        let seen = seen.lock().expect("lock").clone();
        let do_post = member(
            &seen,
            "com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py",
        );
        assert!(
            String::from_utf8_lossy(&do_post).contains("aa11"),
            "the STORED secret rode the zip's scriptExec member"
        );

        // --rotate-secret: a fresh 64-char hex replaces it.
        let result = webdev_deploy(
            &importing_rig(),
            "ign-cli",
            true,
            true,
            &config,
            "dev",
            false,
        )
        .await
        .expect("rotate deploy");
        assert!(result.secret_rotated);
        let rotated = stored_secret(&config).expect("rotated secret stored");
        assert_eq!(rotated.len(), 64);
        assert_ne!(rotated, "aa11");
        let _ = dir;
    }

    /// The status matrix: every probe state maps onto its row; ok is
    /// gated ONLY by the always-on routes; scriptExec rides along
    /// exactly when a secret is configured.
    #[tokio::test]
    async fn status_maps_the_full_probe_matrix() {
        // All present + matching: ok=true.
        let healthy = WebdevRig {
            probe: |_| present(BUNDLE_VERSION),
            transport: None,
            import: Box::new(|_, _| unreachable!("status never imports")),
        };
        let result = webdev_status(&healthy, "ign-cli", None)
            .await
            .expect("status sweep");
        assert_eq!(result.routes.len(), 4);
        assert!(
            result
                .routes
                .iter()
                .all(|row| row.status == RouteStatus::Present)
        );
        assert!(result.ok);

        // One route absent, one mismatched: ok=false, degradation is
        // DATA (rows carry their own verdicts).
        let degraded = WebdevRig {
            transport: None,
            probe: |route| {
                Ok(match route {
                    "tags" => RouteProbe::Absent,
                    "tagConfig" => present("9.9.9").expect("fixture"),
                    "alarms" => present(BUNDLE_VERSION).expect("fixture"),
                    "tagHistory" => RouteProbe::Unlicensed,
                    "scriptExec" => RouteProbe::Denied {
                        code: "secret_mismatch".into(),
                        message: "mismatch".into(),
                        traceback: None,
                    },
                    _ => RouteProbe::AuthGated,
                })
            },
            import: Box::new(|_, _| unreachable!()),
        };
        let result = webdev_status(&degraded, "ign-cli", Some("stored-secret"))
            .await
            .expect("degraded sweep still completes");
        let by_route = |name: &str| {
            result
                .routes
                .iter()
                .find(|row| row.route == name)
                .unwrap_or_else(|| panic!("{name} row"))
        };
        assert_eq!(by_route("tags").status, RouteStatus::Absent);
        assert_eq!(by_route("tagConfig").status, RouteStatus::VersionMismatch);
        assert_eq!(
            by_route("tagConfig").deployed_version.as_deref(),
            Some("9.9.9")
        );
        assert_eq!(by_route("tagHistory").status, RouteStatus::Unlicensed);
        assert_eq!(by_route("scriptExec").status, RouteStatus::SecretMismatch);
        assert!(!result.ok);
        // scriptExec never gates ok: the healthy sweep with a secret
        // whose probe denies stays ok=true.
        let gated_exec = WebdevRig {
            transport: None,
            probe: |route| {
                if route == "scriptExec" {
                    Ok(RouteProbe::AuthGated)
                } else {
                    present(BUNDLE_VERSION)
                }
            },
            import: Box::new(|_, _| unreachable!()),
        };
        let result = webdev_status(&gated_exec, "ign-cli", Some("s"))
            .await
            .expect("sweep");
        assert!(result.ok, "scriptExec never gates ok");
        assert_eq!(result.routes.len(), 5);
    }

    /// THE precondition refusal matrix (must-have truth #3): before
    /// deploy → routes_not_deployed naming `ign webdev deploy`;
    /// mismatch → route_version_mismatch with both versions named.
    #[tokio::test]
    async fn precondition_refuses_the_undeployed_and_mismatched() {
        let undeployed = WebdevRig {
            probe: |_| Ok(RouteProbe::Absent),
            transport: None,
            import: Box::new(|_, _| unreachable!()),
        };
        let err = webdev_precondition(&undeployed, "ign-cli")
            .await
            .expect_err("absent refuses");
        assert_eq!(err.code(), "routes_not_deployed");
        assert_eq!(err.exit_code(), 6);
        assert!(
            err.hint().unwrap().contains("ign webdev deploy"),
            "hint names the fix"
        );

        let older = WebdevRig {
            probe: |_| present("0.9.0"),
            transport: None,
            import: Box::new(|_, _| unreachable!()),
        };
        let err = webdev_precondition(&older, "ign-cli")
            .await
            .expect_err("older refuses");
        assert_eq!(err.code(), "route_version_mismatch");
        assert!(
            err.to_string().contains("0.9.0") && err.to_string().contains(BUNDLE_VERSION),
            "both versions named: {err}"
        );

        let matching = WebdevRig {
            probe: |_| present(BUNDLE_VERSION),
            transport: None,
            import: Box::new(|_, _| unreachable!()),
        };
        webdev_precondition(&matching, "ign-cli")
            .await
            .expect("matching handshake passes");
    }

    const TAG_DELETE_DO_POST: &str =
        "com.inductiveautomation.webdev/resources/cli/tagDelete/doPost.py";
    const TAG_DELETE_CONFIG: &str =
        "com.inductiveautomation.webdev/resources/cli/tagDelete/config.json";

    /// A project repo whose `deploy/project.yaml` owns `providers` (a YAML
    /// flow list such as `[P26Del]`).
    fn temp_repo(providers: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("deploy")).expect("deploy dir");
        std::fs::write(
            dir.path().join("deploy/project.yaml"),
            format!(
                "schema_version: 1\nproject: Demo\nenvironments: [dev]\ntag_providers: {providers}\n"
            ),
        )
        .expect("project.yaml");
        dir
    }

    fn tag_delete_request(repo: &std::path::Path, rotate_secret: bool) -> TagDeleteRequest {
        TagDeleteRequest {
            repos: vec![repo.to_path_buf()],
            user_source: "default".into(),
            role: "FakeDeployRole".into(),
            rotate_secret,
            caller: None,
        }
    }

    fn stored_tag_delete_secret(path: &std::path::Path) -> Option<String> {
        crate::config::load(path)
            .expect("config reloads")
            .profiles
            .get("dev")
            .and_then(|profile| profile.webdev_tag_delete_secret.clone())
    }

    /// A rig whose import must never run (refusals happen before any
    /// gateway request).
    fn no_gateway_rig() -> WebdevRig {
        WebdevRig {
            probe: |_| unreachable!("deploy never probes"),
            transport: None,
            import: Box::new(|_, _| panic!("no gateway request may happen")),
        }
    }

    /// A rig recording the uploaded zip and, at upload time, the stored
    /// tag-delete secret (proving it was persisted BEFORE the upload).
    #[allow(clippy::type_complexity)]
    fn recording_rig(
        config: &std::path::Path,
    ) -> (
        WebdevRig,
        std::sync::Arc<std::sync::Mutex<(Vec<u8>, Option<String>)>>,
    ) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new((Vec::new(), None)));
        let recorder = std::sync::Arc::clone(&seen);
        let config = config.to_path_buf();
        let rig = WebdevRig {
            probe: |_| unreachable!("deploy never probes"),
            transport: None,
            import: Box::new(move |zip, overwrite| {
                assert!(overwrite);
                *recorder.lock().expect("lock") = (zip, stored_tag_delete_secret(&config));
                Ok(ok_import())
            }),
        };
        (rig, seen)
    }

    /// No stored secret: one is generated, persisted 0600 before the
    /// upload and packed; a second deploy reuses it; rotate regenerates.
    /// The result, its Debug form and its JSON carry no secret.
    #[tokio::test]
    async fn tag_delete_secret_generated_persisted_reused_and_rotated() {
        let (dir, config) = temp_config();
        let repo = temp_repo("[P26Del]");
        let (rig, seen) = recording_rig(&config);
        let result = webdev_deploy_with(
            &rig,
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect("tag delete deploy");
        assert!(result.tag_delete && result.tag_delete_secret_rotated);
        assert!(!result.script_exec && !result.secret_rotated);
        assert_eq!(result.tag_delete_providers, vec!["P26Del".to_string()]);
        assert_eq!(
            result.routes,
            vec!["tags", "tagConfig", "alarms", "tagHistory", "tagDelete"]
        );

        let secret = stored_tag_delete_secret(&config).expect("secret persisted");
        assert_eq!(secret.len(), 64);
        assert!(
            secret
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(
            stored_secret(&config),
            None,
            "the scriptExec secret is untouched"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&config)
                .expect("stat")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let (zip, at_upload) = seen.lock().expect("lock").clone();
        assert_eq!(
            at_upload.as_deref(),
            Some(secret.as_str()),
            "persisted before upload"
        );
        assert!(String::from_utf8_lossy(&member(&zip, TAG_DELETE_DO_POST)).contains(&secret));
        let packed_config: serde_json::Value =
            serde_json::from_slice(&member(&zip, TAG_DELETE_CONFIG)).expect("config.json");
        assert_eq!(packed_config["doPost"]["required-roles"], "FakeDeployRole");
        assert_eq!(packed_config["doPost"]["user-source"], "default");

        let serialized = serde_json::to_string(&result).expect("serializes");
        assert!(!serialized.contains(&secret), "redaction: {serialized}");
        assert!(serialized.contains("\"tag_delete\":true"));
        assert!(serialized.contains("\"tag_delete_secret_rotated\":true"));
        assert!(serialized.contains("\"tag_delete_providers\":[\"P26Del\"]"));
        assert!(!format!("{result:?}").contains(&secret));

        // Reuse.
        let (rig, seen) = recording_rig(&config);
        let result = webdev_deploy_with(
            &rig,
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect("reuse deploy");
        assert!(result.tag_delete && !result.tag_delete_secret_rotated);
        assert_eq!(
            stored_tag_delete_secret(&config).as_deref(),
            Some(secret.as_str())
        );
        let (zip, _) = seen.lock().expect("lock").clone();
        assert!(String::from_utf8_lossy(&member(&zip, TAG_DELETE_DO_POST)).contains(&secret));

        // Rotate.
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), true)),
        )
        .await
        .expect("rotate deploy");
        assert!(result.tag_delete_secret_rotated);
        let rotated = stored_tag_delete_secret(&config).expect("rotated");
        assert_eq!(rotated.len(), 64);
        assert_ne!(rotated, secret);
        let _ = dir;
    }

    /// The scriptExec and tag-delete secrets are independent: rotating one
    /// never changes the other, and both routes compose in one import.
    #[tokio::test]
    async fn script_exec_and_tag_delete_secrets_are_independent() {
        let (dir, config) = temp_config();
        let repo = temp_repo("[P26Del, Other]");
        let (rig, seen) = recording_rig(&config);
        let result = webdev_deploy_with(
            &rig,
            "ign-cli",
            true,
            false,
            &config,
            "dev",
            true,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect("composed deploy");
        assert!(result.script_exec && result.tag_delete);
        assert_eq!(
            result.tag_delete_providers,
            vec!["P26Del".to_string(), "Other".to_string()]
        );
        assert!(result.routes.contains(&"scriptExec".to_string()));
        assert!(result.routes.contains(&"tagDelete".to_string()));
        assert!(
            result
                .routes
                .iter()
                .any(|route| route.starts_with("testing/"))
        );
        let names = member_names(&seen.lock().expect("lock").0);
        assert!(names.iter().any(|name| name.contains("cli/scriptExec/")));
        assert!(names.iter().any(|name| name.contains("cli/tagDelete/")));

        let exec_secret = stored_secret(&config).expect("scriptExec secret");
        let delete_secret = stored_tag_delete_secret(&config).expect("tagDelete secret");
        assert_ne!(exec_secret, delete_secret);

        // Rotate scriptExec only.
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            true,
            true,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect("rotate scriptExec");
        assert!(result.secret_rotated && !result.tag_delete_secret_rotated);
        assert_ne!(
            stored_secret(&config).as_deref(),
            Some(exec_secret.as_str())
        );
        assert_eq!(
            stored_tag_delete_secret(&config).as_deref(),
            Some(delete_secret.as_str())
        );

        // Rotate tag delete only.
        let exec_now = stored_secret(&config).expect("scriptExec secret");
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            true,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), true)),
        )
        .await
        .expect("rotate tag delete");
        assert!(!result.secret_rotated && result.tag_delete_secret_rotated);
        assert_eq!(stored_secret(&config).as_deref(), Some(exec_now.as_str()));
        assert_ne!(
            stored_tag_delete_secret(&config).as_deref(),
            Some(delete_secret.as_str())
        );
        let serialized = serde_json::to_string(&result).expect("serializes");
        for secret in [
            exec_now,
            stored_tag_delete_secret(&config).expect("tagDelete secret"),
        ] {
            assert!(!serialized.contains(&secret));
            assert!(!format!("{result:?}").contains(&secret));
        }
        let _ = dir;
    }

    /// The allowlist comes from the repo; an empty list, `System` in any
    /// case, a bad role or user source, or a broken repo refuse before any
    /// gateway request and before any secret is stored. No error text
    /// carries a stored secret.
    #[tokio::test]
    async fn tag_delete_refusals_happen_before_any_gateway_request() {
        let (dir, config) = temp_config();
        let mut seeded = crate::config::load(&config).expect("load");
        seeded.profiles.get_mut("dev").expect("dev").webdev_secret =
            Some("fakeexecsecretvalue".into());
        crate::config::save(&config, &seeded).expect("seed");

        for (providers, code) in [
            ("[]", "invalid_input"),
            ("[System]", "invalid_input"),
            ("[P1, system]", "invalid_input"),
        ] {
            let repo = temp_repo(providers);
            let err = webdev_deploy_with(
                &no_gateway_rig(),
                "ign-cli",
                true,
                false,
                &config,
                "dev",
                false,
                Some(tag_delete_request(repo.path(), false)),
            )
            .await
            .expect_err("refused");
            assert_eq!(err.code(), code, "{providers}: {err}");
            assert!(!err.to_string().contains("fakeexecsecretvalue"));
        }

        let repo = temp_repo("[P26Del]");
        let mut bad_role = tag_delete_request(repo.path(), false);
        bad_role.role = "x'y".into();
        let err = webdev_deploy_with(
            &no_gateway_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(bad_role),
        )
        .await
        .expect_err("bad role");
        assert_eq!(err.code(), "invalid_input");
        assert!(err.to_string().contains("role"), "{err}");

        let empty_repo = tempfile::tempdir().expect("tempdir");
        let err = webdev_deploy_with(
            &no_gateway_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(empty_repo.path(), false)),
        )
        .await
        .expect_err("no project.yaml");
        assert_eq!(err.code(), "deploy_tree_invalid");

        assert_eq!(stored_tag_delete_secret(&config), None, "nothing stored");
        let _ = dir;
    }

    /// 26-REVIEW WR-07: the tagDelete secret goes only over https or to a
    /// loopback host (and with `ssl_verify` on for a remote one), the route
    /// client's own rule: plain http to a remote host, or unverified TLS to
    /// one, refuses `tag_delete_not_configured` before any secret is stored
    /// and before any gateway request.
    #[tokio::test]
    async fn tag_delete_install_refuses_an_unsafe_transport() {
        // The double reports no URL, so the check falls back to the stored
        // profile; no env read happens (WR-D2).
        let repo = temp_repo("[P26Del]");
        for (url, ssl_verify, why) in [
            ("http://gateway.example.com:8088/", true, "plain http"),
            (
                "https://gateway.example.com:8043/",
                false,
                "ssl_verify false",
            ),
        ] {
            let (dir, config) = temp_config();
            let mut seeded = crate::config::load(&config).expect("load");
            let profile = seeded.profiles.get_mut("dev").expect("dev");
            profile.url = url::Url::parse(url).expect("url");
            profile.ssl_verify = ssl_verify;
            crate::config::save(&config, &seeded).expect("seed");
            let err = webdev_deploy_with(
                &no_gateway_rig(),
                "ign-cli",
                false,
                false,
                &config,
                "dev",
                false,
                Some(tag_delete_request(repo.path(), false)),
            )
            .await
            .expect_err("refused");
            assert_eq!(err.code(), "tag_delete_not_configured", "{url}: {err}");
            assert!(err.to_string().contains(why), "{url}: {err}");
            assert_eq!(stored_tag_delete_secret(&config), None, "{url}");
            let _ = dir;
        }
    }

    /// 26-REVIEW-D WR-D2: the transport rule checks the URL the import's
    /// client sends to, not a fresh read of `IGNITION_URL`. An env value
    /// set by another test (or changed after the session was built) neither
    /// refuses a safe client nor admits an unsafe one.
    #[test]
    fn tag_delete_transport_check_reads_the_clients_url() {
        let _lock = crate::config::ENV_LOCK.lock().expect("env lock");
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
            .block_on(transport_check_reads_the_clients_url());
    }

    async fn transport_check_reads_the_clients_url() {
        let repo = temp_repo("[P26Del]");
        let loopback = url::Url::parse("http://localhost:9088/").expect("url");
        let remote = url::Url::parse("http://gateway.example.com:8088/").expect("url");

        // A remote plain-http IGNITION_URL in the env, a loopback client.
        let (dir, config) = temp_config();
        // SAFETY: single-threaded under ENV_LOCK; removed before return.
        unsafe { std::env::set_var("IGNITION_URL", remote.as_str()) };
        let mut rig = importing_rig();
        rig.transport = Some((loopback, true));
        let result = webdev_deploy_with(
            &rig,
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await;
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_URL") };
        let result = result.expect("the client's loopback URL is safe");
        assert!(result.tag_delete);
        let _ = dir;

        // No env, a loopback profile, a remote plain-http client.
        let (dir, config) = temp_config();
        let mut rig = no_gateway_rig();
        rig.transport = Some((remote, true));
        let err = webdev_deploy_with(
            &rig,
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect_err("the client's remote http URL is refused");
        assert_eq!(err.code(), "tag_delete_not_configured", "{err}");
        assert!(err.to_string().contains("plain http"), "{err}");
        assert_eq!(stored_tag_delete_secret(&config), None);
        let _ = dir;
    }

    /// A config store that cannot be written refuses before any gateway
    /// request.
    #[cfg(unix)]
    #[tokio::test]
    async fn tag_delete_broken_config_store_refuses_before_any_gateway_request() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, config) = temp_config();
        let repo = temp_repo("[P26Del]");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o400))
            .expect("read-only");
        let err = webdev_deploy_with(
            &no_gateway_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect_err("unwritable store refuses");
        assert_eq!(err.code(), "config_invalid", "{err}");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).expect("restore");

        std::fs::write(&config, "not = [valid").expect("break");
        let err = webdev_deploy_with(
            &no_gateway_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect_err("unparsable store refuses");
        assert_ne!(err.code(), "internal");
        let _ = dir;
    }

    /// A deploy without the tag-delete request packs no tagDelete member and
    /// reports the three keys as off.
    #[tokio::test]
    async fn deploy_without_tag_delete_packs_no_tag_delete_member() {
        let (dir, config) = temp_config();
        let (rig, seen) = recording_rig(&config);
        let result = webdev_deploy_with(&rig, "ign-cli", true, false, &config, "dev", false, None)
            .await
            .expect("deploy");
        assert!(!result.tag_delete && !result.tag_delete_secret_rotated);
        assert!(result.tag_delete_providers.is_empty());
        assert!(!result.routes.contains(&"tagDelete".to_string()));
        let names = member_names(&seen.lock().expect("lock").0);
        assert!(names.iter().all(|name| !name.contains("tagDelete")));
        assert_eq!(stored_tag_delete_secret(&config), None);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let _ = dir;
    }

    /// 26-REVIEW-D WR-D4: a deploy without the tag-delete request (the CLI
    /// without `--with-tag-delete`, the TUI, adopt) drops the route, so it
    /// also drops the stored tagDelete secret, before the upload, and warns
    /// naming the removal. The profile then reads not configured. The
    /// deploy itself is not refused, and the warning carries no secret.
    #[tokio::test]
    async fn deploy_without_tag_delete_removes_the_stored_secret_and_warns() {
        let (dir, config) = temp_config();
        let mut seeded = crate::config::load(&config).expect("load");
        seeded
            .profiles
            .get_mut("dev")
            .expect("dev")
            .webdev_tag_delete_secret = Some("a".repeat(64));
        crate::config::save(&config, &seeded).expect("seed");

        // A deploy into another project leaves the route in `ign-cli`, the
        // one the route client calls: the secret stays, no warning.
        let result = webdev_deploy(
            &importing_rig(),
            "Other",
            false,
            false,
            &config,
            "dev",
            false,
        )
        .await
        .expect("deploy");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(stored_tag_delete_secret(&config).is_some());

        // `webdev_deploy` is the path adopt and the TUI call.
        let (rig, seen) = recording_rig(&config);
        let result = webdev_deploy(&rig, "ign-cli", true, false, &config, "dev", false)
            .await
            .expect("the deploy is not refused");
        assert!(!result.tag_delete);
        assert_eq!(stored_tag_delete_secret(&config), None, "secret removed");
        assert_eq!(seen.lock().expect("lock").1, None, "removed before upload");
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        let warning = &result.warnings[0];
        assert!(warning.contains("tagDelete"), "{warning}");
        assert!(warning.contains("removed"), "{warning}");
        assert!(warning.contains("--with-tag-delete"), "{warning}");
        assert!(!warning.contains(&"a".repeat(64)), "{warning}");
        assert!(stored_secret(&config).is_some(), "scriptExec kept");

        let profile = crate::config::load(&config)
            .expect("reload")
            .profiles
            .remove("dev")
            .expect("dev");
        match crate::deploy::tag_delete::tag_delete_config("dev", &profile) {
            crate::deploy::tag_delete::TagDeleteConfig::NotConfigured { missing } => {
                assert!(
                    missing
                        .iter()
                        .any(|part| part == "webdev_tag_delete_secret"),
                    "{missing:?}"
                );
            }
            other => panic!("expected NotConfigured, got {other:?}"),
        }

        // Nothing stored: nothing to remove, no warning.
        let result = webdev_deploy(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
        )
        .await
        .expect("deploy");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let _ = dir;
    }

    /// A deploy that installs the route keeps its secret and warns nothing.
    #[tokio::test]
    async fn deploy_with_tag_delete_keeps_the_secret_and_does_not_warn() {
        let (dir, config) = temp_config();
        let repo = temp_repo("[P26Del]");
        for _ in 0..2 {
            let result = webdev_deploy_with(
                &importing_rig(),
                "ign-cli",
                false,
                false,
                &config,
                "dev",
                false,
                Some(tag_delete_request(repo.path(), false)),
            )
            .await
            .expect("deploy");
            assert!(
                !result.warnings.iter().any(|w| w.contains("removed")),
                "{:?}",
                result.warnings
            );
            assert!(stored_tag_delete_secret(&config).is_some());
        }
        let _ = dir;
    }

    /// A temp config whose `dev` profile points at `url` and holds a stored
    /// tagDelete secret.
    fn config_with_route_secret(url: &str, secret: &str) -> (tempfile::TempDir, PathBuf) {
        let (dir, config) = temp_config();
        let mut seeded = crate::config::load(&config).expect("load");
        let profile = seeded.profiles.get_mut("dev").expect("dev");
        profile.url = url::Url::parse(url).expect("url");
        profile.webdev_tag_delete_secret = Some(secret.to_string());
        crate::config::save(&config, &seeded).expect("seed");
        (dir, config)
    }

    /// Mount the live route's `version` answer with `providers`, or a bare
    /// HTTP `status` when `providers` is `None`.
    async fn mount_live_route(server: &wiremock::MockServer, providers: Option<&[&str]>) {
        let response = match providers {
            Some(providers) => wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"ok": true, "data": {
                    "routeVersion": BUNDLE_VERSION, "minCli": "1.0.0",
                    "providers": providers, "maxPaths": 500, "maxPathChars": 1024,
                }}),
            ),
            None => wiremock::ResponseTemplate::new(405),
        };
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(
                "/system/webdev/ign-cli/cli/tagDelete",
            ))
            .and(wiremock::matchers::header(
                "X-Ignition-CLI-Secret",
                "c".repeat(64).as_str(),
            ))
            .respond_with(response)
            .mount(server)
            .await;
    }

    fn caller() -> Option<(crate::config::Secret, crate::config::Secret)> {
        Some((
            crate::config::Secret::new("deployer"),
            crate::config::Secret::new("Pw-fake-Z9"),
        ))
    }

    /// 26-REVIEW-D WR-D3: before an install replaces the route, its live
    /// allowlist is read through one handshake (the stored secret and the
    /// caller's env pair). An install that would drop a provider allowed
    /// now refuses `invalid_input` naming it and both ways forward, before
    /// any secret is stored and before the import. Naming every repo
    /// installs the union.
    #[tokio::test]
    async fn tag_delete_install_refuses_to_drop_live_providers() {
        let server = wiremock::MockServer::start().await;
        mount_live_route(&server, Some(&["PA", "PB"])).await;
        let secret = "c".repeat(64);
        let (dir, config) = config_with_route_secret(&server.uri(), &secret);
        let repo_a = temp_repo("[PA]");
        let repo_b = temp_repo("[PB]");

        let mut request = tag_delete_request(repo_b.path(), false);
        request.caller = caller();
        let err = webdev_deploy_with(
            &no_gateway_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(request.clone()),
        )
        .await
        .expect_err("dropping PA is refused");
        assert_eq!(err.code(), "invalid_input", "{err}");
        let text = err.to_string();
        assert!(text.contains("PA"), "{text}");
        assert!(text.contains("--tag-delete-repo"), "{text}");
        assert!(text.contains("without --with-tag-delete"), "{text}");
        assert!(!text.contains(&secret), "{text}");
        let hint = err.hint().unwrap_or_default();
        assert!(hint.contains("--tag-delete-repo"), "{hint}");
        assert_eq!(
            stored_tag_delete_secret(&config).as_deref(),
            Some(secret.as_str()),
            "nothing stored"
        );

        // A rotate reads with the stored secret and refuses the same way.
        let mut rotate = request.clone();
        rotate.rotate_secret = true;
        let err = webdev_deploy_with(
            &no_gateway_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(rotate),
        )
        .await
        .expect_err("a rotate is refused too");
        assert_eq!(err.code(), "invalid_input", "{err}");

        // Naming both repos keeps both providers.
        request.repos = vec![repo_a.path().to_path_buf(), repo_b.path().to_path_buf()];
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(request),
        )
        .await
        .expect("the union keeps every live provider");
        assert_eq!(result.tag_delete_providers, vec!["PA", "PB"]);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let _ = dir;
    }

    /// A live allowlist that cannot be read (no env pair, a failing route)
    /// installs as before and warns; an absent route (405) or no stored
    /// secret means nothing is installed, so nothing is read or warned.
    #[tokio::test]
    async fn tag_delete_install_warns_when_the_live_allowlist_is_unreadable() {
        let secret = "c".repeat(64);
        let repo = temp_repo("[PB]");

        // No env pair: unreadable, warn, install.
        let server = wiremock::MockServer::start().await;
        mount_live_route(&server, Some(&["PA"])).await;
        let (dir, config) = config_with_route_secret(&server.uri(), &secret);
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(tag_delete_request(repo.path(), false)),
        )
        .await
        .expect("installs");
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(
            result.warnings[0].contains("could not read the live tagDelete allowlist"),
            "{:?}",
            result.warnings
        );
        assert!(!result.warnings[0].contains(&secret));
        let _ = dir;

        // A route answering 500: unreadable, warn, install.
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let (dir, config) = config_with_route_secret(&server.uri(), &secret);
        let mut request = tag_delete_request(repo.path(), false);
        request.caller = caller();
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(request.clone()),
        )
        .await
        .expect("installs");
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].contains("could not read the live tagDelete allowlist"));
        let _ = dir;

        // The route is absent (405): nothing to keep, no warning.
        let server = wiremock::MockServer::start().await;
        mount_live_route(&server, None).await;
        let (dir, config) = config_with_route_secret(&server.uri(), &secret);
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(request.clone()),
        )
        .await
        .expect("installs");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let _ = dir;

        // No stored secret: a first install reads nothing, warns nothing.
        let (dir, config) = temp_config();
        let result = webdev_deploy_with(
            &importing_rig(),
            "ign-cli",
            false,
            false,
            &config,
            "dev",
            false,
            Some(request),
        )
        .await
        .expect("installs");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let _ = dir;
    }

    /// Generated secrets are hex and (statistically) unique across
    /// draws — the shape the route's fail-closed detector relies on.
    #[test]
    fn generated_secrets_are_hex_and_unique() {
        let a = generate_secret().expect("secret");
        let b = generate_secret().expect("secret");
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    fn member_names(zip_bytes: &[u8]) -> Vec<String> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).expect("built zip is readable");
        (0..archive.len())
            .map(|index| archive.by_index(index).expect("member").name().to_string())
            .collect()
    }

    fn member(zip_bytes: &[u8], name: &str) -> Vec<u8> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).expect("built zip is readable");
        let mut file = archive.by_name(name).expect("member present");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).expect("member reads");
        bytes
    }
}
