//! The doctor action (02-05, HLTH-10) — the self-service preflight:
//! one structured `checks[]` report diagnosing URL, liveness,
//! commissioning, auth (401-vs-403 made specific via the
//! security-properties deep-dive), write permission, WebDev-route
//! presence, and rig presence. Serde models OUT, no printing.
//!
//! Every classification is the research's empirically-verified failure
//! taxonomy (02-RESEARCH §Doctor inputs — each row verified live on a
//! real 8.3.6 gateway):
//! - `/StatusPing` separates DOWN-ness from auth failure BY
//!   CONSTRUCTION (this check carries no credential);
//! - 302→`/welcome` on any `/data` probe = uncommissioned;
//! - 401 = token not recognized (the `name:key` format failure);
//!   403 = recognized but under-permitted (the three-part setup);
//! - `scan/projects` is igw-cli's harmless rescan write probe;
//! - `/system/webdev`: the version-action probe answers 405 = absent
//!   (the live-proven 8.3 marker — the Phase-2 404 assumption was
//!   research-Pitfall-1 wrong, re-pinned 05-03), 402 = module
//!   unlicensed, 200 = present.
//!
//! EXIT CONTRACT (planner decision, README-documented): the doctor
//! exits 0 whenever the diagnosis COMPLETES — failing checks are the
//! product, not CLI errors (agents parse `checks[]`; humans read the
//! table). Config errors (no profile) still exit 3 through the normal
//! dispatch path.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::webdev::RouteProbe;
use crate::error::CoreError;

/// TCP dial timeout for the url check (separates DNS/firewall from
/// HTTP — the igw-cli pattern).
const DIAL_TIMEOUT: Duration = Duration::from_secs(3);

/// The state a healthy gateway reports on StatusPing.
const RUNNING: &str = "RUNNING";

/// One check row. The `checks[]` keys are contract: exactly
/// `{name, status, detail, hint}` (hint serializes as `null` when
/// absent — agents can key on it unconditionally).
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// Which check: url / liveness / commissioned / auth / permissions /
    /// write / webdev / rig.
    pub name: String,
    /// ok | warn | fail | skip.
    pub status: CheckStatus,
    /// What was observed.
    pub detail: String,
    /// The actionable next step, when there is one.
    pub hint: Option<String>,
}

/// A check's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// The check passed.
    Ok,
    /// Passed with something worth flagging (restart mid-flight,
    /// read-only token, route absent).
    Warn,
    /// The check failed — the detail + hint carry the diagnosis.
    Fail,
    /// Not requested or not assessable in this context.
    Skip,
}

/// The doctor's output model.
#[derive(Debug, Serialize)]
pub struct DoctorResult {
    /// The checks, in execution order (url, liveness, commissioned,
    /// auth, permissions, write, webdev, rig).
    pub checks: Vec<CheckResult>,
}

/// The doctor's options (the CLI's `--check-write` / `--webdev-route`;
/// no other options exist by design).
#[derive(Debug, Default)]
pub struct DoctorOptions {
    /// Probe write permission via the harmless scan/projects rescan.
    pub check_write: bool,
    /// Probe one WebDev route's presence (`/system/webdev/<NAME>`).
    pub webdev_route: Option<String>,
}

/// Run the check sequence. `profile_url` is the raw configured URL
/// (the url check parses it itself); `credential_present` makes the
/// 401 diagnosis honest (a missing credential is a DIFFERENT failure
/// than an unrecognized one). NEVER returns Err — the diagnosis
/// completing IS success (exit 0).
pub async fn doctor(
    api: &dyn GatewayApi,
    profile_url: &str,
    credential_present: bool,
    opts: &DoctorOptions,
) -> DoctorResult {
    let mut checks = Vec::with_capacity(8);

    // 1. url: parse + TCP dial (separates DNS/firewall from HTTP).
    checks.push(check_url(profile_url));

    // 2. liveness: the UNAUTHENTICATED StatusPing — down-ness and auth
    //    failure are separated by construction (no credential here).
    checks.push(check_liveness(api).await);

    // 3 + 4. commissioned + auth read: ONE gateway-info probe feeds
    //    both (the 302→/welcome classification runs before the
    //    401/403 reading of the same response).
    let gateway_info = api.gateway_info().await;
    checks.push(check_commissioned(&gateway_info));
    let auth_status = check_auth(&gateway_info, credential_present);
    checks.push(auth_status.clone());

    // 5. permissions deep-dive: when the token WORKS, surface the
    //    gateway's actual read/write permission wiring; when auth
    //    failed with 403, attempting the read confirms whether the
    //    token can read the security config at all (the three-part
    //    diagnosis's part 2).
    checks.push(check_permissions(api, &auth_status).await);

    // 6. write probe (only with --check-write).
    checks.push(check_write(api, opts).await);

    // 7. webdev route presence (only with --webdev-route).
    checks.push(check_webdev(api, opts).await);

    // 8. rig: local-only, no gateway calls.
    checks.push(check_rig());

    DoctorResult { checks }
}

/// Build a row.
fn row(name: &str, status: CheckStatus, detail: String, hint: Option<String>) -> CheckResult {
    CheckResult {
        name: name.to_string(),
        status,
        detail,
        hint,
    }
}

/// 1. url: parse the profile URL, then TCP dial host:port with a 3 s
///    timeout — a DNS or firewall failure is a DIFFERENT diagnosis
///    than an HTTP-level one (igw-cli pattern).
fn check_url(raw: &str) -> CheckResult {
    let url = match url::Url::parse(raw) {
        Ok(url) => url,
        Err(err) => {
            return row(
                "url",
                CheckStatus::Fail,
                format!("cannot parse the profile url {raw:?}: {err}"),
                Some("fix the profile url with `ign profile add`".to_string()),
            );
        }
    };
    let Some(host) = url.host_str().map(str::to_string) else {
        return row(
            "url",
            CheckStatus::Fail,
            format!("the profile url {raw:?} carries no host"),
            Some("fix the profile url with `ign profile add`".to_string()),
        );
    };
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = match (host.as_str(), port).to_socket_addrs() {
        Ok(addrs) => addrs.collect::<Vec<_>>(),
        Err(err) => {
            return row(
                "url",
                CheckStatus::Fail,
                format!("DNS resolution of {host} failed: {err}"),
                Some("check the hostname / VPN / DNS".to_string()),
            );
        }
    };
    let mut last_err = None;
    for addr in &addrs {
        match TcpStream::connect_timeout(addr, DIAL_TIMEOUT) {
            Ok(_) => {
                return row(
                    "url",
                    CheckStatus::Ok,
                    format!("TCP connect to {host}:{port} succeeded"),
                    None,
                );
            }
            Err(err) => last_err = Some(err),
        }
    }
    let err = last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            "no addresses resolved",
        )
    });
    row(
        "url",
        CheckStatus::Fail,
        format!("TCP connect to {host}:{port} failed: {err}"),
        Some(format!(
            "check the gateway host/port ({host}:{port}) and any firewall/VPN"
        )),
    )
}

/// 2. liveness: the unauth StatusPing — RUNNING / mid-restart state /
///    no answer. This check carries no credential BY CONSTRUCTION, so
///    its failure can never be an auth problem.
async fn check_liveness(api: &dyn GatewayApi) -> CheckResult {
    match api.status_ping().await {
        Ok(ping) if ping.state == RUNNING => row(
            "liveness",
            CheckStatus::Ok,
            format!("gateway {RUNNING} (unauthenticated /StatusPing)"),
            None,
        ),
        Ok(ping) => row(
            "liveness",
            CheckStatus::Warn,
            format!(
                "gateway {} — restarting or not ready (unauthenticated /StatusPing)",
                ping.state
            ),
            Some("gateway not RUNNING yet; try `ign wait gateway`".to_string()),
        ),
        Err(CoreError::GatewayRestarting { .. }) => row(
            "liveness",
            CheckStatus::Warn,
            "webserver up but services restarting (503)".to_string(),
            Some("try `ign wait restart`".to_string()),
        ),
        Err(err) => row(
            "liveness",
            CheckStatus::Fail,
            format!("gateway down — no /StatusPing answer: {err}"),
            Some("check the gateway process/container and the url row above".to_string()),
        ),
    }
}

/// 3. commissioned: a 302→`/welcome` on the gateway-info probe means
///    the gateway is uncommissioned (it 302s EVERY /data route at the
///    wizard — verified on a fresh container).
fn check_commissioned(
    gateway_info: &Result<crate::client::version::GatewayInfo, CoreError>,
) -> CheckResult {
    match gateway_info {
        Err(CoreError::GatewayNotCommissioned { .. }) => row(
            "commissioned",
            CheckStatus::Fail,
            "every /data route redirects to /welcome — gateway not commissioned".to_string(),
            Some("open http://<host>:<port>/welcome in a browser and complete the commissioning wizard".to_string()),
        ),
        _ => row(
            "commissioned",
            CheckStatus::Ok,
            "no /welcome redirect on /data routes".to_string(),
            None,
        ),
    }
}

/// 4. auth read: gateway-info with the credential — the verified
///    401-vs-403 split, with the no-credential case kept honest.
fn check_auth(
    gateway_info: &Result<crate::client::version::GatewayInfo, CoreError>,
    credential_present: bool,
) -> CheckResult {
    match gateway_info {
        Ok(info) => row(
            "auth",
            CheckStatus::Ok,
            format!(
                "gateway-info read succeeded (HTTP 200, gateway {})",
                info.ignition_version
            ),
            None,
        ),
        Err(CoreError::Auth { status: 401, .. }) => {
            let (detail, hint) = if credential_present {
                (
                    "token not recognized (HTTP 401 on gateway-info)".to_string(),
                    "the X-Ignition-API-Token header must be the FULL `name:key` string from the gateway UI (Platform→Security→API Keys); Basic auth does not work on 8.3 /data routes — create an API token".to_string(),
                )
            } else {
                (
                    "no credential resolved for this profile (gateway-info answered 401)".to_string(),
                    "set IGNITION_TOKEN (or the profile's token_env / keyring) to an API token".to_string(),
                )
            };
            row("auth", CheckStatus::Fail, detail, Some(hint))
        }
        Err(CoreError::Auth { status: 403, .. }) => row(
            "auth",
            CheckStatus::Fail,
            "token recognized but under-permitted (HTTP 403 on gateway-info)".to_string(),
            Some("Ignition token setup is three parts: (1) the token holds an adequate security level, (2) the gateway's read/write permissions include that level (default: only Authenticated/Roles/Administrator), (3) 'Require secure connections' is unchecked for http gateways — the permissions row below helps with part 2".to_string()),
        ),
        Err(CoreError::GatewayNotCommissioned { .. }) => row(
            "auth",
            CheckStatus::Skip,
            "gateway not commissioned — auth not assessable".to_string(),
            None,
        ),
        Err(CoreError::GatewayRestarting { .. }) => row(
            "auth",
            CheckStatus::Skip,
            "gateway restarting — auth not assessable yet".to_string(),
            Some("re-run doctor once the gateway is RUNNING".to_string()),
        ),
        Err(CoreError::Network { .. }) => row(
            "auth",
            CheckStatus::Skip,
            "gateway unreachable — auth not assessable".to_string(),
            None,
        ),
        Err(err) => row(
            "auth",
            CheckStatus::Fail,
            format!("gateway-info probe failed: {err}"),
            None,
        ),
    }
}

/// 5. permissions deep-dive: read the security-properties singleton
///    and surface the actual read/write permission wiring — the specific
///    instruction for the 403 case's part 2. Also attempted on a 403 (the
///    read failing too CONFIRMS the wiring diagnosis); skipped for other
///    auth failures (the read needs a working token).
async fn check_permissions(api: &dyn GatewayApi, auth: &CheckResult) -> CheckResult {
    let attempt = match auth.status {
        CheckStatus::Ok => true,
        // A 403 token is recognized — attempt the read; its failure is
        // itself diagnostic.
        CheckStatus::Fail if auth.detail.contains("403") => true,
        _ => false,
    };
    if !attempt {
        return row(
            "permissions",
            CheckStatus::Skip,
            "auth read failed — the security-properties read needs a working token".to_string(),
            None,
        );
    }
    match api.security_properties().await {
        Ok(props) => {
            let read = props
                .read_permissions
                .as_ref()
                .map(|value| serde_json::to_string(value).unwrap_or_default())
                .unwrap_or_else(|| "(absent)".to_string());
            let write = props
                .write_permissions
                .as_ref()
                .map(|value| serde_json::to_string(value).unwrap_or_default())
                .unwrap_or_else(|| "(absent)".to_string());
            row(
                "permissions",
                CheckStatus::Ok,
                format!("readPermissions: {read}; writePermissions: {write}"),
                None,
            )
        }
        Err(CoreError::Auth { status: 403, .. }) => row(
            "permissions",
            CheckStatus::Warn,
            "this token cannot read security-properties either (HTTP 403) — the gateway's read/write permissions likely exclude the token's security level (three-part cause 2)".to_string(),
            Some("in the gateway UI (Platform→Security→Permissions) add the token's security level to the read/write permission lists, or grant the token a level the permissions already include".to_string()),
        ),
        Err(err) => row(
            "permissions",
            CheckStatus::Warn,
            format!("could not read security-properties: {err}"),
            None,
        ),
    }
}

/// 6. write probe (only with --check-write): the harmless
///    scan/projects rescan — 2xx = write permission, 403 = read-only
///    token (igw-cli's choice; set+reset of a logger level would be more
///    visibly mutating and is deliberately NOT used).
async fn check_write(api: &dyn GatewayApi, opts: &DoctorOptions) -> CheckResult {
    if !opts.check_write {
        return row(
            "write",
            CheckStatus::Skip,
            "not requested (--check-write)".to_string(),
            None,
        );
    }
    match api.scan_projects().await {
        Ok(()) => row(
            "write",
            CheckStatus::Ok,
            "scan/projects accepted (2xx) — write permitted".to_string(),
            None,
        ),
        Err(CoreError::Auth { status: 403, .. }) => row(
            "write",
            CheckStatus::Warn,
            "read-only token (HTTP 403 on scan/projects)".to_string(),
            Some(
                "grant the token write permission or use a token with an adequate security level"
                    .to_string(),
            ),
        ),
        Err(err) => row(
            "write",
            CheckStatus::Fail,
            format!("scan/projects probe failed: {err}"),
            None,
        ),
    }
}

/// 7. webdev route presence (only with --webdev-route NAME): the
///    05-03 re-pin — probe the route's `version` action inside the
///    CLI's ign-cli project via `webdev_route_probe`. **405 = absent**
///    (the live-proven 8.3 marker; the Phase-2 404 assumption was
///    research-Pitfall-1 WRONG), 402 = module unlicensed,
///    200 = present (+ handshake version). The status code IS the
///    answer — never classified.
async fn check_webdev(api: &dyn GatewayApi, opts: &DoctorOptions) -> CheckResult {
    let Some(route) = opts.webdev_route.as_deref() else {
        return row(
            "webdev",
            CheckStatus::Skip,
            "not requested (--webdev-route NAME)".to_string(),
            None,
        );
    };
    match api
        .webdev_route_probe(crate::client::webdev::DEFAULT_PROJECT, route, &[])
        .await
    {
        Ok(RouteProbe::Present { route_version }) => row(
            "webdev",
            CheckStatus::Ok,
            format!("route {route:?} present (version {route_version})"),
            None,
        ),
        Ok(RouteProbe::Absent) => row(
            "webdev",
            CheckStatus::Warn,
            format!("route {route:?} absent (HTTP 405 — the 8.3 absent marker)"),
            Some(
                "run `ign webdev deploy` to install the CLI's routes (or check the \
                  route name)"
                    .to_string(),
            ),
        ),
        Ok(RouteProbe::Unlicensed) => row(
            "webdev",
            CheckStatus::Warn,
            "WebDev module unlicensed (HTTP 402 — trial-expired rigs cannot \
              serve /system/webdev routes)"
                .to_string(),
            Some(
                "license the gateway; on a rig, `ign rig trial reset --yes` restarts \
                  an expired trial"
                    .to_string(),
            ),
        ),
        Ok(RouteProbe::AuthGated) => row(
            "webdev",
            CheckStatus::Ok,
            format!("route {route:?} present (auth-gated — HTTP 401/403)"),
            None,
        ),
        Ok(RouteProbe::Denied { code, .. }) => row(
            "webdev",
            CheckStatus::Ok,
            format!("route {route:?} present (denied: {code})"),
            None,
        ),
        Err(err) => row(
            "webdev",
            CheckStatus::Fail,
            format!("route {route:?} probe failed: {err}"),
            None,
        ),
    }
}

/// 8. rig: local-only — Docker reachable → ok with its version;
///    absent → skip (Phase 4 owns real rig detection). No gateway calls.
fn check_rig() -> CheckResult {
    match std::process::Command::new("docker")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            row("rig", CheckStatus::Ok, version, None)
        }
        _ => row(
            "rig",
            CheckStatus::Skip,
            "no Docker / Phase 4 rig detection".to_string(),
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckStatus, DoctorOptions};
    use crate::client::GatewayApi;
    use crate::client::query::ListEnvelope;
    use crate::client::restart::SecurityProperties;
    use crate::client::status::StatusPing;
    use crate::client::version::GatewayInfo;
    use crate::error::CoreError;

    /// A scripted double serving fn-pointer results (CoreError is not
    /// Clone, so each call constructs its error fresh); everything
    /// else unreachable.
    struct DoctorRig {
        ping: fn() -> Result<StatusPing, CoreError>,
        info: fn() -> Result<GatewayInfo, CoreError>,
        props: fn() -> Result<SecurityProperties, CoreError>,
        webdev_probe: fn() -> Result<crate::client::webdev::RouteProbe, CoreError>,
    }

    fn tags_present() -> Result<crate::client::webdev::RouteProbe, CoreError> {
        Ok(crate::client::webdev::RouteProbe::Present {
            route_version: crate::webdev::ROUTE_BUNDLE_VERSION.to_string(),
        })
    }

    fn tags_absent() -> Result<crate::client::webdev::RouteProbe, CoreError> {
        Ok(crate::client::webdev::RouteProbe::Absent)
    }

    fn webdev_unlicensed() -> Result<crate::client::webdev::RouteProbe, CoreError> {
        Ok(crate::client::webdev::RouteProbe::Unlicensed)
    }

    fn running() -> Result<StatusPing, CoreError> {
        Ok(StatusPing {
            state: "RUNNING".into(),
        })
    }

    fn ok_info() -> Result<GatewayInfo, CoreError> {
        Ok(serde_json::from_value(serde_json::json!({
            "name": "GW",
            "edition": "standard",
            "ignitionVersion": "8.3.6 (b2026042713)"
        }))
        .expect("gateway-info fixture parses"))
    }

    fn ok_props() -> Result<SecurityProperties, CoreError> {
        Ok(serde_json::from_value(serde_json::json!({
            "readPermissions": {"anyOf": ["Authenticated/Roles/Administrator"]},
            "writePermissions": {"anyOf": ["Authenticated/Roles/Administrator"]}
        }))
        .expect("security-properties fixture parses"))
    }

    fn info_403() -> Result<GatewayInfo, CoreError> {
        Err(CoreError::Auth {
            status: 403,
            endpoint: None,
        })
    }

    fn info_401() -> Result<GatewayInfo, CoreError> {
        Err(CoreError::Auth {
            status: 401,
            endpoint: None,
        })
    }

    fn props_403() -> Result<SecurityProperties, CoreError> {
        Err(CoreError::Auth {
            status: 403,
            endpoint: None,
        })
    }

    fn props_401() -> Result<SecurityProperties, CoreError> {
        Err(CoreError::Auth {
            status: 401,
            endpoint: None,
        })
    }

    #[async_trait::async_trait]
    impl GatewayApi for DoctorRig {
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
        async fn gateway_info(&self) -> Result<GatewayInfo, CoreError> {
            (self.info)()
        }
        async fn status_ping(&self) -> Result<StatusPing, CoreError> {
            (self.ping)()
        }
        async fn security_properties(&self) -> Result<SecurityProperties, CoreError> {
            (self.props)()
        }
        async fn scan_projects(&self) -> Result<(), CoreError> {
            Err(CoreError::Auth {
                status: 403,
                endpoint: None,
            })
        }
        async fn restart(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unreachable!("not part of this action")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::status::ModuleInfo>, CoreError> {
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
        ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::VisionClient>, CoreError> {
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
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LogEntry>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &crate::client::query::ListQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LoggerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
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
        async fn webdev_route_probe(
            &self,
            _project: &str,
            _route: &str,
            _extra_headers: &[(&str, &str)],
        ) -> Result<crate::client::webdev::RouteProbe, CoreError> {
            (self.webdev_probe)()
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
        async fn project_import(
            &self,
            _name: &str,
            _zip: Vec<u8>,
            _overwrite: bool,
        ) -> Result<crate::client::projects::ImportOutcome, CoreError> {
            unreachable!("not part of this action")
        }
    }

    fn healthy_rig() -> DoctorRig {
        DoctorRig {
            ping: running,
            info: ok_info,
            props: ok_props,
            webdev_probe: tags_present,
        }
    }

    /// The check ORDER is contract (README documents the table): url,
    /// liveness, commissioned, auth, permissions, write, webdev, rig.
    #[tokio::test]
    async fn checks_run_in_the_documented_order() {
        let result = super::doctor(
            &healthy_rig(),
            "http://127.0.0.1:1",
            true,
            &DoctorOptions::default(),
        )
        .await;
        let names: Vec<&str> = result.checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "url",
                "liveness",
                "commissioned",
                "auth",
                "permissions",
                "write",
                "webdev",
                "rig"
            ],
        );
        // A healthy rig: url FAILS (dead port dial), everything gateway
        // side is ok, write/webdev skip without their flags.
        let by_name = |name: &str| {
            result
                .checks
                .iter()
                .find(|c| c.name == name)
                .unwrap_or_else(|| panic!("{name} row present"))
        };
        assert_eq!(by_name("liveness").status, CheckStatus::Ok);
        assert_eq!(by_name("commissioned").status, CheckStatus::Ok);
        assert_eq!(by_name("auth").status, CheckStatus::Ok);
        assert_eq!(by_name("permissions").status, CheckStatus::Ok);
        assert_eq!(by_name("write").status, CheckStatus::Skip);
        assert_eq!(by_name("webdev").status, CheckStatus::Skip);
    }

    /// The healthy permissions row surfaces the ACTUAL wiring
    /// (readPermissions/writePermissions verbatim).
    #[tokio::test]
    async fn healthy_permissions_row_surfaces_the_wiring() {
        let result = super::doctor(
            &healthy_rig(),
            "http://127.0.0.1:1",
            true,
            &DoctorOptions::default(),
        )
        .await;
        let perms = result
            .checks
            .iter()
            .find(|c| c.name == "permissions")
            .unwrap();
        assert!(
            perms.detail.contains("readPermissions"),
            "detail: {}",
            perms.detail
        );
        assert!(
            perms.detail.contains("Authenticated/Roles/Administrator"),
            "the wiring value surfaces verbatim: {}",
            perms.detail
        );
    }

    /// The 403 wiring diagnosis: auth fails with the three-part hint
    /// AND the permissions deep-dive CONFIRMS the token cannot read
    /// the security config either (the part-2 confirmation).
    #[tokio::test]
    async fn the_403_case_carries_the_three_part_hint_and_permissions_detail() {
        let rig = DoctorRig {
            ping: running,
            info: info_403,
            props: props_403,
            webdev_probe: tags_present,
        };
        let result =
            super::doctor(&rig, "http://127.0.0.1:1", true, &DoctorOptions::default()).await;
        let auth = result.checks.iter().find(|c| c.name == "auth").unwrap();
        assert_eq!(auth.status, CheckStatus::Fail);
        let hint = auth.hint.as_deref().unwrap();
        assert!(hint.contains("three parts"), "hint: {hint}");
        assert!(hint.contains("permissions"), "hint: {hint}");
        let perms = result
            .checks
            .iter()
            .find(|c| c.name == "permissions")
            .unwrap();
        assert_eq!(perms.status, CheckStatus::Warn);
        assert!(
            perms.detail.contains("cannot read security-properties"),
            "detail: {}",
            perms.detail
        );
    }

    /// A no-credential 401 is diagnosed as UNCONFIGURED, not as a bad
    /// token — the honest split; permissions skip (needs a working
    /// token).
    #[tokio::test]
    async fn the_no_credential_401_is_its_own_diagnosis() {
        let rig = DoctorRig {
            ping: running,
            info: info_401,
            props: props_401,
            webdev_probe: tags_present,
        };
        let result =
            super::doctor(&rig, "http://127.0.0.1:1", false, &DoctorOptions::default()).await;
        let auth = result.checks.iter().find(|c| c.name == "auth").unwrap();
        assert_eq!(auth.status, CheckStatus::Fail);
        assert!(
            auth.detail.contains("no credential resolved"),
            "detail: {}",
            auth.detail
        );
        assert!(
            auth.hint.as_deref().unwrap().contains("IGNITION_TOKEN"),
            "hint names the fix"
        );
        let perms = result
            .checks
            .iter()
            .find(|c| c.name == "permissions")
            .unwrap();
        assert_eq!(perms.status, CheckStatus::Skip);
    }

    /// A token-present 401 names the name:key format failure.
    #[tokio::test]
    async fn the_token_401_names_the_name_key_format() {
        let rig = DoctorRig {
            ping: running,
            info: info_401,
            props: props_401,
            webdev_probe: tags_present,
        };
        let result =
            super::doctor(&rig, "http://127.0.0.1:1", true, &DoctorOptions::default()).await;
        let auth = result.checks.iter().find(|c| c.name == "auth").unwrap();
        assert!(
            auth.hint.as_deref().unwrap().contains("name:key"),
            "hint: {:?}",
            auth.hint
        );
    }

    /// The url check parses and TCP-dials: a dead port FAILS with a
    /// connect diagnosis (127.0.0.1:1 refuses instantly).
    #[tokio::test]
    async fn url_check_dials_and_reports_a_dead_port() {
        let result = super::doctor(
            &healthy_rig(),
            "http://127.0.0.1:1",
            true,
            &DoctorOptions::default(),
        )
        .await;
        let url = result.checks.first().unwrap();
        assert_eq!(url.status, CheckStatus::Fail);
        assert!(url.detail.contains("TCP connect"), "detail: {}", url.detail);
    }

    /// The write probe: 403 → warn "read-only token" (the rig's
    /// scan_projects answers 403); skipped without --check-write.
    #[tokio::test]
    async fn write_probe_warns_read_only_on_403() {
        let result = super::doctor(
            &healthy_rig(),
            "http://127.0.0.1:1",
            true,
            &DoctorOptions {
                check_write: true,
                webdev_route: None,
            },
        )
        .await;
        let write = result.checks.iter().find(|c| c.name == "write").unwrap();
        assert_eq!(write.status, CheckStatus::Warn);
        assert!(
            write.detail.contains("read-only token"),
            "detail: {}",
            write.detail
        );
    }

    /// THE 05-03 re-pin: a 405 answer is ABSENT (warn + `ign webdev
    /// deploy` hint) — replacing the documented-but-wrong Phase-2 404
    /// assumption (research Pitfall 1).
    #[tokio::test]
    async fn webdev_405_means_absent_with_a_deploy_hint() {
        let rig = DoctorRig {
            webdev_probe: tags_absent,
            ..healthy_rig()
        };
        let result = super::doctor(
            &rig,
            "http://127.0.0.1:1",
            true,
            &DoctorOptions {
                check_write: false,
                webdev_route: Some("tags".into()),
            },
        )
        .await;
        let webdev = result.checks.iter().find(|c| c.name == "webdev").unwrap();
        assert_eq!(webdev.status, CheckStatus::Warn);
        assert!(
            webdev.detail.contains("405"),
            "the 405 marker surfaces: {}",
            webdev.detail
        );
        assert!(
            webdev
                .hint
                .as_deref()
                .unwrap()
                .contains("ign webdev deploy"),
            "hint names the fix"
        );
    }

    /// A present route answers ok with its handshake version; a 402
    /// rig warns "module unlicensed" (the trial-expired state).
    #[tokio::test]
    async fn webdev_present_ok_and_402_unlicensed() {
        let result = super::doctor(
            &healthy_rig(),
            "http://127.0.0.1:1",
            true,
            &DoctorOptions {
                check_write: false,
                webdev_route: Some("tags".into()),
            },
        )
        .await;
        let webdev = result.checks.iter().find(|c| c.name == "webdev").unwrap();
        assert_eq!(webdev.status, CheckStatus::Ok);
        assert!(
            webdev.detail.contains("present (version"),
            "detail carries the handshake version: {}",
            webdev.detail
        );

        let rig = DoctorRig {
            webdev_probe: webdev_unlicensed,
            ..healthy_rig()
        };
        let result = super::doctor(
            &rig,
            "http://127.0.0.1:1",
            true,
            &DoctorOptions {
                check_write: false,
                webdev_route: Some("tags".into()),
            },
        )
        .await;
        let webdev = result.checks.iter().find(|c| c.name == "webdev").unwrap();
        assert_eq!(webdev.status, CheckStatus::Warn);
        assert!(
            webdev.detail.contains("unlicensed"),
            "detail: {}",
            webdev.detail
        );
    }

    /// Serialization pins: statuses are lowercase; the checks[] keys
    /// are exactly {name, status, detail, hint} with hint null-able.
    #[test]
    fn check_result_serializes_with_exactly_four_keys() {
        let body = serde_json::to_value(super::CheckResult {
            name: "auth".into(),
            status: CheckStatus::Fail,
            detail: "detail".into(),
            hint: None,
        })
        .expect("serialize");
        assert_eq!(
            body,
            serde_json::json!({
                "name": "auth",
                "status": "fail",
                "detail": "detail",
                "hint": null
            })
        );
    }
}
