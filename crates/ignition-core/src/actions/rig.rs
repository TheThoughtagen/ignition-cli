//! Rig lifecycle actions (04-01, RIG-01): `up` / `down` / `status` —
//! serde models OUT, no printing (the TUI rides this layer in Phase 6).
//!
//! Every action takes [`&dyn ComposeRunner`] (the Task-1 seam) so the
//! full decision tree is unit-testable without docker. `rig_up`
//! additionally takes an OPTIONAL gateway probe: the dispatch derives
//! the rig's gateway URL ([`gateway_url_from`]) and builds a
//! HEADER-LESS client pointed at it — the commissioned wait probes
//! `/StatusPing` on the RIG's own port, never the profile's gateway.
//!
//! ## Uncommissioned is DATA, not failure (research Pattern 4)
//!
//! A fresh-volume rig terminally reports `"up, uncommissioned"` — exit
//! 0 with the wizard URL inside `warnings` (the version-command
//! degradation precedent). There is NO headless commissioning (verified:
//! no commissioning endpoints in 83-api); the wait deadline only covers
//! STARTING→RUNNING on an already-commissioned volume.
//!
//! ## The wait reuses poll.rs VERBATIM (locked)
//!
//! [`crate::poll`] is THE wait engine — its retry set is LOCKED and
//! untouched. Probe translation (research Pattern 4): `RUNNING` →
//! Done; other states → Pending; `GatewayNotCommissioned` → the probe
//! ITSELF catches it and returns Pending with the wizard hint (never
//! aborts); Network/GatewayRestarting propagate for poll's native
//! retry; Auth can't fire (the probe is header-less).

use std::cell::Cell;
use std::time::Duration;

use serde::Serialize;

use crate::client::GatewayApi;
use crate::rig::compose::{
    check_output, compose_version, down_args, docker_ps_publish_args, parse_docker_ps_ldjson,
    parse_ps_ldjson, parse_volume_ls_ldjson, ps_args, up_args, volume_ls_args, ComposeRunner,
};
use crate::error::CoreError;
use crate::poll::{self, PollConfig, PollState};
use crate::rig::{RigPlan, port_preflight};

/// Default wait budget for BOTH `up --wait-timeout` and the
/// commissioned probe deadline (research Pitfall 3: healthchecks block
/// `--wait`, and image pulls add minutes).
pub const DEFAULT_WAIT_TIMEOUT_S: u64 = 300;

/// The state a fully-ready gateway reports on `/StatusPing`.
const RUNNING: &str = "RUNNING";

/// Gateway target ports (the documented heuristic inputs).
const GATEWAY_HTTP_TARGET: u16 = 8088;
const GATEWAY_HTTPS_TARGET: u16 = 443;

/// `ign rig up` output model (all keys always present).
#[derive(Debug, Serialize)]
pub struct RigUpResult {
    /// Compose project name — the identity truth.
    pub rig: String,
    /// Compose project name (same truth; kept distinct from `rig` for
    /// future alias-vs-project divergence).
    pub project: String,
    /// `"running"` | `"uncommissioned"`.
    pub state: String,
    /// The derived gateway URL (what the commissioned wait probed);
    /// `null` when no 8088/443 mapping exists.
    pub gateway_url: Option<String>,
    /// Data-level warnings (uncommissioned wizard hint, skipped-wait
    /// note) — exit 0 carries them here, never on stderr.
    pub warnings: Vec<String>,
}

/// `ign rig down` output model.
#[derive(Debug, Serialize)]
pub struct RigDownResult {
    /// Compose project name.
    pub rig: String,
    /// Compose project name.
    pub project: String,
    /// Always `"down"` on success.
    pub state: String,
}

/// One published-port row in status output (allowlist only).
#[derive(Debug, Serialize)]
pub struct StatusPublisher {
    /// Host port.
    pub published_port: Option<u16>,
    /// Container port.
    pub target_port: Option<u16>,
    /// `tcp`/`udp`.
    pub protocol: Option<String>,
}

/// One service row in status output (allowlist only).
#[derive(Debug, Serialize)]
pub struct StatusService {
    /// Service name.
    pub name: String,
    /// `running` / `exited` / …
    pub state: String,
    /// `healthy` / `starting` / `null`.
    pub health: Option<String>,
    /// Last exit code.
    pub exit_code: Option<i64>,
    /// Published ports.
    pub publishers: Vec<StatusPublisher>,
}

/// `ign rig status` output model — an ALLOWLIST, never a compose
/// config/inspect passthrough (the resolved config contains
/// `GATEWAY_ADMIN_PASSWORD` etc.; research anti-pattern).
#[derive(Debug, Serialize)]
pub struct RigStatusResult {
    /// Compose project name.
    pub rig: String,
    /// Compose project name.
    pub project: String,
    /// The compose file the rig resolved from.
    pub compose_file: String,
    /// One row per RUNNING-OR-EXITED compose service (empty when the
    /// rig is down — state is data, exit 0).
    pub services: Vec<StatusService>,
    /// Named volumes `reset` would remove (project-labeled).
    pub volumes: Vec<String>,
    /// True when NO docker container currently publishes any of the
    /// rig's host ports (a running rig holds its own ports → false).
    pub ports_free: bool,
}

/// Derive the rig's gateway URL from the resolved port mappings — the
/// DOCUMENTED HEURISTIC: the first mapping targeting the gateway's http
/// port (8088) wins, then its https port (443), else nothing. `data`
/// carries what was derived (`gateway_url`), so agents can see the
/// heuristic's answer.
pub fn gateway_url_from(plan: &RigPlan) -> Option<String> {
    if let Some(mapping) = plan
        .port_mappings
        .iter()
        .find(|mapping| mapping.target == GATEWAY_HTTP_TARGET)
    {
        return Some(format!("http://localhost:{}", mapping.published));
    }
    if let Some(mapping) = plan
        .port_mappings
        .iter()
        .find(|mapping| mapping.target == GATEWAY_HTTPS_TARGET)
    {
        return Some(format!("https://localhost:{}", mapping.published));
    }
    None
}

/// `ign rig up`: version gate → port pre-flight → `up -d --wait` →
/// commissioned wait (poll.rs) with uncommissioned-as-data semantics.
///
/// `gateway` is the optional probe client the dispatch builds from
/// [`gateway_url_from`] — when absent (or when no gateway port is
/// derivable) the wait is skipped with a data-level warning and the
/// compose `--wait` result stands on its own.
pub async fn rig_up(
    runner: &dyn ComposeRunner,
    plan: &RigPlan,
    wait_timeout_s: u64,
    gateway: Option<&dyn GatewayApi>,
) -> Result<RigUpResult, CoreError> {
    // 1. Fail fast on a missing/too-old compose (exit 7 + install hint).
    compose_version(runner).await?;

    // 2. Port pre-flight — cross-project occupants abort BEFORE any
    //    container is touched, with attribution (first conflict named;
    //    pre-flight collected them all).
    if let Some(conflict) = port_preflight(runner, plan).await?.first() {
        return Err(CoreError::Rig(format!(
            "port {} in use by {} — stop it or change the rig's published port",
            conflict.port, conflict.attribution
        )));
    }

    // 3. The up itself.
    let output = runner.run(&up_args(plan, wait_timeout_s)).await;
    check_output(&output, "docker compose up")?;

    let gateway_url = gateway_url_from(plan);
    let mut warnings = Vec::new();
    let state = match (gateway, &gateway_url) {
        (Some(api), Some(url)) => {
            commissioned_wait(api, url, wait_timeout_s, &mut warnings).await?
        }
        _ => {
            warnings.push(
                "no gateway port mapping (target 8088/443) found — skipped the \
                 commissioned wait"
                    .to_string(),
            );
            "running".to_string()
        }
    };

    Ok(RigUpResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        state,
        gateway_url,
        warnings,
    })
}

/// The commissioned wait (research Pattern 4): poll the rig's own
/// `/StatusPing` (header-less — auth can never block readiness) until
/// RUNNING. Deadline expiry with a terminal uncommissioned observation
/// degrades to SUCCESS-as-data (`state = "uncommissioned"` + wizard
/// hint in `warnings`); expiry while merely STARTING is a Rig error.
async fn commissioned_wait(
    api: &dyn GatewayApi,
    url: &str,
    wait_timeout_s: u64,
    warnings: &mut Vec<String>,
) -> Result<String, CoreError> {
    let cfg = PollConfig {
        subject: format!("rig gateway RUNNING (GET {url}/StatusPing)"),
        interval: Duration::from_secs(2),
        deadline: Duration::from_secs(wait_timeout_s),
        ..PollConfig::default()
    };
    // The poll-owned state: a borrowed Cell (the 02-05 HRTB shape) that
    // remembers a terminal-uncommissioned observation across probes.
    let mut uncommissioned = Cell::new(false);
    let url_owned = url.to_string();
    let outcome = poll::poll(cfg, &mut uncommissioned, |uncommissioned| {
        Box::pin(async {
            match api.status_ping().await {
                Ok(ping) if ping.state == RUNNING => Ok(PollState::<()>::Done(())),
                Ok(ping) => Ok(PollState::Pending(Some(ping.state))),
                // Probe-side translation (locked): never abort on the
                // wizard redirect — remember it, keep waiting.
                Err(CoreError::GatewayNotCommissioned { .. }) => {
                    uncommissioned.set(true);
                    Ok(PollState::Pending(Some(format!(
                        "gateway uncommissioned — open {url_owned}/welcome"
                    ))))
                }
                // Network/GatewayRestarting: poll's native retry set.
                Err(other) => Err(other),
            }
        })
    })
    .await;
    match outcome {
        Ok(()) => Ok("running".to_string()),
        // The DEADLINE error only (the locked source:None marker from
        // 02-04) degrades to data when the terminal observation was
        // uncommissioned — any other error class stays an error.
        Err(CoreError::Network { source: None, .. }) if uncommissioned.get() => {
            warnings.push(format!(
                "gateway uncommissioned — open {url}/welcome in a browser and complete \
                 the commissioning wizard (no headless commissioning exists)"
            ));
            Ok("uncommissioned".to_string())
        }
        Err(other) => Err(CoreError::Rig(format!(
            "gateway did not reach RUNNING within {wait_timeout_s}s — {other}"
        ))),
    }
}

/// `ign rig down`: version gate → `down --remove-orphans` (volumes
/// KEPT — the `-v` teardown half belongs to `rig reset`, 04-02).
pub async fn rig_down(runner: &dyn ComposeRunner, plan: &RigPlan) -> Result<RigDownResult, CoreError> {
    compose_version(runner).await?;
    let output = runner.run(&down_args(plan, false)).await;
    check_output(&output, "docker compose down")?;
    Ok(RigDownResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        state: "down".to_string(),
    })
}

/// `ign rig status`: version gate → `ps` LDJSON → `volume ls` →
/// port occupancy — serialized as an ALLOWLIST (services' state/health/
/// publishers, volume names, identity). Exit 0 even when the rig is
/// down: state is data.
pub async fn rig_status(runner: &dyn ComposeRunner, plan: &RigPlan) -> Result<RigStatusResult, CoreError> {
    compose_version(runner).await?;

    let ps = runner.run(&ps_args(plan)).await;
    let rows = parse_ps_ldjson(check_output(&ps, "docker compose ps")?);

    let volume_ls = runner.run_docker(&volume_ls_args(&plan.name)).await;
    let volumes = parse_volume_ls_ldjson(check_output(&volume_ls, "docker volume ls")?)
        .into_iter()
        .map(|entry| entry.name)
        .collect();

    // Occupancy signal: any docker container (own project included —
    // a running rig holds its own ports) publishing a rig port.
    let mut ports_free = true;
    for port in &plan.host_ports {
        let output = runner.run_docker(&docker_ps_publish_args(*port)).await;
        let occupants = parse_docker_ps_ldjson(check_output(&output, "docker ps")?);
        if !occupants.is_empty() {
            ports_free = false;
        }
    }

    let services = rows
        .into_iter()
        .map(|row| StatusService {
            name: if row.service.is_empty() {
                row.name
            } else {
                row.service
            },
            state: row.state,
            health: row.health,
            exit_code: row.exit_code,
            publishers: row
                .publishers
                .into_iter()
                .map(|publisher| StatusPublisher {
                    published_port: publisher.published_port,
                    target_port: publisher.target_port,
                    protocol: publisher.protocol,
                })
                .collect(),
        })
        .collect();

    Ok(RigStatusResult {
        rig: plan.name.clone(),
        project: plan.name.clone(),
        compose_file: plan.compose_file.display().to_string(),
        services,
        volumes,
        ports_free,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::{
        DEFAULT_WAIT_TIMEOUT_S, RigDownResult, RigStatusResult, RigUpResult, gateway_url_from,
        rig_down, rig_status, rig_up,
    };
    use crate::rig::compose::{ComposeOutput, ComposeRunner, PortMapping, down_args, up_args};
    use crate::error::CoreError;
    use crate::rig::RigPlan;

    // ---------------------------------------------------------------------
    // Test doubles
    // ---------------------------------------------------------------------

    /// Scripted runner: records every (program, args) call and serves
    /// queued outputs FIFO (same seam shape as the rig module's fake).
    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<(&'static str, Vec<String>)>>,
        outputs: Mutex<VecDeque<ComposeOutput>>,
    }

    impl FakeRunner {
        fn with(outputs: Vec<ComposeOutput>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into()),
                ..Self::default()
            }
        }

        fn calls(&self) -> Vec<(&'static str, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ComposeRunner for FakeRunner {
        async fn run(&self, args: &[String]) -> ComposeOutput {
            self.calls.lock().unwrap().push(("docker compose", args.to_vec()));
            self.outputs.lock().unwrap().pop_front().expect("outputs exhausted")
        }

        async fn run_docker(&self, args: &[String]) -> ComposeOutput {
            self.calls.lock().unwrap().push(("docker", args.to_vec()));
            self.outputs.lock().unwrap().pop_front().expect("outputs exhausted")
        }
    }

    fn ok(stdout: &str) -> ComposeOutput {
        ComposeOutput {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        }
    }

    fn version_ok() -> ComposeOutput {
        ok("Docker Compose version v5.1.2\n")
    }

    /// A one-service rig publishing the gateway ports (the
    /// gateway_url_from inputs).
    fn gw_plan() -> RigPlan {
        RigPlan {
            name: "fixture-rig".into(),
            compose_file: "/rigs/docker/compose.yml".into(),
            project_dir: "/rigs/docker".into(),
            services: vec!["ignition".into()],
            host_ports: vec![9088, 9443],
            port_mappings: vec![
                PortMapping {
                    target: 8088,
                    published: 9088,
                },
                PortMapping {
                    target: 443,
                    published: 9443,
                },
            ],
            volumes: vec!["gw-data".into()],
        }
    }

    /// A wiremock StatusPing responder serving one canned state.
    async fn status_ping_server(state: &str) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/StatusPing"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "state": state }),
            ))
            .expect(1..)
            .mount(&server)
            .await;
        server
    }

    /// A wiremock StatusPing responder that ALWAYS 302s to /welcome —
    /// the uncommissioned gateway shape (classify maps it to
    /// GatewayNotCommissioned; the probe must translate to Pending).
    async fn uncommissioned_server() -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/StatusPing"))
            .respond_with(wiremock::ResponseTemplate::new(302).insert_header(
                "Location",
                "/welcome",
            ))
            .expect(1..)
            .mount(&server)
            .await;
        server
    }

    // ---------------------------------------------------------------------
    // gateway_url_from
    // ---------------------------------------------------------------------

    #[test]
    fn gateway_url_prefers_http_8088_then_https_443() {
        assert_eq!(
            gateway_url_from(&gw_plan()),
            Some("http://localhost:9088".to_string()),
            "the 8088 mapping wins even though 443 is also present"
        );
        let https_only = RigPlan {
            port_mappings: vec![PortMapping {
                target: 443,
                published: 9443,
            }],
            host_ports: vec![9443],
            ..gw_plan()
        };
        assert_eq!(
            gateway_url_from(&https_only),
            Some("https://localhost:9443".to_string())
        );
        let no_gateway = RigPlan {
            port_mappings: vec![PortMapping {
                target: 22,
                published: 9022,
            }],
            host_ports: vec![9022],
            ..gw_plan()
        };
        assert_eq!(gateway_url_from(&no_gateway), None);
    }

    #[test]
    fn default_wait_timeout_is_300() {
        assert_eq!(DEFAULT_WAIT_TIMEOUT_S, 300, "research Pitfall 3 headroom");
    }

    // ---------------------------------------------------------------------
    // rig_up
    // ---------------------------------------------------------------------

    /// Success path: version → free-port preflight → up (exact args) →
    /// probe sees RUNNING immediately → state running, no warnings.
    #[tokio::test]
    async fn up_success_probes_to_running() {
        let server = status_ping_server("RUNNING").await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(vec![version_ok(), ok(""), ok(""), ok("")]);
        let result = rig_up(&runner, &gw_plan(), 300, Some(&api))
            .await
            .expect("up succeeds");
        assert_eq!(result.rig, "fixture-rig");
        assert_eq!(result.state, "running");
        assert!(result.warnings.is_empty());
        assert_eq!(result.gateway_url.as_deref(), Some("http://localhost:9088"));

        let calls = runner.calls();
        assert_eq!(calls[0], ("docker compose", vec!["version".to_string()]));
        // The pre-flight ran per host port via PLAIN docker.
        assert_eq!(calls[1].0, "docker");
        assert_eq!(calls[2].0, "docker");
        assert_eq!(
            calls[3],
            ("docker compose", up_args(&gw_plan(), 300)),
            "up rides the LOCKED arg shape"
        );
    }

    /// The uncommissioned degradation: the probe only ever sees the
    /// wizard redirect; the deadline expires; the result is SUCCESS as
    /// data (state uncommissioned, wizard hint inside warnings).
    #[tokio::test]
    async fn up_uncommissioned_is_data_not_failure() {
        let server = uncommissioned_server().await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(vec![version_ok(), ok(""), ok(""), ok("")]);
        let result = rig_up(&runner, &gw_plan(), 1, Some(&api))
            .await
            .expect("uncommissioned is exit-0 data");
        assert_eq!(result.state, "uncommissioned");
        assert_eq!(result.gateway_url.as_deref(), Some("http://localhost:9088"));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("http://localhost:9088/welcome")),
            "wizard URL in warnings: {:?}",
            result.warnings
        );
    }

    /// STARTING at deadline is a REAL failure (Rig error), not data.
    #[tokio::test]
    async fn up_still_starting_at_deadline_is_rig_error() {
        let server = status_ping_server("STARTING").await;
        let api = crate::client::ReqwestGatewayApi::for_tests(&server.uri(), None);

        let runner = FakeRunner::with(vec![version_ok(), ok(""), ok(""), ok("")]);
        let err = rig_up(&runner, &gw_plan(), 1, Some(&api))
            .await
            .expect_err("still-STARTING deadline errors");
        assert!(matches!(err, CoreError::Rig(_)));
        assert_eq!(err.exit_code(), 7);
        let message = err.to_string();
        assert!(message.contains("did not reach RUNNING"), "{message}");
        assert!(message.contains("STARTING"), "last observation named: {message}");
    }

    /// Port conflict aborts BEFORE the up, with attribution.
    #[tokio::test]
    async fn up_port_conflict_aborts_with_attribution() {
        let occupant = r#"{"Names":"other-gw-1","Labels":"com.docker.compose.project=other"}"#;
        let runner = FakeRunner::with(vec![version_ok(), ok(occupant), ok(occupant)]);

        let err = rig_up(&runner, &gw_plan(), 300, None)
            .await
            .expect_err("cross-project occupant aborts");
        let message = err.to_string();
        assert!(
            message.contains("port 9088 in use by container other-gw-1 (rig other)"),
            "{message}"
        );
        // The up NEVER ran — the last recorded call is the pre-flight.
        let calls = runner.calls();
        assert_eq!(
            calls.len(),
            3,
            "version + two port checks only, no up: {calls:?}"
        );
    }

    /// No probe client → the wait is skipped with a data-level warning
    /// and compose's own --wait stands as the readiness signal.
    #[tokio::test]
    async fn up_without_probe_skips_wait_with_warning() {
        let runner = FakeRunner::with(vec![version_ok(), ok(""), ok(""), ok("")]);
        let result = rig_up(&runner, &gw_plan(), 300, None)
            .await
            .expect("up succeeds without a probe");
        assert_eq!(result.state, "running");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("skipped the commissioned wait")),
            "{:?}",
            result.warnings
        );
    }

    /// Compose missing → version gate fails fast with the install hint.
    #[tokio::test]
    async fn up_missing_compose_fails_fast() {
        let missing = ComposeOutput {
            stdout: String::new(),
            stderr: "docker: command not found".into(),
            code: 127,
        };
        let runner = FakeRunner::with(vec![missing]);
        let err = rig_up(&runner, &gw_plan(), 300, None)
            .await
            .expect_err("no docker errors");
        let message = err.to_string();
        assert!(message.contains("docker compose is unavailable"), "{message}");
        assert!(message.contains("not supported"), "{message}");
    }

    // ---------------------------------------------------------------------
    // rig_down
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn down_runs_exact_args_and_reports_down() {
        let runner = FakeRunner::with(vec![version_ok(), ok("")]);
        let result = rig_down(&runner, &gw_plan()).await.expect("down succeeds");
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "rig": "fixture-rig",
                "project": "fixture-rig",
                "state": "down",
            }),
            "RigDownResult shape (all keys always)"
        );
        let calls = runner.calls();
        assert_eq!(calls[1], ("docker compose", down_args(&gw_plan(), false)));
    }

    #[tokio::test]
    async fn down_failure_carries_stderr_tail() {
        let runner = FakeRunner::with(vec![
            version_ok(),
            ComposeOutput {
                stdout: String::new(),
                stderr: "error while removing network: active endpoints".into(),
                code: 1,
            },
        ]);
        let err = rig_down(&runner, &gw_plan())
            .await
            .expect_err("down failure errors");
        let message = err.to_string();
        assert!(message.contains("docker compose down failed (exit 1)"), "{message}");
        assert!(message.contains("active endpoints"), "{message}");
    }

    // ---------------------------------------------------------------------
    // rig_status — the allowlist pin
    // ---------------------------------------------------------------------

    /// ps LDJSON with a Publishers array + a second bare service.
    const PS_STDOUT: &str = concat!(
        r#"{"Name":"fixture-rig-ignition-1","Service":"ignition","State":"running","Health":"healthy","ExitCode":0,"Publishers":[{"URL":"0.0.0.0","TargetPort":8088,"PublishedPort":9088,"Protocol":"tcp"},{"URL":"0.0.0.0","TargetPort":443,"PublishedPort":9443,"Protocol":"tcp"}]}"#,
        "\n",
        r#"{"Name":"fixture-rig-db-1","Service":"db","State":"exited","ExitCode":137,"Publishers":[]}"#,
        "\n",
    );

    const VOLUME_STDOUT: &str = concat!(
        r#"{"Name":"fixture-rig_gw-data","Labels":{"com.docker.compose.project":"fixture-rig"}}"#,
        "\n",
    );

    #[tokio::test]
    async fn status_serializes_the_allowlist_exactly() {
        // Call order: version(run), ps(run), volume ls(docker), then a
        // docker ps per host port (2 ports), each showing an occupant
        // (ports_free=false — a running rig holds its own ports).
        let occupant = r#"{"Names":"fixture-rig-ignition-1","Labels":"com.docker.compose.project=fixture-rig"}"#;
        let runner = FakeRunner::with(vec![
            version_ok(),
            ok(PS_STDOUT),
            ok(VOLUME_STDOUT),
            ok(occupant),
            ok(occupant),
        ]);

        let result = rig_status(&runner, &gw_plan())
            .await
            .expect("status succeeds");
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "rig": "fixture-rig",
                "project": "fixture-rig",
                "compose_file": "/rigs/docker/compose.yml",
                "services": [
                    {
                        "name": "ignition",
                        "state": "running",
                        "health": "healthy",
                        "exit_code": 0,
                        "publishers": [
                            {"published_port": 9088, "target_port": 8088, "protocol": "tcp"},
                            {"published_port": 9443, "target_port": 443, "protocol": "tcp"}
                        ]
                    },
                    {
                        "name": "db",
                        "state": "exited",
                        "health": null,
                        "exit_code": 137,
                        "publishers": []
                    }
                ],
                "volumes": ["fixture-rig_gw-data"],
                "ports_free": false
            }),
            "EXACT shape comparison: no compose-config passthrough, no \
             unknown keys — the allowlist IS the contract"
        );
    }

    /// A down rig: empty ps, empty docker-ps per port → exit 0, empty
    /// services, ports_free true (state is data).
    #[tokio::test]
    async fn status_down_rig_is_data() {
        let runner = FakeRunner::with(vec![
            version_ok(),
            ok(""),
            ok(""),
            ok(""),
            ok(""),
        ]);
        let result = rig_status(&runner, &gw_plan())
            .await
            .expect("status of a down rig exits 0");
        assert!(result.services.is_empty());
        assert!(result.volumes.is_empty());
        assert!(result.ports_free);
    }

    /// The result types serialize every family key — agents never
    /// key-hunt (the locked shape rule).
    #[test]
    fn up_and_down_results_carry_all_keys() {
        let up = RigUpResult {
            rig: "r".into(),
            project: "r".into(),
            state: "uncommissioned".into(),
            gateway_url: None,
            warnings: vec![],
        };
        let json = serde_json::to_value(&up).unwrap();
        for key in ["rig", "project", "state", "gateway_url", "warnings"] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        let down = RigDownResult {
            rig: "r".into(),
            project: "r".into(),
            state: "down".into(),
        };
        let json = serde_json::to_value(&down).unwrap();
        for key in ["rig", "project", "state"] {
            assert!(json.get(key).is_some(), "missing key {key}");
        }
        let status_keys = ["rig", "project", "compose_file", "services", "volumes", "ports_free"];
        let _ = RigStatusResult {
            rig: "r".into(),
            project: "r".into(),
            compose_file: "/c.yml".into(),
            services: vec![],
            volumes: vec![],
            ports_free: true,
        };
        // (shape asserted end-to-end in status_serializes_the_allowlist_exactly;
        // the keys list is pinned here for the doc string)
        assert_eq!(status_keys.len(), 6);
    }
}
