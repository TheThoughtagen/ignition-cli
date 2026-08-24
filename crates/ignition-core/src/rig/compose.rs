//! The compose shell-out engine (04-01, RIG-01): runner seam, version
//! check, LOCKED arg builders, and the LDJSON/array parsers — serde
//! models out, no printing (the TUI rides the actions layer in Phase 6).
//!
//! ## The runner seam
//!
//! [`ComposeRunner`] is the ONLY way any rig code spawns a process:
//! actions and discovery take `&dyn ComposeRunner`, so every decision
//! path is unit-testable against a scripted fake (no docker needed).
//! The production [`DockerCompose`] shells out via
//! `tokio::process::Command` (the workspace `process` feature — the one
//! Phase 4 dependency change). `run` prefixes the `compose` subcommand;
//! `run_docker` spawns the PLAIN docker CLI (volume ls / port
//! attribution — no `-p` prefix, those are not compose project ops);
//! `run_streaming` pipes stdout and forwards lines as they arrive (the
//! `logs -f` follow shape, 04-02).
//!
//! ## LOCKED invocation shapes (research §Compose invocation shapes)
//!
//! Every builder is a pure function whose exact output vector is
//! unit-pinned — `-p <resolved-name>` EXPLICIT on every project op
//! (Pitfall 8: no implicit directory-name projects), `--project-directory`
//! always on resolve (Pitfall 8: `.env` loading is cwd-sensitive),
//! `--remove-orphans` on up AND down (Pitfall 4), `--wait-timeout`
//! explicit on up (Pitfall 3: healthchecks + image pulls).
//!
//! ## The two output conventions (research Pitfall 1)
//!
//! `ps`/`volume ls`/`docker ps` emit ONE OBJECT PER LINE (LDJSON —
//! parsed with a `StreamDeserializer`); `config --format json` emits a
//! SINGLE doc (an object on current compose; an older array shape is
//! tolerated by unwrapping the first element). BOTH conventions are
//! fixture-pinned so the divergence can never regress into a naive
//! `from_str::<Vec<T>>`.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::error::CoreError;

use super::RigPlan;

/// How many stderr lines ride a failed invocation's error message
/// (research Pitfall 3: compose's tail is the diagnosis).
const STDERR_TAIL_LINES: usize = 5;

/// One completed `docker …` invocation: captured output + exit code.
/// A spawn FAILURE (no docker binary) is `code: 127` with the spawn
/// error in `stderr` — one error path, no io::Error leakage.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposeOutput {
    /// Captured stdout (UTF-8 lossy).
    pub stdout: String,
    /// Captured stderr (UTF-8 lossy).
    pub stderr: String,
    /// Process exit code (`-1` when terminated by signal).
    pub code: i32,
}

/// The process seam for everything rig-related. Actions NEVER spawn
/// processes directly — they script this trait (the GatewayApi
/// precedent), which is what makes the whole family testable without
/// docker.
#[async_trait::async_trait]
pub trait ComposeRunner: Send + Sync {
    /// Run `docker compose <args…>` and capture stdout/stderr/exit.
    async fn run(&self, args: &[String]) -> ComposeOutput;
    /// Run the PLAIN docker CLI (`docker <args…>`) — the volume-ls and
    /// port-attribution shapes that are NOT compose project ops.
    async fn run_docker(&self, args: &[String]) -> ComposeOutput;
    /// Stream `docker compose <args…>`: stdout forwards to `line_sink`
    /// LINE-BY-LINE as it arrives (piped stdout — `logs -f` follow
    /// mode), stderr is captured for diagnostics (a failed invocation
    /// carries its tail in the error; a successful one's diagnostics
    /// go to OUR stderr via tracing — NEVER the data stream). The
    /// returned [`ComposeOutput`] carries EMPTY stdout: the lines
    /// already went to the sink (fakes replay a preloaded stdout
    /// through the sink — the keep-it-simple contract).
    async fn run_streaming(
        &self,
        args: &[String],
        line_sink: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> ComposeOutput;
}

/// Production [`ComposeRunner`]: shells out to the real `docker` binary
/// via `tokio::process::Command`.
pub struct DockerCompose;

impl DockerCompose {
    /// Spawn `program` with `args`, capture everything. Spawn failure
    /// (binary absent) maps to the shell's 127 with the reason in
    /// stderr — callers translate nonzero exits uniformly.
    async fn spawn(program: &str, args: &[String]) -> ComposeOutput {
        let joined = std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        match tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
        {
            Ok(output) => ComposeOutput {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                code: output.status.code().unwrap_or(-1),
            },
            Err(err) => ComposeOutput {
                stdout: String::new(),
                stderr: format!("failed to spawn `{joined}`: {err}"),
                code: 127,
            },
        }
    }

    /// The STREAMING spawn (04-02): piped stdout read line-by-line
    /// into `line_sink` until EOF, stderr drained CONCURRENTLY (a full
    /// stderr pipe would deadlock the stdout reader), then the wait —
    /// the child streams until it exits (Ctrl-C kills the whole
    /// foreground process group, the `logs -f` precedent).
    async fn spawn_streaming(
        program: &str,
        args: &[String],
        line_sink: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> ComposeOutput {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

        let joined = std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        let mut child = match tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                return ComposeOutput {
                    stdout: String::new(),
                    stderr: format!("failed to spawn `{joined}`: {err}"),
                    code: 127,
                };
            }
        };
        let stdout = child.stdout.take().expect("stdout piped above");
        let mut stderr = child.stderr.take().expect("stderr piped above");
        // Concurrent stderr drain — the join only fails on task panic.
        let stderr_task = tokio::spawn(async move {
            let mut buffer = String::new();
            let _ = stderr.read_to_string(&mut buffer).await;
            buffer
        });
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            line_sink(&line);
        }
        match child.wait().await {
            Ok(status) => ComposeOutput {
                stdout: String::new(),
                stderr: stderr_task.await.unwrap_or_default(),
                code: status.code().unwrap_or(-1),
            },
            Err(err) => ComposeOutput {
                stdout: String::new(),
                stderr: format!("failed to wait for `{joined}`: {err}"),
                code: -1,
            },
        }
    }
}

#[async_trait::async_trait]
impl ComposeRunner for DockerCompose {
    async fn run(&self, args: &[String]) -> ComposeOutput {
        let mut full = vec!["compose".to_string()];
        full.extend(args.iter().cloned());
        Self::spawn("docker", &full).await
    }

    async fn run_docker(&self, args: &[String]) -> ComposeOutput {
        Self::spawn("docker", args).await
    }

    async fn run_streaming(
        &self,
        args: &[String],
        line_sink: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> ComposeOutput {
        let mut full = vec!["compose".to_string()];
        full.extend(args.iter().cloned());
        Self::spawn_streaming("docker", &full, line_sink).await
    }
}

/// Map a completed invocation: exit 0 → the stdout str; nonzero →
/// [`CoreError::Rig`] carrying the stderr tail (last
/// [`STDERR_TAIL_LINES`] lines — research Pitfall 3).
pub fn check_output<'a>(output: &'a ComposeOutput, context: &str) -> Result<&'a str, CoreError> {
    if output.code == 0 {
        Ok(&output.stdout)
    } else {
        let tail = stderr_tail(&output.stderr);
        let note = if tail.is_empty() {
            String::new()
        } else {
            format!(": {tail}")
        };
        Err(CoreError::Rig(format!(
            "{context} failed (exit {}){note}",
            output.code
        )))
    }
}

/// The last `lines` stderr lines, in order, trimmed — the diagnosis
/// tail compose prints on failure.
pub(crate) fn stderr_tail(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .take(STDERR_TAIL_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Verify `docker compose` answers with major version ≥ 2 (the v1
/// `docker-compose` Python binary never answers to `docker compose` —
/// absence IS the install-hint case). Returns the version string
/// (e.g. `"5.1.2"`).
pub async fn compose_version(runner: &dyn ComposeRunner) -> Result<String, CoreError> {
    let output = runner.run(&["version".to_string()]).await;
    if output.code != 0 {
        return Err(CoreError::Rig(format!(
            "docker compose is unavailable (exit {}): {} — install Docker Desktop or \
             the compose v2 plugin; the legacy docker-compose v1 binary is not supported",
            output.code,
            stderr_tail(&output.stderr)
        )));
    }
    parse_compose_version(&output.stdout).ok_or_else(|| {
        CoreError::Rig(format!(
            "cannot parse `docker compose version` output ({:?}) — compose ≥ v2 \
             (the `docker compose` plugin) is required",
            output.stdout.trim()
        ))
    })
}

/// `"Docker Compose version v5.1.2"` → `Some("5.1.2")`; anything else —
/// including a major < 2 — is `None`. Pure; pinned by tests.
fn parse_compose_version(stdout: &str) -> Option<String> {
    let rest = stdout.trim().strip_prefix("Docker Compose version ")?;
    let version = rest.split_whitespace().next()?;
    let version = version.strip_prefix('v').unwrap_or(version);
    let mut parts = version.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    if major < 2 {
        return None;
    }
    Some(version.to_string())
}

// ---------------------------------------------------------------------------
// Arg builders — pure, exact-vector-pinned below.
// ---------------------------------------------------------------------------

/// The resolve step (research Pattern 1): `docker compose -f <file>
/// --project-directory <dir> config --format json`. `--project-directory`
/// is ALWAYS explicit — `.env` (and thus `COMPOSE_PROJECT_NAME`)
/// loading is cwd-sensitive (Pitfall 8).
pub fn config_args(file: &Path, project_dir: &Path) -> Vec<String> {
    vec![
        "-f".into(),
        file.display().to_string(),
        "--project-directory".into(),
        project_dir.display().to_string(),
        "config".into(),
        "--format".into(),
        "json".into(),
    ]
}

/// `up` (research LOCKED shape): explicit `-p <name>` (never an
/// implicit directory-name project), detached + `--wait` with an
/// EXPLICIT timeout (Pitfall 3: `--wait` blocks on healthchecks and
/// image pulls), `--remove-orphans`.
pub fn up_args(plan: &RigPlan, wait_timeout_s: u64) -> Vec<String> {
    vec![
        "-p".into(),
        plan.name.clone(),
        "-f".into(),
        plan.compose_file.display().to_string(),
        "up".into(),
        "-d".into(),
        "--wait".into(),
        "--wait-timeout".into(),
        wait_timeout_s.to_string(),
        "--remove-orphans".into(),
    ]
}

/// `down`: stop + remove containers/networks; `--remove-orphans` always
/// (Pitfall 4); `-v` (named+anonymous volume deletion) only for the
/// reset teardown half (04-02).
pub fn down_args(plan: &RigPlan, volumes: bool) -> Vec<String> {
    let mut args = vec![
        "-p".into(),
        plan.name.clone(),
        "-f".into(),
        plan.compose_file.display().to_string(),
        "down".into(),
        "--remove-orphans".into(),
    ];
    if volumes {
        args.push("-v".into());
    }
    args
}

/// `ps` as LDJSON (research Pitfall 1): one object per service.
pub fn ps_args(plan: &RigPlan) -> Vec<String> {
    vec![
        "-p".into(),
        plan.name.clone(),
        "-f".into(),
        plan.compose_file.display().to_string(),
        "ps".into(),
        "--format".into(),
        "json".into(),
    ]
}

/// `logs` (human-form passthrough by design — the streaming exception
/// when `--follow`; wired by 04-02's `rig_logs` via `run` one-shot /
/// `run_streaming` follow). Invocation shape LOCKED from day one.
pub fn logs_args(plan: &RigPlan, tail: u32, follow: bool, service: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "-p".into(),
        plan.name.clone(),
        "-f".into(),
        plan.compose_file.display().to_string(),
        "logs".into(),
        "--tail".into(),
        tail.to_string(),
    ];
    if follow {
        args.push("-f".into());
    }
    if let Some(service) = service {
        args.push(service.to_string());
    }
    args
}

/// `docker volume ls` (04-01 status / 04-02 reset preview): PLAIN
/// docker CLI — no `compose` subcommand, no `-p` prefix (volumes are
/// labeled, not project-scoped, at this layer). Invoked via
/// [`ComposeRunner::run_docker`], which spawns `docker`, NOT
/// `docker compose`, for exactly this shape.
pub fn volume_ls_args(project: &str) -> Vec<String> {
    vec![
        "volume".into(),
        "ls".into(),
        "--filter".into(),
        format!("label=com.docker.compose.project={project}"),
        "--format".into(),
        "json".into(),
    ]
}

/// `docker ps --filter publish=<port> --format json` — host-port
/// occupancy with attribution (research Pattern 3); also a PLAIN-docker
/// shape via [`ComposeRunner::run_docker`].
pub fn docker_ps_publish_args(port: u16) -> Vec<String> {
    vec![
        "ps".into(),
        "--filter".into(),
        format!("publish={port}"),
        "--format".into(),
        "json".into(),
    ]
}

// ---------------------------------------------------------------------------
// Parsers — pure functions over recorded-fixture strings.
// ---------------------------------------------------------------------------

/// One published-port mapping from a resolved compose config
/// (container `target` → host `published`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PortMapping {
    /// Container port.
    pub target: u16,
    /// Host port.
    pub published: u16,
}

/// Parse `docker compose config --format json` output into a
/// [`RigPlan`] skeleton: `.name` is THE identity truth (honors the
/// rig's own `.env` `COMPOSE_PROJECT_NAME`), services are the service
/// map's keys, `port_mappings` the collected target→published pairs,
/// volumes the volume map's keys. Tolerant where compose is shape-shifty
/// (`published` arrives as string OR number; the doc may be a bare
/// object — current compose — or a single-element array — older
/// builds); loud where identity is at stake (no `.name` → error, never
/// an implicit directory-name project).
pub fn parse_config(
    stdout: &str,
    compose_file: &Path,
    project_dir: &Path,
) -> Result<RigPlan, CoreError> {
    let trimmed = stdout.trim();
    let doc: Value = serde_json::from_str(trimmed).map_err(|err| {
        CoreError::Rig(format!(
            "cannot parse `docker compose config` output: {err}"
        ))
    })?;
    let root = match doc {
        Value::Array(mut items) => items.pop().unwrap_or(Value::Null),
        Value::Object(object) => Value::Object(object),
        other => other,
    };
    let name = root
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            CoreError::Rig(
                "resolved compose config carries no `.name` — the project name is the \
                 rig's identity truth; refusing to guess (set COMPOSE_PROJECT_NAME in \
                 the rig's .env)"
                    .to_string(),
            )
        })?
        .to_string();

    let services: Vec<String> = root
        .get("services")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();

    let mut port_mappings: Vec<PortMapping> = Vec::new();
    if let Some(services) = root.get("services").and_then(Value::as_object) {
        for service in services.values() {
            let ports = service.get("ports").and_then(Value::as_array);
            for port in ports.into_iter().flatten() {
                // `published` is a STRING on current compose ("9088"),
                // a number on some builds — tolerate both; entries
                // without a published port (random host ports) don't
                // map and are skipped.
                let published = port.get("published").and_then(|value| {
                    value
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .or_else(|| value.as_u64().and_then(|n| u16::try_from(n).ok()))
                });
                let target = port
                    .get("target")
                    .and_then(|value| value.as_u64().and_then(|n| u16::try_from(n).ok()));
                if let (Some(published), Some(target)) = (published, target)
                    && !port_mappings
                        .iter()
                        .any(|mapping| mapping.published == published)
                {
                    port_mappings.push(PortMapping { target, published });
                }
            }
        }
    }
    let host_ports = port_mappings
        .iter()
        .map(|mapping| mapping.published)
        .collect();

    let volumes: Vec<String> = root
        .get("volumes")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();

    Ok(RigPlan {
        name,
        compose_file: compose_file.to_path_buf(),
        project_dir: project_dir.to_path_buf(),
        services,
        host_ports,
        port_mappings,
        volumes,
    })
}

// ---------------------------------------------------------------------------
// Runner-scripted ops (04-02)
// ---------------------------------------------------------------------------

/// The reset preview (04-02, RIG-01): the named-volume names `rig
/// reset`'s `down -v` half will remove, reported in the result data so
/// agents see WHAT reset took before/as it acts. Label-filtered at the
/// docker layer ([`volume_ls_args`]) and name-filtered here — only
/// `<project>_-prefixed` volumes are reset's to take (defense in
/// depth; research Pitfall 4 shape: `Name` + `Labels`).
pub async fn reset_preview(
    runner: &dyn ComposeRunner,
    plan: &RigPlan,
) -> Result<Vec<String>, CoreError> {
    let output = runner.run_docker(&volume_ls_args(&plan.name)).await;
    let stdout = check_output(&output, "docker volume ls")?;
    let prefix = format!("{}_", plan.name);
    Ok(parse_volume_ls_ldjson(stdout)
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.starts_with(&prefix))
        .collect())
}

/// One `docker compose ps` publisher row (live-captured field names).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Publisher {
    /// Bind host (`"0.0.0.0"`).
    #[serde(default, rename = "URL")]
    pub url: Option<String>,
    /// Container port.
    #[serde(default, rename = "TargetPort")]
    pub target_port: Option<u16>,
    /// Host port.
    #[serde(default, rename = "PublishedPort")]
    pub published_port: Option<u16>,
    /// `tcp`/`udp`.
    #[serde(default, rename = "Protocol")]
    pub protocol: Option<String>,
}

/// One `docker compose ps` service row (live-captured field names;
/// `Health`/`Publishers` optional — services without healthchecks or
/// ports omit them).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServiceStatus {
    /// Container name (`whk-global-ignition-1`).
    #[serde(default, rename = "Name")]
    pub name: String,
    /// Service name (`ignition`).
    #[serde(default, rename = "Service")]
    pub service: String,
    /// `running` / `exited` / …
    #[serde(default, rename = "State")]
    pub state: String,
    /// `healthy` / `starting` / absent.
    #[serde(default, rename = "Health")]
    pub health: Option<String>,
    /// Last exit code.
    #[serde(default, rename = "ExitCode")]
    pub exit_code: Option<i64>,
    /// Published ports with attribution.
    #[serde(default, rename = "Publishers")]
    pub publishers: Vec<Publisher>,
}

/// Parse LDJSON (one JSON object per line — research Pitfall 1). The
/// per-line split IS the delimiter contract (compose never wraps a row
/// across lines), so a stray non-JSON warning line WARNs and skips
/// without losing the rows after it; empty output is an empty vec (a
/// down rig has no rows).
fn parse_ldjson<T: serde::de::DeserializeOwned>(stdout: &str, what: &str) -> Vec<T> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str::<T>(line) {
            Ok(item) => Some(item),
            Err(err) => {
                tracing::warn!(source = what, error = %err, "skipping unparseable line");
                None
            }
        })
        .collect()
}

/// Parse `docker compose ps --format json` LDJSON into
/// [`ServiceStatus`] rows.
pub fn parse_ps_ldjson(stdout: &str) -> Vec<ServiceStatus> {
    parse_ldjson(stdout, "docker compose ps")
}

/// One `docker volume ls` row (only the name is consumed by status).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VolumeEntry {
    /// Volume name (`<project>_<volume>`).
    #[serde(default, rename = "Name")]
    pub name: String,
    /// Labels verbatim (includes `com.docker.compose.project`).
    #[serde(default, rename = "Labels")]
    pub labels: Value,
}

/// Parse `docker volume ls --format json` LDJSON (research Pitfall 1).
pub fn parse_volume_ls_ldjson(stdout: &str) -> Vec<VolumeEntry> {
    parse_ldjson(stdout, "docker volume ls")
}

/// One `docker ps --filter publish=` row, reduced to attribution: the
/// container's (first) name and its compose project label, when it has
/// one. `docker ps` JSON differs from `compose ps` JSON (`Names`
/// plural, `Labels` sometimes a `"k=v,k=v"` string instead of a map) —
/// both shapes are tolerated and fixture-pinned.
#[derive(Debug, Clone, PartialEq)]
pub struct DockerPsEntry {
    /// Container name.
    pub name: String,
    /// `com.docker.compose.project` label, when present.
    pub compose_project: Option<String>,
}

/// Parse `docker ps --format json` LDJSON into [`DockerPsEntry`] rows
/// (name + compose-project attribution only).
pub fn parse_docker_ps_ldjson(stdout: &str) -> Vec<DockerPsEntry> {
    parse_ldjson::<serde_json::Value>(stdout, "docker ps")
        .into_iter()
        .filter_map(|value| {
            // `Names` (docker ps, plural, comma-joined when multiple)
            // with `Name` (compose ps shape) accepted as a fallback.
            let name = value
                .get("Names")
                .or_else(|| value.get("Name"))
                .and_then(Value::as_str)
                .map(|names| names.split(',').next().unwrap_or(names).trim().to_string())
                .unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            let compose_project = match value.get("Labels") {
                Some(Value::String(labels)) => labels.split(',').find_map(|pair| {
                    pair.trim()
                        .strip_prefix("com.docker.compose.project=")
                        .map(str::to_string)
                }),
                Some(labels @ Value::Object(_)) => labels
                    .get("com.docker.compose.project")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                _ => None,
            };
            Some(DockerPsEntry {
                name,
                compose_project,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        ComposeOutput, ComposeRunner, DockerCompose, PortMapping, check_output, config_args,
        docker_ps_publish_args, down_args, logs_args, parse_compose_version, parse_config,
        parse_docker_ps_ldjson, parse_ps_ldjson, parse_volume_ls_ldjson, ps_args, stderr_tail,
        up_args, volume_ls_args,
    };
    use crate::rig::RigPlan;

    fn sample_plan() -> RigPlan {
        RigPlan {
            name: "ignition-devops".into(),
            compose_file: "/rigs/git-module/docker/docker-compose.yml".into(),
            project_dir: "/rigs/git-module/docker".into(),
            services: vec!["ignition".into(), "db".into()],
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

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    // ----- builder pins (the LOCKED invocation shapes) -------------------

    #[test]
    fn config_args_pinned() {
        assert_eq!(
            config_args(
                Path::new("/rigs/docker/compose.yml"),
                Path::new("/rigs/docker"),
            ),
            s(&[
                "-f",
                "/rigs/docker/compose.yml",
                "--project-directory",
                "/rigs/docker",
                "config",
                "--format",
                "json"
            ]),
        );
    }

    #[test]
    fn up_args_pinned() {
        assert_eq!(
            up_args(&sample_plan(), 300),
            s(&[
                "-p",
                "ignition-devops",
                "-f",
                "/rigs/git-module/docker/docker-compose.yml",
                "up",
                "-d",
                "--wait",
                "--wait-timeout",
                "300",
                "--remove-orphans"
            ]),
        );
    }

    #[test]
    fn down_args_pinned_with_and_without_volumes() {
        assert_eq!(
            down_args(&sample_plan(), false),
            s(&[
                "-p",
                "ignition-devops",
                "-f",
                "/rigs/git-module/docker/docker-compose.yml",
                "down",
                "--remove-orphans"
            ]),
            "plain down keeps volumes (reset's -v arrives in 04-02)"
        );
        assert_eq!(
            down_args(&sample_plan(), true),
            s(&[
                "-p",
                "ignition-devops",
                "-f",
                "/rigs/git-module/docker/docker-compose.yml",
                "down",
                "--remove-orphans",
                "-v"
            ]),
        );
    }

    #[test]
    fn ps_args_pinned() {
        assert_eq!(
            ps_args(&sample_plan()),
            s(&[
                "-p",
                "ignition-devops",
                "-f",
                "/rigs/git-module/docker/docker-compose.yml",
                "ps",
                "--format",
                "json"
            ]),
        );
    }

    #[test]
    fn logs_args_pinned() {
        assert_eq!(
            logs_args(&sample_plan(), 200, false, None),
            s(&[
                "-p",
                "ignition-devops",
                "-f",
                "/rigs/git-module/docker/docker-compose.yml",
                "logs",
                "--tail",
                "200"
            ]),
        );
        assert_eq!(
            logs_args(&sample_plan(), 50, true, Some("ignition")),
            s(&[
                "-p",
                "ignition-devops",
                "-f",
                "/rigs/git-module/docker/docker-compose.yml",
                "logs",
                "--tail",
                "50",
                "-f",
                "ignition"
            ]),
        );
    }

    #[test]
    fn volume_ls_args_pinned_plain_docker_shape() {
        assert_eq!(
            volume_ls_args("ignition-devops"),
            s(&[
                "volume",
                "ls",
                "--filter",
                "label=com.docker.compose.project=ignition-devops",
                "--format",
                "json"
            ]),
            "plain docker CLI: no compose subcommand, no -p prefix"
        );
    }

    #[test]
    fn docker_ps_publish_args_pinned() {
        assert_eq!(
            docker_ps_publish_args(18088),
            s(&["ps", "--filter", "publish=18088", "--format", "json"]),
        );
    }

    // ----- version parse -------------------------------------------------

    #[test]
    fn version_parses_live_capture_and_rejects_old_or_garbage() {
        // Live v5.1.2 capture (research).
        assert_eq!(
            parse_compose_version("Docker Compose version v5.1.2\n"),
            Some("5.1.2".into())
        );
        assert_eq!(
            parse_compose_version("Docker Compose version v2.24.6\n"),
            Some("2.24.6".into())
        );
        // Unversioned builds exist ("Docker Compose version 2.34.0").
        assert_eq!(
            parse_compose_version("Docker Compose version 2.34.0\n"),
            Some("2.34.0".into())
        );
        // v1-shaped output and garbage are unusable.
        assert_eq!(parse_compose_version("1.29.2"), None);
        assert_eq!(parse_compose_version(""), None);
        assert_eq!(parse_compose_version("docker-compose 1.25.5"), None);
    }

    // ----- exit mapping ---------------------------------------------------

    #[test]
    fn check_output_maps_nonzero_to_rig_with_stderr_tail() {
        let ok = ComposeOutput {
            stdout: "{}".into(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(check_output(&ok, "ctx").unwrap(), "{}");

        let tail = (1..=9).map(|n| format!("line-{n}")).collect::<Vec<_>>();
        let failed = ComposeOutput {
            stdout: String::new(),
            stderr: tail.join("\n"),
            code: 1,
        };
        let err = check_output(&failed, "docker compose up").unwrap_err();
        assert!(matches!(err, crate::error::CoreError::Rig(_)));
        let message = err.to_string();
        assert!(
            message.contains("docker compose up failed (exit 1)"),
            "{message}"
        );
        // The tail is the LAST ~5 lines only.
        assert!(
            !message.contains("line-4\n"),
            "tail keeps at most 5 lines: {message}"
        );
        assert!(message.contains("line-5"));
        assert!(message.contains("line-9"));
    }

    #[test]
    fn stderr_tail_trims_and_keeps_order() {
        assert_eq!(stderr_tail("b\na\n"), "b\na");
        assert_eq!(stderr_tail(""), "");
        assert_eq!(stderr_tail("\n\n"), "");
    }

    // ----- parse_config ---------------------------------------------------

    /// The whk-environment-orchestration-shaped resolve fixture:
    /// `.name` from the rig's own `.env`, string-form `published`, a
    /// volumes map, and ignored top-level keys.
    const CONFIG_FIXTURE: &str = r#"{
        "name": "whk-global",
        "services": {
            "ignition": {
                "image": "inductiveautomation/ignition:8.3.6",
                "ports": [
                    {"mode": "host", "target": 8088, "published": "9088", "protocol": "tcp"},
                    {"mode": "host", "target": 443, "published": "9443", "protocol": "tcp"}
                ]
            },
            "git-server": {
                "image": "git-server:latest",
                "ports": [{"mode": "host", "target": 22, "published": "9022", "protocol": "tcp"}]
            }
        },
        "volumes": {
            "gw-data": {"name": "whk-global_gw-data", "driver": "local"},
            "gw-tag-definition": {"name": "whk-global_gw-tag-definition"}
        },
        "networks": {"default": {"name": "whk-global_default"}},
        "secrets": {}
    }"#;

    #[test]
    fn parse_config_reads_name_services_ports_volumes() {
        let plan = parse_config(
            CONFIG_FIXTURE,
            Path::new("/whk/whk-environment-orchestration/docker-compose.yml"),
            Path::new("/whk/whk-environment-orchestration"),
        )
        .expect("fixture parses");
        assert_eq!(plan.name, "whk-global");
        assert_eq!(plan.services, vec!["git-server", "ignition"]);
        // Services iterate sorted (serde_json's map) → ports arrive in
        // service order: git-server's 9022 first.
        assert_eq!(plan.host_ports, vec![9022, 9088, 9443]);
        assert!(
            plan.port_mappings.contains(&PortMapping {
                target: 8088,
                published: 9088
            }),
            "string-form `published` tolerated: {:?}",
            plan.port_mappings
        );
        assert_eq!(plan.volumes, vec!["gw-data", "gw-tag-definition"]);
        assert_eq!(
            plan.compose_file,
            Path::new("/whk/whk-environment-orchestration/docker-compose.yml")
        );
        assert_eq!(
            plan.project_dir,
            Path::new("/whk/whk-environment-orchestration")
        );
    }

    #[test]
    fn parse_config_tolerates_array_doc_and_numeric_published() {
        // Older compose wraps the config doc in a single-element array,
        // and some builds emit numeric `published`.
        let array_form = format!("[{}]", CONFIG_FIXTURE.replace("\"9088\"", "9088"));
        let plan = parse_config(&array_form, Path::new("/c.yml"), Path::new("/"))
            .expect("array doc + numeric published parse");
        assert_eq!(plan.name, "whk-global");
        assert!(plan.host_ports.contains(&9088));
    }

    #[test]
    fn parse_config_without_name_refuses() {
        let fixture = r#"{"services": {"ignition": {}}}"#;
        let err = parse_config(fixture, Path::new("/c.yml"), Path::new("/")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("no `.name`"), "{message}");
        assert!(message.contains("COMPOSE_PROJECT_NAME"), "{message}");
    }

    #[test]
    fn parse_config_empty_services_and_missing_sections() {
        let plan = parse_config(r#"{"name": "bare"}"#, Path::new("/c.yml"), Path::new("/"))
            .expect("minimal doc parses");
        assert!(plan.services.is_empty());
        assert!(plan.host_ports.is_empty());
        assert!(plan.volumes.is_empty());
    }

    /// Research Open Question 4, resolved empirically: the rig's own
    /// `.env` `COMPOSE_PROJECT_NAME` governs the resolved `.name` even
    /// when `ign` runs from an unrelated cwd — because `config_args`
    /// ALWAYS passes `--project-directory` (Pitfall 8). Runs the REAL
    /// docker CLI (config is client-side; no daemon needed) and skips
    /// quietly when docker is absent (CI).
    #[tokio::test]
    async fn config_resolves_env_project_name_from_project_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let compose = dir.path().join("docker-compose.yml");
        std::fs::write(
            &compose,
            "services:\n  sidecar:\n    image: alpine:latest\n",
        )
        .expect("write compose");
        std::fs::write(
            dir.path().join(".env"),
            "COMPOSE_PROJECT_NAME=env-resolved-name\n",
        )
        .expect("write .env");

        let runner = DockerCompose;
        if runner.run(&["version".to_string()]).await.code != 0 {
            eprintln!("skipping: docker compose unavailable");
            return;
        }
        // The spawn's cwd is the TEST binary's dir — nowhere near the
        // fixture — which is exactly the cwd-elsewhere case.
        let output = runner.run(&config_args(&compose, dir.path())).await;
        assert_eq!(output.code, 0, "config run: {}", output.stderr);
        let plan = parse_config(&output.stdout, &compose, dir.path()).expect("resolve parses");
        assert_eq!(
            plan.name, "env-resolved-name",
            "the .env name wins over the directory-derived default"
        );
    }

    /// The STREAMING spawn (04-02): piped stdout forwarded line-by-line
    /// to the sink, stderr drained, exit reported. Runs the REAL docker
    /// CLI (`logs` on an absent project is exit 0 + empty output on
    /// compose v2 — no daemon-side state needed) and skips quietly when
    /// docker is absent (CI). Client-side proof of the follow seam.
    #[tokio::test]
    async fn run_streaming_forwards_lines_via_piped_stdout() {
        let dir = tempfile::tempdir().expect("tempdir");
        let compose = dir.path().join("docker-compose.yml");
        std::fs::write(
            &compose,
            "services:\n  sidecar:\n    image: alpine:latest\n",
        )
        .expect("write compose");

        let runner = DockerCompose;
        if runner.run(&["version".to_string()]).await.code != 0 {
            eprintln!("skipping: docker compose unavailable");
            return;
        }
        let plan = parse_config(
            r#"{"name":"stream-fixture","services":{"sidecar":{"image":"alpine"}}}"#,
            &compose,
            dir.path(),
        )
        .expect("plan parses");
        let mut streamed = 0usize;
        let mut sink = |_: &str| streamed += 1;
        let output = runner
            .run_streaming(&logs_args(&plan, 5, false, None), &mut sink)
            .await;
        assert_eq!(
            output.code, 0,
            "compose logs on an absent project: {}",
            output.stderr
        );
        assert_eq!(
            streamed, 0,
            "no containers → no lines, but the spawn/read/wait ran"
        );
        assert!(
            output.stdout.is_empty(),
            "streamed stdout never reports back"
        );
    }

    // ----- LDJSON parsers (research Pitfall 1: BOTH conventions pinned) --

    /// Live-captured compose ps row shape (whk-global-style): one
    /// object per line, Health/Publishers present on this row.
    const PS_ROW_FULL: &str = r#"{"Command":"\"/usr/bin/start-ignition.sh\"","CreatedAt":"2026-08-22 10:00:00 +0000 UTC","ExitCode":0,"Health":"healthy","Labels":{"com.docker.compose.project":"whk-global","com.docker.compose.service":"ignition"},"Name":"whk-global-ignition-1","Networks":"whk-global_default","Ports":"0.0.0.0:9088->8088/tcp, 0.0.0.0:9443->443/tcp","Publishers":[{"URL":"0.0.0.0","TargetPort":8088,"PublishedPort":9088,"Protocol":"tcp"},{"URL":"0.0.0.0","TargetPort":443,"PublishedPort":9443,"Protocol":"tcp"}],"RunningFor":"2 hours","Service":"ignition","State":"running","Status":"Up 2 hours (healthy)"}"#;

    /// A second row WITHOUT Health/Publishers (service publishes no
    /// ports, no healthcheck) — the optional-fields tolerance pin.
    const PS_ROW_MINIMAL: &str = r#"{"Command":"sleep","ExitCode":0,"Name":"whk-global-sidecar-1","Service":"sidecar","State":"exited","Status":"Exited (0) 3 minutes ago"}"#;

    #[test]
    fn parse_ps_ldjson_reads_rows_and_tolerates_missing_optionals() {
        let rows = parse_ps_ldjson(&format!("{PS_ROW_FULL}\n{PS_ROW_MINIMAL}\n"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "whk-global-ignition-1");
        assert_eq!(rows[0].service, "ignition");
        assert_eq!(rows[0].state, "running");
        assert_eq!(rows[0].health.as_deref(), Some("healthy"));
        assert_eq!(rows[0].exit_code, Some(0));
        assert_eq!(rows[0].publishers.len(), 2);
        assert_eq!(rows[0].publishers[0].target_port, Some(8088));
        assert_eq!(rows[0].publishers[0].published_port, Some(9088));
        assert_eq!(rows[0].publishers[0].protocol.as_deref(), Some("tcp"));

        assert_eq!(rows[1].health, None);
        assert!(rows[1].publishers.is_empty());
    }

    #[test]
    fn parse_ps_ldjson_empty_and_warning_lines() {
        assert!(parse_ps_ldjson("").is_empty(), "a down rig has no rows");
        // A stray non-JSON warning line skips (warn), the rows survive.
        let rows = parse_ps_ldjson(&format!("[NOTE] something\n{PS_ROW_MINIMAL}\n"));
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn parse_volume_ls_ldjson_fixture() {
        let stdout = concat!(
            r#"{"CreatedAt":"2026-08-22T10:00:00Z","Driver":"local","Labels":{"com.docker.compose.project":"ignition-devops","com.docker.compose.volume":"gw-data"},"Mountpoint":"/var/lib/docker/volumes/ignition-devops_gw-data/_data","Name":"ignition-devops_gw-data","Options":null,"Scope":"local"}"#,
            "\n",
            r#"{"CreatedAt":"2026-08-22T10:00:00Z","Driver":"local","Labels":{"com.docker.compose.project":"ignition-devops","com.docker.compose.volume":"gw-tag-definition"},"Mountpoint":"/var/lib/docker/volumes/ignition-devops_gw-tag-definition/_data","Name":"ignition-devops_gw-tag-definition","Options":null,"Scope":"local"}"#,
            "\n",
        );
        let volumes = parse_volume_ls_ldjson(stdout);
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0].name, "ignition-devops_gw-data");
        assert_eq!(volumes[1].name, "ignition-devops_gw-tag-definition");
        assert!(parse_volume_ls_ldjson("").is_empty());
    }

    #[test]
    fn parse_docker_ps_labels_as_string_and_map() {
        // docker ps emits `Names` (plural) and Labels as a k=v string.
        let string_labels = r#"{"ID":"abc123","Image":"alpine","Labels":"com.docker.compose.project=other-project,com.docker.compose.service=sidecar","Names":"other-project-sidecar-1","Ports":"0.0.0.0:18088->8088/tcp","State":"running"}"#;
        let rows = parse_docker_ps_ldjson(string_labels);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "other-project-sidecar-1");
        assert_eq!(rows[0].compose_project.as_deref(), Some("other-project"));

        // The compose-ps-style map labels + Name key are accepted too.
        let map_labels =
            r#"{"Name":"ign-research","Labels":{"com.docker.compose.project":"research"}}"#;
        let rows = parse_docker_ps_ldjson(map_labels);
        assert_eq!(rows[0].name, "ign-research");
        assert_eq!(rows[0].compose_project.as_deref(), Some("research"));

        // Non-compose containers attribute with no project.
        let rows = parse_docker_ps_ldjson(r#"{"Names":"standalone-thing","Labels":""}"#);
        assert_eq!(rows[0].compose_project, None);
        assert!(parse_docker_ps_ldjson("").is_empty());
    }
}
