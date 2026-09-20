//! The testing action (QUICK-p0g) — `ign testing run`, the
//! standalone verb over the gateway's already-deployed testing
//! bundle (`POST /system/webdev/{project}/testing/run`), which until
//! now only `ign adopt --testing` exercised as a deploy smoke check.
//!
//! Purpose: an agent or a CI job runs the gateway's Jython suite and
//! reads a machine verdict — without `ign adopt`, without the
//! Designer, without the gateway webpage.
//!
//! Two contract rules this module exists to enforce:
//!
//! 1. **The verdict rides the BODY, never the status line.** The
//!    route answers 200 on green and **207** on failures/errors —
//!    both are success transport shapes. `failed + errors > 0` is
//!    the only oracle (the route module's own Pitfall-2 rule).
//! 2. **An absent bundle is a target-state refusal, not a bug.** The
//!    presence probe runs BEFORE any run POST and reads the RAW
//!    status ([`crate::client::webdev::testing_probe`]), because
//!    `classify` collapses 405/500/501 into `Internal` (exit 1) —
//!    which would let "the testing bundle was never deployed" read
//!    as a CLI defect.
//!
//! **The exit-code shape.** A red suite renders the SUCCESS envelope
//! (every result field intact) and exits 6 — the `ign lint --strict`
//! precedent, the repo's sanctioned success-path EXIT exception.
//! `ErrorEnvelope` is LOCKED to `{ok, profile, error{…}}` with no
//! `data` field, so a red run CANNOT carry its results through the
//! error path; and this is how every real test runner behaves (full
//! report on stdout, verdict in the exit code). The stable slug
//! rides the payload as `data.slug = "tests_failed"` — it is NOT a
//! `CoreError` variant and therefore must never appear in the
//! exit-code tables (`readme_exit_table_agreement` cross-checks
//! those against the enum literals, bidirectionally).

use std::time::Duration;

use serde::Serialize;

use crate::client::ReqwestGatewayApi;
use crate::client::webdev::{TestingProbe, testing_probe};
use crate::error::{CoreError, TESTING_ROUTE_PREFIX};

/// The route folder every refusal names (`testing/run`) — BUILT from
/// [`TESTING_ROUTE_PREFIX`] so the raise site and the content address
/// the [`CoreError::RoutesNotDeployed`] hint keys off cannot drift
/// apart (a `testing/`-less route here would silently send the caller
/// to the generic `ign webdev deploy` hint, which does not install
/// the testing bundle).
fn testing_run_route() -> String {
    format!("{TESTING_ROUTE_PREFIX}run")
}

/// The first-touch warmup pause. A freshly deployed route can answer
/// 500 on its FIRST request while WebDev lazily compiles the module;
/// the second hit answers (live-pinned on both a reset rig and a
/// fresh gateway — `actions::adopt`'s empty-suite trap). ONE bounded
/// retry, then the error is real.
const LAZY_COMPILE_WARMUP: Duration = Duration::from_secs(3);

/// The stable red-run slug. Rides the SUCCESS payload as `data.slug`
/// (never the error envelope — see the module docs).
pub const TESTS_FAILED_SLUG: &str = "tests_failed";

/// `ign testing run` inputs.
#[derive(Debug, Clone, Default)]
pub struct TestingRunOptions {
    /// List the gateway's discovered test modules instead of running.
    pub discover: bool,
}

/// `ign testing run` result — ALL keys always present (the family
/// convention: agents never key-hunt). Discover mode leaves the
/// verdict/count fields at their zero/None defaults.
#[derive(Debug, Serialize)]
pub struct TestingRunResult {
    /// `"discover"` or `"run"`.
    pub mode: String,
    /// The project whose testing bundle answered.
    pub project: String,
    /// Discover: every discovered module. Run: the modules executed.
    pub modules: Vec<String>,
    /// Discover: the module count. Run: the module count executed.
    pub count: u32,
    /// `"passed"` | `"failed"` — `null` in discover mode (a
    /// discovery is not a verdict).
    pub verdict: Option<String>,
    /// [`TESTS_FAILED_SLUG`] on a red run, `null` otherwise — the
    /// stable machine slug agents branch on.
    pub slug: Option<String>,
    /// Tests that passed.
    pub passed: u32,
    /// Tests that failed an assertion.
    pub failed: u32,
    /// Tests skipped.
    pub skipped: u32,
    /// Tests that raised (not an assertion failure).
    pub errors: u32,
    /// `passed + failed + skipped + errors`, as the route reports it.
    pub total: u32,
    /// The route-measured wall time in milliseconds.
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    /// The route's answer VERBATIM — `null` in discover mode. Never
    /// interpreted, so a route that grows a field does not need a CLI
    /// release to surface it.
    pub results: serde_json::Value,
}

impl TestingRunResult {
    /// The red-run exit passthrough, mirroring
    /// [`crate::actions::lint::LintResult::strict_exit_code`]:
    /// `Some(6)` (target state — the LOCKED taxonomy's
    /// gateway-state class) when the suite failed, `None` otherwise.
    /// The envelope renders FIRST; the binary decides the exit after.
    pub fn failure_exit_code(&self) -> Option<u8> {
        match self.verdict.as_deref() {
            Some("failed") => Some(6),
            _ => None,
        }
    }

    /// The discover-mode shape: modules + count, no verdict.
    fn discovered(project: &str, discovered: &serde_json::Value) -> Self {
        let modules = string_list(discovered.get("discovered_modules"));
        let count = discovered
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .map_or(modules.len() as u32, |count| count as u32);
        Self {
            mode: "discover".to_string(),
            project: project.to_string(),
            modules,
            count,
            verdict: None,
            slug: None,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
            total: 0,
            duration_ms: 0,
            results: serde_json::Value::Null,
        }
    }
}

/// `ign testing run` — prove the bundle answers, then discover.
///
/// Sequence (the `script_run` probe-then-act precedent —
/// correctness over latency): the raw-status presence probe with one
/// bounded lazy-compile retry, whose 200 body IS the discover
/// answer.
pub async fn testing_run(
    api: &ReqwestGatewayApi,
    project: &str,
    opts: &TestingRunOptions,
) -> Result<TestingRunResult, CoreError> {
    let route = testing_run_route();
    let endpoint = || Some(format!("/system/webdev/{project}/{route}"));
    let discovered = match probe_with_warmup(api, project).await? {
        TestingProbe::Present { discovered } => discovered,
        TestingProbe::Absent => {
            return Err(CoreError::RoutesNotDeployed {
                project: project.to_string(),
                route: route.clone(),
                endpoint: endpoint(),
            });
        }
        TestingProbe::Unlicensed => {
            return Err(CoreError::WebdevUnlicensed {
                endpoint: endpoint(),
            });
        }
        TestingProbe::AuthGated { status } => {
            return Err(CoreError::Auth {
                status,
                endpoint: endpoint(),
            });
        }
        TestingProbe::LazyCompile { body } => {
            // The SECOND 500 is real — the route is deployed and
            // blowing up, which is a bug, not a target state.
            return Err(CoreError::Internal(format!(
                "the testing route answered HTTP 500 twice (the first-touch \
                 lazy-compile warmup did not clear it): {body}"
            )));
        }
    };

    let _ = opts.discover;
    Ok(TestingRunResult::discovered(project, &discovered))
}

/// The probe with the ONE bounded first-touch warmup retry.
async fn probe_with_warmup(
    api: &ReqwestGatewayApi,
    project: &str,
) -> Result<TestingProbe, CoreError> {
    match testing_probe(api, project).await? {
        TestingProbe::LazyCompile { .. } => {
            tokio::time::sleep(LAZY_COMPILE_WARMUP).await;
            testing_probe(api, project).await
        }
        probe => Ok(probe),
    }
}

/// `["a", "b"]` under `field` → the owned string list (non-strings
/// and a missing/ill-typed field degrade to an empty list rather
/// than failing a read).
fn string_list(field: Option<&serde_json::Value>) -> Vec<String> {
    field
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
