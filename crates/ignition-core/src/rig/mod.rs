//! Rig discovery + pre-flight (04-01, RIG-01): the [`RigPlan`] model,
//! the LOCKED 5-level discovery that always ends in one resolve-then-act
//! `config` run, and the port-collision pre-flight.
//!
//! ## Resolve-then-act (research Pattern 1)
//!
//! Discovery only ever finds a compose FILE; [`resolve_plan`] then runs
//! `docker compose -f <file> --project-directory <dir> config --format
//! json` through the runner and the returned `.name` (which honors the
//! rig's own `.env` `COMPOSE_PROJECT_NAME`) becomes the identity truth
//! every later op passes as explicit `-p <name>` (Pitfall 8: no
//! implicit directory-name projects, ever).
//!
//! ## Discovery order (LOCKED — must-have truth)
//!
//! 1. `--rig NAME` flag → `[rigs.NAME]`
//! 2. `IGNITION_RIG` env → same (the bin folds it into the flag — one
//!    env→flag home, the IGNITION_PROFILE precedent)
//! 3. `[rig].default` → `[rigs.<default>]` (a stale default is a LOUD
//!    error, never a silent scan)
//! 4. cwd candidates: `./docker/compose.yml`, `./docker/docker-compose.yml`,
//!    `./compose.yml`, `./compose.yaml`, `./docker-compose.yml`
//! 5. WHK conventions — the git-module repo, then the WHK-Global
//!    orchestration repo, each probed under BOTH home roots
//!    (plan-checker: machine layouts differ; never pin one root):
//!    `~/Documents/whiskeyhouse/` first, `~/whiskeyhouse/` second.
//!
//! Nothing found → [`CoreError::Rig`] carrying the full search trail
//! (agents self-diagnose). Convention roots live in ONE const array;
//! [`whk_roots`] additionally honors `IGNITION_RIG_ROOTS` (path-separated)
//! so binary tests can isolate the machine's real home and agents with
//! rigs elsewhere can redirect the convention scan.

use std::path::{Path, PathBuf};

use serde::Serialize;

pub mod compose;

pub use compose::{
    ComposeOutput, ComposeRunner, DockerCompose, DockerPsEntry, PortMapping, Publisher,
    ServiceStatus, VolumeEntry, compose_version, config_args, docker_ps_publish_args, parse_config,
    parse_docker_ps_ldjson, reset_preview,
};

use crate::config::Config;
use crate::error::CoreError;

/// The resolved rig — research Pattern 1's model, plus the target→
/// published port pairs (the gateway-URL heuristic in `actions::rig`
/// needs targets, not just the published half).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RigPlan {
    /// Compose project name — THE identity truth (honors the rig's own
    /// `.env` `COMPOSE_PROJECT_NAME` via the resolve run).
    pub name: String,
    /// The compose file discovery found.
    pub compose_file: PathBuf,
    /// Its directory (`--project-directory` on every resolve; where
    /// `.env` is read from).
    pub project_dir: PathBuf,
    /// Service names, sorted (the config map's keys).
    pub services: Vec<String>,
    /// Published host ports (the published half of `port_mappings`).
    pub host_ports: Vec<u16>,
    /// Full target→published pairs.
    pub port_mappings: Vec<PortMapping>,
    /// Named volumes declared by the compose file.
    pub volumes: Vec<String>,
}

/// What the caller wants resolved: an explicit name (the `--rig` flag,
/// which the bin already folded `IGNITION_RIG` into) or the auto chain
/// (`[rig].default` → cwd scan → convention scan).
#[derive(Debug, Clone, PartialEq)]
pub enum RigSelection {
    /// `--rig NAME` / `IGNITION_RIG` — MUST exist in `[rigs.*]` or the
    /// error lists the knowns (the ProfileNotFound shape precedent).
    Named(String),
    /// No preference — `[rig].default`, then the cwd/convention scan.
    Auto,
}

/// WHK convention home roots, tried in this order (plan-checker
/// 2026-08-22: this machine's whk-environment-orchestration lives under
/// `~/whiskeyhouse/`, not `~/Documents/whiskeyhouse/` — layouts differ
/// per machine, so BOTH roots are probed for BOTH convention repos).
pub const WHK_HOME_ROOTS: &[&str] = &["~/Documents/whiskeyhouse", "~/whiskeyhouse"];

/// Relative compose-file locations of the two WHK convention repos.
const GIT_MODULE_RELPATH: &str = "ignition-git-module/docker/docker-compose.yml";
const WHK_GLOBAL_RELPATH: &str = "whk-environment-orchestration/docker-compose.yml";

/// cwd compose candidates, in order (discovery level 4).
const CWD_CANDIDATES: &[&str] = &[
    "docker/compose.yml",
    "docker/docker-compose.yml",
    "compose.yml",
    "compose.yaml",
    "docker-compose.yml",
];

/// The expanded convention roots for this invocation:
/// `IGNITION_RIG_ROOTS` (path-separated, `~`-expanded) overrides the
/// const pair — binary-test isolation plus a real agent affordance
/// (rigs checked out elsewhere).
fn whk_roots() -> Vec<PathBuf> {
    if let Ok(override_roots) = std::env::var("IGNITION_RIG_ROOTS")
        && !override_roots.trim().is_empty()
    {
        return override_roots
            .split(':')
            .filter(|part| !part.is_empty())
            .map(expand_path)
            .collect();
    }
    WHK_HOME_ROOTS.iter().map(|root| expand_path(root)).collect()
}

/// Expand a configured path: leading `~`/`~/` against the home dir,
/// then `${NAME}` placeholders from the env (manual — no new dep).
/// Unknown vars stay literal (visible, not silently empty).
pub(crate) fn expand_path(raw: &str) -> PathBuf {
    let mut path = raw.to_string();
    if path == "~" {
        if let Some(home) = home_dir() {
            path = home.display().to_string();
        }
    } else if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        path = home.join(rest).display().to_string();
    }
    while let Some(start) = path.find("${")
        && let Some(end_rel) = path[start..].find('}')
    {
        let end = start + end_rel;
        let name = &path[start + 2..end];
        let replacement = std::env::var(name).unwrap_or_else(|_| format!("${{{name}}}"));
        path.replace_range(start..=end, &replacement);
        // A var that expands to contain "${" again would loop forever;
        // unknown vars keep their literal braces and advance past them.
        if replacement.starts_with("${") {
            break;
        }
    }
    PathBuf::from(path)
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// Where discovery looks — parameterized so unit tests inject temp
/// dirs (cwd + convention roots) without touching the real home.
#[derive(Debug)]
pub(crate) struct DiscoveryEnv {
    pub cwd: PathBuf,
    pub roots: Vec<PathBuf>,
}

/// Resolve the rig end-to-end: discovery → the one `config` run → a
/// [`RigPlan`] whose `.name` every later op passes as explicit `-p`.
pub async fn resolve_plan(
    runner: &dyn ComposeRunner,
    selection: RigSelection,
    config: &Config,
) -> Result<RigPlan, CoreError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env = DiscoveryEnv {
        cwd,
        roots: whk_roots(),
    };
    resolve_plan_with(runner, selection, config, &env).await
}

/// The parameterized core of [`resolve_plan`] (tests inject cwd/roots).
pub(crate) async fn resolve_plan_with(
    runner: &dyn ComposeRunner,
    selection: RigSelection,
    config: &Config,
    env: &DiscoveryEnv,
) -> Result<RigPlan, CoreError> {
    let known: Vec<String> = config.rigs.keys().cloned().collect();

    // Levels 1+2: an explicit name (the flag already folded IGNITION_RIG).
    if let RigSelection::Named(name) = &selection {
        return match config.rigs.get(name) {
            Some(entry) => resolve_entry(runner, name, entry).await,
            None => Err(CoreError::Rig(format!(
                "rig {name:?} not found (known rigs: {known:?}); add a [rigs.{name}] \
                 entry or run from the rig's directory"
            ))),
        };
    }

    // Level 3: the config default — an explicit user preference, so it
    // outranks the cwd scan (must-have truth #4), and a stale one is a
    // LOUD error rather than a surprise scan.
    if let Some(default) = config.rig.default.as_deref() {
        return match config.rigs.get(default) {
            Some(entry) => resolve_entry(runner, default, entry).await,
            None => Err(CoreError::Rig(format!(
                "[rig] default {default:?} names no [rigs.{default}] entry \
                 (known rigs: {known:?})"
            ))),
        };
    }

    let mut trail: Vec<String> = Vec::new();

    // Level 4: cwd candidates, in order.
    for candidate in CWD_CANDIDATES {
        let path = env.cwd.join(candidate);
        if path.is_file() {
            return resolve_file(runner, &path).await;
        }
        trail.push(candidate.to_string());
    }

    // Level 5: WHK conventions — git-module first, then WHK-Global,
    // each under BOTH home roots (first hit wins).
    for relpath in [GIT_MODULE_RELPATH, WHK_GLOBAL_RELPATH] {
        for root in &env.roots {
            let path = root.join(relpath);
            if path.is_file() {
                return resolve_file(runner, &path).await;
            }
            trail.push(path.display().to_string());
        }
    }

    Err(CoreError::Rig(format!(
        "no compose file discovered — pass --rig NAME, set IGNITION_RIG, configure \
         [rig].default/[rigs.NAME], or run from a directory with a compose file \
         (searched cwd candidates {trail:?}; WHK convention roots {:?})",
        env.roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
    )))
}

/// Resolve one `[rigs.NAME]` entry: expand its path, then the shared
/// resolve run. An explicit `project_name` on the entry overrides the
/// resolved `.name` (a deliberate escape hatch — omit it to honor the
/// rig's own `.env`).
async fn resolve_entry(
    runner: &dyn ComposeRunner,
    name: &str,
    entry: &crate::config::RigEntry,
) -> Result<RigPlan, CoreError> {
    let file = expand_path(&entry.compose_file);
    if !file.is_file() {
        return Err(CoreError::Rig(format!(
            "rig {name:?}: compose file {} not found",
            file.display()
        )));
    }
    let mut plan = resolve_file(runner, &file).await?;
    if let Some(project_name) = &entry.project_name {
        plan.name.clone_from(project_name);
    }
    Ok(plan)
}

/// The resolve-then-act step: ONE `config --format json` run through
/// the runner; the parsed `.name` is the identity truth.
async fn resolve_file(runner: &dyn ComposeRunner, file: &Path) -> Result<RigPlan, CoreError> {
    let project_dir = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let output = runner.run(&config_args(file, &project_dir)).await;
    let stdout = compose::check_output(&output, "docker compose config")?;
    parse_config(stdout, file, &project_dir)
}

/// A host port already held by something that is NOT this rig's own
/// project (same-project occupants are recreate-safe).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PortConflict {
    /// The contested host port.
    pub port: u16,
    /// Who holds it: `container <name> (rig <project>)` /
    /// `container <name> (no compose project)` /
    /// `host process <name> (pid <pid>)`.
    pub attribution: String,
}

/// Port pre-flight (research Pattern 3), run before `up`: per host
/// port, `docker ps --filter publish=<port> --format json` first (rich
/// attribution); when docker reports no occupant, an advisory `lsof`
/// pass attributes non-docker HOST processes. `lsof` absence is
/// tolerated silently (skip — it is advisory-only). Occupants belonging
/// to THIS project are fine (a recreate); anything else is a conflict.
pub async fn port_preflight(
    runner: &dyn ComposeRunner,
    plan: &RigPlan,
) -> Result<Vec<PortConflict>, CoreError> {
    let mut conflicts = Vec::new();
    for port in &plan.host_ports {
        let output = runner.run_docker(&docker_ps_publish_args(*port)).await;
        let stdout = compose::check_output(&output, "docker ps")?;
        let entries = parse_docker_ps_ldjson(stdout);
        for entry in &entries {
            match entry.compose_project.as_deref() {
                Some(project) if project == plan.name => { /* recreate-safe */ }
                Some(other) => conflicts.push(PortConflict {
                    port: *port,
                    attribution: format!("container {} (rig {})", entry.name, other),
                }),
                None => conflicts.push(PortConflict {
                    port: *port,
                    attribution: format!("container {} (no compose project)", entry.name),
                }),
            }
        }
        if entries.is_empty()
            && let Some(process) = lsof_listener(*port)
        {
            conflicts.push(PortConflict {
                port: *port,
                attribution: process,
            });
        }
    }
    Ok(conflicts)
}

/// Advisory host-process attribution: the LISTENing process on `port`
/// per `lsof -nP -iTCP:<port> -sTCP:LISTEN` (first matching row's
/// NAME (PID)). Any failure — including lsof simply being absent —
/// is `None` (skip silently; docker attribution is the primary pass).
fn lsof_listener(port: u16) -> Option<String> {
    let output = std::process::Command::new("lsof")
        .args([
            "-nP".to_string(),
            format!("-iTCP:{port}"),
            "-sTCP:LISTEN".to_string(),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.contains(format!(":{port}").as_str()))
        .or_else(|| stdout.lines().nth(1))?;
    let mut fields = line.split_whitespace();
    let name = fields.next()?.to_string();
    let pid = fields.next()?;
    Some(format!("host process {name} (pid {pid})"))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::{
        ComposeOutput, ComposeRunner, DockerPsEntry, DiscoveryEnv, PortConflict, RigSelection,
        WHK_HOME_ROOTS, config_args, expand_path, parse_config, parse_docker_ps_ldjson,
        port_preflight, resolve_plan_with,
    };
    use crate::config::{Config, RigConfig, RigEntry};
    use crate::error::CoreError;

    /// A minimal valid compose file for fixtures that need a real file
    /// on disk (discovery checks existence before resolving).
    const MINIMAL_COMPOSE: &str = "services:\n  sidecar:\n    image: alpine:latest\n";

    /// The resolve run's scripted answer: a one-service project.
    const RESOLVE_STDOUT: &str = r#"{"name":"fixture-rig","services":{"sidecar":{"image":"alpine"}},"volumes":{}}"#;

    /// Scripted fake runner: records (program, args) per call and serves
    /// queued outputs. `run` and `run_docker` are recorded separately so
    /// tests can assert WHICH program shape an op used.
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

        fn next(&self, program: &'static str, args: &[String]) -> ComposeOutput {
            self.calls
                .lock()
                .unwrap()
                .push((program, args.to_vec()));
            self.outputs
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted outputs exhausted")
        }

        fn calls(&self) -> Vec<(&'static str, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ComposeRunner for FakeRunner {
        async fn run(&self, args: &[String]) -> ComposeOutput {
            self.next("docker compose", args)
        }

        async fn run_docker(&self, args: &[String]) -> ComposeOutput {
            self.next("docker", args)
        }
    }

    fn ok(stdout: &str) -> ComposeOutput {
        ComposeOutput {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        }
    }

    fn resolve_output() -> ComposeOutput {
        ok(RESOLVE_STDOUT)
    }

    fn entry(path: &Path) -> RigEntry {
        RigEntry {
            compose_file: path.display().to_string(),
            project_name: None,
        }
    }

    fn discovery_env(cwd: &Path, roots: &[PathBuf]) -> DiscoveryEnv {
        DiscoveryEnv {
            cwd: cwd.to_path_buf(),
            roots: roots.to_vec(),
        }
    }

    // ----- path expansion -------------------------------------------------

    #[test]
    fn expand_path_tilde_and_env_vars() {
        let lock = crate::config::ENV_LOCK.lock().expect("env lock");
        // SAFETY: single-threaded under ENV_LOCK; restored before return.
        unsafe { std::env::set_var("IGNITION_TEST_VAR", "expanded") };

        assert_eq!(
            expand_path("${IGNITION_TEST_VAR}/rigs"),
            PathBuf::from("expanded/rigs")
        );
        // Unknown vars stay literal (visible, not silently empty).
        assert_eq!(
            expand_path("${IGNITION_NO_SUCH_VAR}/x"),
            PathBuf::from("${IGNITION_NO_SUCH_VAR}/x")
        );
        // Plain paths pass through untouched.
        assert_eq!(expand_path("/abs/path"), PathBuf::from("/abs/path"));

        // ~ expands against the real home.
        if let Some(home) = directories::BaseDirs::new() {
            let home = home.home_dir();
            assert_eq!(expand_path("~/"), home.to_path_buf());
            assert_eq!(expand_path("~"), home.to_path_buf());
            assert_eq!(
                expand_path("~/sub/rig"),
                home.join("sub/rig")
            );
        }
        // SAFETY: single-threaded under ENV_LOCK.
        unsafe { std::env::remove_var("IGNITION_TEST_VAR") };
        drop(lock);
    }

    #[test]
    fn whk_roots_const_pins_both_home_roots_in_order() {
        assert_eq!(WHK_HOME_ROOTS, &["~/Documents/whiskeyhouse", "~/whiskeyhouse"]);
    }

    // ----- levels 1/2/3: named + config default ---------------------------

    #[tokio::test]
    async fn named_rig_resolves_and_runner_receives_exact_config_args() {
        let dir = tempfile::tempdir().expect("tempdir");
        let compose = dir.path().join("docker-compose.yml");
        std::fs::write(&compose, MINIMAL_COMPOSE).expect("write compose");

        let mut config = Config::default();
        config.rigs.insert("git-module".into(), entry(&compose));

        let runner = FakeRunner::with(vec![resolve_output()]);
        let plan = resolve_plan_with(
            &runner,
            RigSelection::Named("git-module".into()),
            &config,
            &discovery_env(Path::new("/elsewhere"), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect("named rig resolves");
        assert_eq!(plan.name, "fixture-rig");
        assert_eq!(plan.compose_file, compose);

        let calls = runner.calls();
        assert_eq!(calls.len(), 1, "exactly one resolve run");
        assert_eq!(
            calls[0],
            (
                "docker compose",
                config_args(&compose, dir.path()),
            ),
            "resolve-then-act: -f + --project-directory + config --format json"
        );
    }

    #[tokio::test]
    async fn named_rig_miss_lists_knowns() {
        let mut config = Config::default();
        config.rigs.insert(
            "alpha".into(),
            entry(Path::new("/does-not-matter.yml")),
        );

        let err = resolve_plan_with(
            &FakeRunner::default(),
            RigSelection::Named("nope".into()),
            &config,
            &discovery_env(Path::new("/elsewhere"), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect_err("unknown rig name errors");
        let message = err.to_string();
        assert!(matches!(err, CoreError::Rig(_)));
        assert_eq!(err.exit_code(), 7, "rig_error class");
        assert!(message.contains("known rigs: [\"alpha\"]"), "{message}");
    }

    #[tokio::test]
    async fn named_rig_with_missing_file_errors_with_expanded_path() {
        let mut config = Config::default();
        config.rigs.insert(
            "ghost".into(),
            entry(Path::new("/nonexistent/ghost-compose.yml")),
        );
        let err = resolve_plan_with(
            &FakeRunner::default(),
            RigSelection::Named("ghost".into()),
            &config,
            &discovery_env(Path::new("/elsewhere"), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect_err("missing compose file errors");
        assert!(
            err.to_string().contains("ghost-compose.yml not found"),
            "{}",
            err
        );
    }

    #[tokio::test]
    async fn project_name_override_wins_over_resolved_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let compose = dir.path().join("compose.yml");
        std::fs::write(&compose, MINIMAL_COMPOSE).expect("write compose");

        let mut config = Config::default();
        config.rigs.insert(
            "override".into(),
            RigEntry {
                compose_file: compose.display().to_string(),
                project_name: Some("explicit-name".into()),
            },
        );

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Named("override".into()),
            &config,
            &discovery_env(Path::new("/elsewhere"), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect("override resolves");
        assert_eq!(plan.name, "explicit-name");
    }

    /// The precedence pin (must-have truth #4): `[rig].default` BEATS a
    /// cwd full of compose candidates — the explicit user preference
    /// outranks context scanning.
    #[tokio::test]
    async fn config_default_beats_cwd_candidates() {
        let rig_dir = tempfile::tempdir().expect("tempdir");
        let rig_compose = rig_dir.path().join("remote-compose.yml");
        std::fs::write(&rig_compose, MINIMAL_COMPOSE).expect("write rig compose");

        let cwd = tempfile::tempdir().expect("tempdir");
        std::fs::write(cwd.path().join("compose.yml"), MINIMAL_COMPOSE)
            .expect("write cwd compose");

        let mut config = Config {
            rig: RigConfig {
                default: Some("remote".into()),
            },
            ..Config::default()
        };
        config.rigs.insert("remote".into(), entry(&rig_compose));

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Auto,
            &config,
            &discovery_env(cwd.path(), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect("default resolves");
        assert_eq!(
            plan.compose_file, rig_compose,
            "the [rig].default entry wins over the cwd compose.yml"
        );
    }

    #[tokio::test]
    async fn stale_default_is_a_loud_error() {
        let config = Config {
            rig: RigConfig {
                default: Some("ghost".into()),
            },
            ..Config::default()
        };
        let err = resolve_plan_with(
            &FakeRunner::default(),
            RigSelection::Auto,
            &config,
            &discovery_env(Path::new("/elsewhere"), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect_err("stale default errors");
        let message = err.to_string();
        assert!(message.contains("[rig] default"), "{message}");
        assert!(message.contains("ghost"), "{message}");
    }

    // ----- level 4: cwd candidates ----------------------------------------

    #[tokio::test]
    async fn cwd_candidates_probed_in_order() {
        let cwd = tempfile::tempdir().expect("tempdir");
        // The FIRST candidate: ./docker/compose.yml.
        std::fs::create_dir(cwd.path().join("docker")).expect("mkdir");
        let first = cwd.path().join("docker/compose.yml");
        std::fs::write(&first, MINIMAL_COMPOSE).expect("write first candidate");
        // A later candidate also exists — order must pick the first.
        std::fs::write(cwd.path().join("docker-compose.yml"), MINIMAL_COMPOSE)
            .expect("write later candidate");

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(cwd.path(), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect("cwd candidate resolves");
        assert_eq!(plan.compose_file, first);

        // With NO candidate present (empty cwd): falls through to the
        // roots, then errors with the trail — covered below.
    }

    #[tokio::test]
    async fn no_rig_anywhere_errors_with_search_trail() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let err = resolve_plan_with(
            &FakeRunner::default(),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(cwd.path(), &[PathBuf::from("/tmp/no-such-root")]),
        )
        .await
        .expect_err("nothing found errors");
        let message = err.to_string();
        assert!(matches!(err, CoreError::Rig(_)));
        assert_eq!(err.exit_code(), 7);
        assert!(message.contains("no compose file discovered"), "{message}");
        assert!(
            message.contains("docker/compose.yml") && message.contains("docker-compose.yml"),
            "cwd candidates named in the trail: {message}"
        );
        assert!(
            message.contains("/tmp/no-such-root"),
            "convention roots named in the trail: {message}"
        );
    }

    // ----- level 5: WHK conventions (both roots, first hit wins) ----------

    #[tokio::test]
    async fn git_module_convention_probes_both_roots_first_hit_wins() {
        let root1 = tempfile::tempdir().expect("root1");
        let root2 = tempfile::tempdir().expect("root2");
        // Only root2 has the git-module repo.
        let path = root2
            .path()
            .join("ignition-git-module/docker/docker-compose.yml");
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        std::fs::write(&path, MINIMAL_COMPOSE).expect("write");

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(Path::new("/empty-cwd"), &[root1.path().into(), root2.path().into()]),
        )
        .await
        .expect("level-5 resolves via the second root");
        assert_eq!(plan.compose_file, path);
    }

    #[tokio::test]
    async fn whk_global_convention_tried_after_git_module() {
        let root = tempfile::tempdir().expect("root");
        // No git-module repo; WHK-Global present.
        let path = root.path().join("whk-environment-orchestration/docker-compose.yml");
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        std::fs::write(&path, MINIMAL_COMPOSE).expect("write");

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(Path::new("/empty-cwd"), &[root.path().into()]),
        )
        .await
        .expect("WHK-Global convention resolves");
        assert_eq!(plan.compose_file, path);
    }

    #[tokio::test]
    async fn git_module_beats_whk_global_when_both_exist() {
        let root = tempfile::tempdir().expect("root");
        let git_module = root
            .path()
            .join("ignition-git-module/docker/docker-compose.yml");
        std::fs::create_dir_all(git_module.parent().unwrap()).expect("mkdir");
        std::fs::write(&git_module, MINIMAL_COMPOSE).expect("write");
        let whk_global = root
            .path()
            .join("whk-environment-orchestration/docker-compose.yml");
        std::fs::create_dir_all(whk_global.parent().unwrap()).expect("mkdir");
        std::fs::write(&whk_global, MINIMAL_COMPOSE).expect("write");

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(Path::new("/empty-cwd"), &[root.path().into()]),
        )
        .await
        .expect("conventions resolve");
        assert_eq!(
            plan.compose_file, git_module,
            "git-module outranks WHK-Global (discovery order)"
        );
    }

    /// First root wins when BOTH roots carry the same convention repo —
    /// the `~/Documents/whiskeyhouse/`-first ordering pin.
    #[tokio::test]
    async fn first_root_wins_when_both_roots_have_the_repo() {
        let root1 = tempfile::tempdir().expect("root1");
        let root2 = tempfile::tempdir().expect("root2");
        let in_root1 = root1
            .path()
            .join("whk-environment-orchestration/docker-compose.yml");
        std::fs::create_dir_all(in_root1.parent().unwrap()).expect("mkdir");
        std::fs::write(&in_root1, MINIMAL_COMPOSE).expect("write");
        let in_root2 = root2
            .path()
            .join("whk-environment-orchestration/docker-compose.yml");
        std::fs::create_dir_all(in_root2.parent().unwrap()).expect("mkdir");
        std::fs::write(&in_root2, MINIMAL_COMPOSE).expect("write");

        let plan = resolve_plan_with(
            &FakeRunner::with(vec![resolve_output()]),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(Path::new("/empty-cwd"), &[root1.path().into(), root2.path().into()]),
        )
        .await
        .expect("resolves");
        assert_eq!(plan.compose_file, in_root1, "first root wins");
    }

    // ----- resolve failures propagate as Rig ------------------------------

    #[tokio::test]
    async fn failing_config_run_maps_to_rig_error_with_tail() {
        let cwd = tempfile::tempdir().expect("tempdir");
        std::fs::write(cwd.path().join("compose.yml"), MINIMAL_COMPOSE)
            .expect("write compose");

        let failed = ComposeOutput {
            stdout: String::new(),
            stderr: "no configuration file provided at ./compose.yml\n".into(),
            code: 14,
        };
        let err = resolve_plan_with(
            &FakeRunner::with(vec![failed]),
            RigSelection::Auto,
            &Config::default(),
            &discovery_env(cwd.path(), &[PathBuf::from("/no-roots")]),
        )
        .await
        .expect_err("config failure propagates");
        let message = err.to_string();
        assert!(message.contains("docker compose config failed (exit 14)"), "{message}");
        assert!(message.contains("no configuration file"), "{message}");
    }

    // ----- port pre-flight --------------------------------------------------

    fn plan_with_port(port: u16) -> crate::rig::RigPlan {
        parse_config(
            &format!(
                r#"{{"name":"mine","services":{{"gw":{{"ports":[{{"target":8088,"published":"{port}"}}]}}}}}}"#
            ),
            Path::new("/p/compose.yml"),
            Path::new("/p"),
        )
        .expect("plan parses")
    }

    #[tokio::test]
    async fn preflight_free_port_reports_no_conflicts() {
        // Port 1: privileged, nothing listens on it in test envs, and
        // even where lsof exists it finds no listener → the clean case.
        let runner = FakeRunner::with(vec![ok("")]);
        let conflicts = port_preflight(&runner, &plan_with_port(1))
            .await
            .expect("preflight runs");
        assert!(conflicts.is_empty(), "{conflicts:?}");
        // The docker-attribution shape ran via run_docker (plain docker).
        assert_eq!(runner.calls()[0].0, "docker");
    }

    #[tokio::test]
    async fn preflight_same_project_occupant_is_recreate_safe() {
        let occupant = r#"{"Names":"mine-gw-1","Labels":"com.docker.compose.project=mine"}"#;
        let runner = FakeRunner::with(vec![ok(occupant)]);
        let conflicts = port_preflight(&runner, &plan_with_port(18088))
            .await
            .expect("preflight runs");
        assert!(conflicts.is_empty(), "own project → recreate, not conflict");
    }

    #[tokio::test]
    async fn preflight_cross_project_occupant_conflicts_with_attribution() {
        let occupant = r#"{"Names":"other-gw-1","Labels":"com.docker.compose.project=other"}"#;
        let runner = FakeRunner::with(vec![ok(occupant)]);
        let conflicts = port_preflight(&runner, &plan_with_port(18088))
            .await
            .expect("preflight runs");
        assert_eq!(
            conflicts,
            vec![PortConflict {
                port: 18088,
                attribution: "container other-gw-1 (rig other)".into(),
            }],
            "the plan's Rig error reads: port 18088 in use by container other-gw-1 (rig other)"
        );
    }

    #[tokio::test]
    async fn preflight_non_compose_occupant_conflicts() {
        let occupant = r#"{"Names":"standalone","Labels":""}"#;
        let runner = FakeRunner::with(vec![ok(occupant)]);
        let conflicts = port_preflight(&runner, &plan_with_port(18088))
            .await
            .expect("preflight runs");
        assert_eq!(
            conflicts,
            vec![PortConflict {
                port: 18088,
                attribution: "container standalone (no compose project)".into(),
            }]
        );
    }

    #[tokio::test]
    async fn preflight_docker_failure_maps_to_rig_error() {
        let runner = FakeRunner::with(vec![ComposeOutput {
            stdout: String::new(),
            stderr: "docker daemon not running".into(),
            code: 1,
        }]);
        let err = port_preflight(&runner, &plan_with_port(18088))
            .await
            .expect_err("docker ps failure errors");
        assert!(err.to_string().contains("docker ps failed"), "{err}");
    }

    /// The docker-ps parser handles the map-Labels shape too (kept here
    /// next to its consumer for the attribution story).
    #[test]
    fn docker_ps_map_labels_attributed() {
        let entries: Vec<DockerPsEntry> =
            parse_docker_ps_ldjson(r#"{"Names":"x","Labels":{"com.docker.compose.project":"p"}}"#);
        assert_eq!(entries[0].compose_project.as_deref(), Some("p"));
    }
}
