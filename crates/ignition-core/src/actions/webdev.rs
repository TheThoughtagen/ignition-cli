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

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::webdev::{self as seam, RouteProbe};
use crate::config;
use crate::error::CoreError;
use crate::webdev as bundle;

/// The secret-bearing scriptExec route name (deploy/status append it
/// only when explicitly requested / configured).
const SCRIPT_EXEC_ROUTE: &str = "scriptExec";

/// The header the scriptExec gate compares (case-insensitive
/// server-side; the CLI sends the canonical form).
const SECRET_HEADER: &str = "X-Ignition-CLI-Secret";

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
) -> Result<WebdevDeployResult, CoreError> {
    // Secret lifecycle first — refuse on an unwritable config store
    // BEFORE touching the gateway.
    let mut config = config::load(config_path)?;
    let existing = config
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.webdev_secret.clone());
    let (pack_secret, secret_rotated) = if rotate_secret || (with_script_exec && existing.is_none())
    {
        let secret = generate_secret()?;
        config
            .profiles
            .get_mut(profile_name)
            .ok_or_else(|| {
                CoreError::Internal(format!(
                    "profile {profile_name:?} vanished from the config between dispatch and deploy"
                ))
            })?
            .webdev_secret = Some(secret.clone());
        config::save(config_path, &config)?; // re-asserts 0600
        (Some(secret), true)
    } else if with_script_exec {
        (existing, false)
    } else {
        (None, false)
    };

    // Pack (scriptExec only when flagged — build_deploy_zip's
    // fail-closed guard covers the (true, None) bug case) and import
    // overwrite=true through the 03-02 machinery. NO pre-flight
    // project create (Pitfall 10).
    let zip = seam::build_deploy_zip(project, with_script_exec, pack_secret.as_deref())?;
    let mut routes = seam::always_on_routes();
    if with_script_exec {
        routes.push(SCRIPT_EXEC_ROUTE.to_string());
    }
    let import = api.project_import(project, zip, true).await?;

    Ok(WebdevDeployResult {
        project: project.to_string(),
        routes,
        script_exec: with_script_exec,
        secret_rotated,
        import: import.response,
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

/// 32 bytes from `/dev/urandom`, hex-encoded (64 chars) — zero-dep
/// generation (no `rand` in the workspace; unix-only is fine, no
/// Windows CI is locked by Phase 1 decision).
fn generate_secret() -> Result<String, CoreError> {
    let mut bytes = [0u8; 32];
    let mut source = std::fs::File::open("/dev/urandom").map_err(|err| {
        CoreError::Internal(format!(
            "cannot open /dev/urandom for secret generation: {err}"
        ))
    })?;
    source
        .read_exact(&mut bytes)
        .map_err(|err| CoreError::Internal(format!("cannot read /dev/urandom: {err}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::{
        RouteProbe, RouteStatus, WebdevDeployResult, generate_secret, webdev_deploy,
        webdev_precondition, webdev_status,
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
            import: Box::new(move |zip, overwrite| {
                assert!(overwrite, "deploy ALWAYS overwrite-imports");
                *recorder.lock().expect("zip lock") = zip;
                Ok(ok_import())
            }),
        };
        let result = webdev_deploy(&rig, "ign-cli", false, false, &config, "dev")
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
            import: Box::new(move |zip, _| {
                *recorder.lock().expect("zip lock") = zip;
                Ok(ok_import())
            }),
        };
        let result = webdev_deploy(&rig, "ign-cli", true, false, &config, "dev")
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
            import: Box::new(move |zip, _| {
                *recorder.lock().expect("lock") = zip;
                Ok(ok_import())
            }),
        };
        let result = webdev_deploy(&rig, "ign-cli", true, false, &config, "dev")
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
        let result = webdev_deploy(&importing_rig(), "ign-cli", true, true, &config, "dev")
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
            probe: |route| {
                Ok(match route {
                    "tags" => RouteProbe::Absent,
                    "tagConfig" => present("9.9.9").expect("fixture"),
                    "alarms" => present("1.0.0").expect("fixture"),
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
            import: Box::new(|_, _| unreachable!()),
        };
        let err = webdev_precondition(&older, "ign-cli")
            .await
            .expect_err("older refuses");
        assert_eq!(err.code(), "route_version_mismatch");
        assert!(
            err.to_string().contains("0.9.0") && err.to_string().contains("1.0.0"),
            "both versions named: {err}"
        );

        let matching = WebdevRig {
            probe: |_| present(BUNDLE_VERSION),
            import: Box::new(|_, _| unreachable!()),
        };
        webdev_precondition(&matching, "ign-cli")
            .await
            .expect("matching handshake passes");
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
