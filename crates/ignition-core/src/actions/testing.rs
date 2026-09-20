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

/// The report `ign testing run` renders into `data.report`.
///
/// The gateway route CAN emit junit/text itself, but only its json
/// path sets the 200/207 status and returns machine counts — its
/// junit answer is raw XML and its text answer is raw `<pre>` text,
/// both at HTTP 200 with no `passed/failed/errors/total` anywhere.
/// A straight passthrough would therefore make a RED suite exit 0 in
/// junit and text mode, the exact opposite of what a CI flag is for
/// (and XML on stdout would be a new exception to the envelope
/// purity contract that `contract_stdout_purity` pins).
///
/// So the CLI always POSTs `format: "json"` and renders the report
/// locally, as pure functions: ONE suite execution, one round trip,
/// and the verdict + exit code are IDENTICAL in every format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TestingFormat {
    /// Structured results only — `data.report` stays empty.
    #[default]
    Json,
    /// JUnit XML in `data.report`, for a CI test reporter.
    Junit,
    /// Console-style text in `data.report`, for a human reading logs.
    Text,
}

impl TestingFormat {
    /// The flag value / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Junit => "junit",
            Self::Text => "text",
        }
    }
}

/// `ign testing run` inputs. `discover` / `module` / `package` are
/// mutually exclusive — enforced HERE as well as in clap (defence in
/// depth: the TUI and the MCP bridge call the action directly).
#[derive(Debug, Clone, Default)]
pub struct TestingRunOptions {
    /// List the gateway's discovered test modules instead of running.
    pub discover: bool,
    /// Run ONE dotted module (`proj.foo_test`).
    pub module: Option<String>,
    /// Run every module under a package prefix (`proj.`).
    pub package: Option<String>,
    /// The report rendered into `data.report` (see [`TestingFormat`]).
    pub format: TestingFormat,
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
    /// The rendered report for `--format junit|text` (empty string in
    /// json and discover modes — the key still rides).
    pub report: String,
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
            report: String::new(),
            results: serde_json::Value::Null,
        }
    }
}

/// `ign testing run` — prove the bundle answers, then discover or run.
///
/// Sequence (the `script_run` probe-then-act precedent —
/// correctness over latency): the usage gate FIRST (zero HTTP), then
/// the raw-status presence probe (one bounded lazy-compile retry,
/// whose 200 body IS the discover answer), then the run POST.
pub async fn testing_run(
    api: &ReqwestGatewayApi,
    project: &str,
    opts: &TestingRunOptions,
) -> Result<TestingRunResult, CoreError> {
    // Usage errors LEAD (the 03-03 put convention): the selectors are
    // three ways to answer the same question and combining them has
    // no meaning. Refuses BEFORE any network I/O.
    check_selector_exclusivity(opts)?;

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

    if opts.discover {
        return Ok(TestingRunResult::discovered(project, &discovered));
    }

    // The run leg rides the CLASSIFIED path — 200 and 207 both pass
    // through classify untouched, and the probe already proved the
    // route present, so anything else here is a genuine failure.
    let results = run_with_warmup(api, project, &run_body(opts)).await?;
    Ok(map_run(project, opts.format, results))
}

/// The three-way selector gate: `--discover`, `--module` and
/// `--package` are mutually exclusive. `invalid_input` (exit 2)
/// naming BOTH offending flags, before any gateway I/O — the
/// `actions::script::read_script_input` refusal shape.
fn check_selector_exclusivity(opts: &TestingRunOptions) -> Result<(), CoreError> {
    let refuse = |first: &str, second: &str| {
        Err(CoreError::InvalidInput {
            reason: format!(
                "{first} and {second} are mutually exclusive — they are different \
                 ways to answer the same question: pass exactly one (or none, to \
                 run every discovered module)"
            ),
        })
    };
    match (opts.discover, opts.module.is_some(), opts.package.is_some()) {
        (true, true, _) => refuse("--discover", "--module"),
        (true, _, true) => refuse("--discover", "--package"),
        (_, true, true) => refuse("--module", "--package"),
        _ => Ok(()),
    }
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

/// The run POST with the SAME single warmup retry as the probe leg —
/// the classified path turns a 500 into `Internal`, and a first-touch
/// blowup on the run leg is the same lazily-compiled-module story
/// (the probe warms `testing.runner`; the run leg can be the first
/// touch of a test module's own imports).
async fn run_with_warmup(
    api: &ReqwestGatewayApi,
    project: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, CoreError> {
    match crate::client::webdev::testing_run(api, project, body).await {
        Ok(results) => Ok(results),
        Err(_) => {
            tokio::time::sleep(LAZY_COMPILE_WARMUP).await;
            crate::client::webdev::testing_run(api, project, body).await
        }
    }
}

/// The POST body: ONLY the keys the caller set, plus the always-json
/// format (the report is rendered client-side — see
/// [`TestingFormat`]).
fn run_body(opts: &TestingRunOptions) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(module) = &opts.module {
        body.insert("module".to_string(), serde_json::json!(module));
    }
    if let Some(package) = &opts.package {
        body.insert("package".to_string(), serde_json::json!(package));
    }
    body.insert("format".to_string(), serde_json::json!("json"));
    serde_json::Value::Object(body)
}

/// Map the route's run answer onto the result. The verdict comes
/// from `failed + errors`, NEVER from the HTTP status (200 and 207
/// are both success transport shapes).
fn map_run(project: &str, format: TestingFormat, results: serde_json::Value) -> TestingRunResult {
    let count_of = |key: &str| {
        results
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
    };
    let failed = count_of("failed");
    let errors = count_of("errors");
    let red = failed + errors > 0;
    let modules = results
        .get("modules")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("module")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let report = match format {
        TestingFormat::Json => String::new(),
        TestingFormat::Junit => to_junit_xml(&results),
        TestingFormat::Text => to_console(&results),
    };
    TestingRunResult {
        mode: "run".to_string(),
        project: project.to_string(),
        count: modules.len() as u32,
        modules,
        verdict: Some(if red { "failed" } else { "passed" }.to_string()),
        slug: red.then(|| TESTS_FAILED_SLUG.to_string()),
        passed: count_of("passed"),
        failed,
        skipped: count_of("skipped"),
        errors,
        total: count_of("total"),
        duration_ms: u64_of(&results, "duration_ms"),
        report,
        results,
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

/// The per-module views a report iterates (`results.modules`, or the
/// whole body when the route answered a single-module shape — the
/// gateway reporter's own `if not modules: modules = [results]`).
fn report_modules(results: &serde_json::Value) -> Vec<&serde_json::Value> {
    match results.get("modules").and_then(serde_json::Value::as_array) {
        Some(modules) if !modules.is_empty() => modules.iter().collect(),
        _ => vec![results],
    }
}

/// `key` as u64, 0 when absent/ill-typed.
fn u64_of(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// `key` as &str, `fallback` when absent/ill-typed.
fn str_of<'a>(value: &'a serde_json::Value, key: &str, fallback: &'a str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

/// JUnit XML from the structured results — a PURE function, unit
/// tested without a gateway. Mirrors the attribute set and element
/// shapes of the bundle's `testing/reporter/code.py::to_junit_xml`
/// so a CI reporter sees the same document either way.
pub fn to_junit_xml(results: &serde_json::Value) -> String {
    let mut parts = vec![r#"<?xml version="1.0" encoding="UTF-8"?>"#.to_string()];
    parts.push(format!(
        r#"<testsuites tests="{}" failures="{}" errors="{}" skipped="{}" time="{:.3}">"#,
        u64_of(results, "total"),
        u64_of(results, "failed"),
        u64_of(results, "errors"),
        u64_of(results, "skipped"),
        u64_of(results, "duration_ms") as f64 / 1000.0,
    ));
    for module in report_modules(results) {
        let module_name = str_of(module, "module", "unknown");
        let cases = module
            .get("results")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        parts.push(format!(
            r#"  <testsuite name="{}" tests="{}" failures="{}" errors="{}" skipped="{}" time="{:.3}">"#,
            escape_xml(module_name),
            cases.len(),
            u64_of(module, "failed"),
            u64_of(module, "errors"),
            u64_of(module, "skipped"),
            u64_of(module, "duration_ms") as f64 / 1000.0,
        ));
        for case in cases {
            parts.push(format!(
                r#"    <testcase name="{}" classname="{}" time="{:.3}">"#,
                escape_xml(str_of(case, "name", "unknown")),
                escape_xml(module_name),
                u64_of(case, "duration_ms") as f64 / 1000.0,
            ));
            let message = escape_xml(str_of(case, "message", ""));
            let traceback = escape_xml(str_of(case, "traceback", ""));
            match str_of(case, "status", "error") {
                "failed" => parts.push(format!(
                    r#"      <failure message="{message}">{traceback}</failure>"#
                )),
                "error" => parts.push(format!(
                    r#"      <error message="{message}">{traceback}</error>"#
                )),
                "skipped" => parts.push(format!(r#"      <skipped message="{message}" />"#)),
                _ => {}
            }
            parts.push("    </testcase>".to_string());
        }
        parts.push("  </testsuite>".to_string());
    }
    parts.push("</testsuites>".to_string());
    parts.join("\n")
}

/// Console-style text from the structured results — a PURE function,
/// mirroring the bundle's `testing/reporter/code.py::to_console`.
pub fn to_console(results: &serde_json::Value) -> String {
    let rule = "=".repeat(60);
    let mut lines = vec![rule.clone(), "TEST RESULTS".to_string(), rule.clone()];
    for module in report_modules(results) {
        lines.push(String::new());
        lines.push(format!("--- {} ---", str_of(module, "module", "unknown")));
        let cases = module
            .get("results")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for case in cases {
            let status = str_of(case, "status", "error").to_uppercase();
            let marker = match status.as_str() {
                "PASSED" => "  PASS",
                "FAILED" => "  FAIL",
                "SKIPPED" => "  SKIP",
                _ => " ERROR",
            };
            let mut line = format!("{marker}  {}", str_of(case, "name", "unknown"));
            let duration = u64_of(case, "duration_ms");
            if duration > 0 {
                line.push_str(&format!("  ({duration}ms)"));
            }
            lines.push(line);
            let message = str_of(case, "message", "");
            if !message.is_empty() && matches!(status.as_str(), "FAILED" | "ERROR") {
                lines.push(format!("         {message}"));
            }
        }
    }
    lines.push(String::new());
    lines.push(rule.clone());
    lines.push(format!(
        "Total: {}  Passed: {}  Failed: {}  Skipped: {}  Errors: {}  ({}ms)",
        u64_of(results, "total"),
        u64_of(results, "passed"),
        u64_of(results, "failed"),
        u64_of(results, "skipped"),
        u64_of(results, "errors"),
        u64_of(results, "duration_ms"),
    ));
    lines.push(rule);
    lines.join("\n")
}

/// XML escaping with the control-character STRIP the gateway
/// reporter also does — XML 1.0 forbids most characters below 0x20
/// (only `\t \n \r` are legal), so an unstripped traceback would
/// make the document malformed, not merely unescaped.
fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text
        .chars()
        .filter(|ch| matches!(ch, '\t' | '\n' | '\r') || *ch >= '\u{20}')
    {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sample the renderers are pinned against — the route's own
    /// `run_all` shape.
    fn sample() -> serde_json::Value {
        serde_json::json!({
            "passed": 1,
            "failed": 1,
            "skipped": 1,
            "errors": 0,
            "total": 3,
            "duration_ms": 1500,
            "modules": [{
                "module": "proj.foo_test",
                "passed": 1,
                "failed": 1,
                "skipped": 1,
                "errors": 0,
                "duration_ms": 1500,
                "results": [
                    {"name": "test_ok", "status": "passed", "duration_ms": 5},
                    {"name": "test_bad", "status": "failed", "duration_ms": 7,
                     "message": "1 != 2 <&>", "traceback": "Traceback ..."},
                    {"name": "test_skip", "status": "skipped", "duration_ms": 0,
                     "message": "not today"}
                ]
            }]
        })
    }

    /// The selector gate refuses every pair, naming BOTH flags.
    #[test]
    fn selectors_are_mutually_exclusive() {
        let cases = [
            (
                TestingRunOptions {
                    discover: true,
                    module: Some("a.b".into()),
                    ..Default::default()
                },
                ["--discover", "--module"],
            ),
            (
                TestingRunOptions {
                    discover: true,
                    package: Some("a.".into()),
                    ..Default::default()
                },
                ["--discover", "--package"],
            ),
            (
                TestingRunOptions {
                    module: Some("a.b".into()),
                    package: Some("a.".into()),
                    ..Default::default()
                },
                ["--module", "--package"],
            ),
        ];
        for (opts, flags) in cases {
            let err = check_selector_exclusivity(&opts).expect_err("the pair refuses");
            assert_eq!(err.code(), "invalid_input");
            assert_eq!(err.exit_code(), 2, "usage class");
            let message = err.to_string();
            for flag in flags {
                assert!(message.contains(flag), "message names {flag}: {message}");
            }
        }
        // Zero or one selector is fine (none = run everything).
        assert!(check_selector_exclusivity(&TestingRunOptions::default()).is_ok());
        assert!(
            check_selector_exclusivity(&TestingRunOptions {
                module: Some("a.b".into()),
                ..Default::default()
            })
            .is_ok()
        );
    }

    /// The POST body carries ONLY the keys the caller set, plus the
    /// always-json format — `--format` NEVER changes the wire.
    #[test]
    fn run_body_carries_only_what_was_set() {
        assert_eq!(
            run_body(&TestingRunOptions::default()),
            serde_json::json!({"format": "json"})
        );
        assert_eq!(
            run_body(&TestingRunOptions {
                module: Some("a.b.c_test".into()),
                format: TestingFormat::Junit,
                ..Default::default()
            }),
            serde_json::json!({"module": "a.b.c_test", "format": "json"}),
            "the format flag never leaves the CLI — the report is rendered locally"
        );
        assert_eq!(
            run_body(&TestingRunOptions {
                package: Some("proj.".into()),
                ..Default::default()
            }),
            serde_json::json!({"package": "proj.", "format": "json"})
        );
    }

    /// The verdict comes from `failed + errors`, and `failure_exit_code`
    /// is the 6 the binary passes through.
    #[test]
    fn verdict_reads_the_body_not_the_status() {
        let green = map_run(
            "P",
            TestingFormat::Json,
            serde_json::json!({"passed": 3, "failed": 0, "errors": 0, "total": 3}),
        );
        assert_eq!(green.verdict.as_deref(), Some("passed"));
        assert_eq!(green.slug, None);
        assert_eq!(green.failure_exit_code(), None);
        assert!(green.report.is_empty(), "json mode renders no report");

        // errors alone (zero failures) is still RED.
        let red = map_run(
            "P",
            TestingFormat::Json,
            serde_json::json!({"passed": 3, "failed": 0, "errors": 1, "total": 4}),
        );
        assert_eq!(red.verdict.as_deref(), Some("failed"));
        assert_eq!(red.slug.as_deref(), Some(TESTS_FAILED_SLUG));
        assert_eq!(red.failure_exit_code(), Some(6));
    }

    /// Every format produces the SAME verdict — the point of
    /// rendering locally rather than passing `--format` to the route.
    #[test]
    fn the_verdict_is_identical_in_every_format() {
        for format in [
            TestingFormat::Json,
            TestingFormat::Junit,
            TestingFormat::Text,
        ] {
            let result = map_run("P", format, sample());
            assert_eq!(
                result.verdict.as_deref(),
                Some("failed"),
                "{} keeps the red verdict",
                format.as_str()
            );
            assert_eq!(result.failure_exit_code(), Some(6));
            assert_eq!(result.report.is_empty(), format == TestingFormat::Json);
        }
    }

    /// The JUnit renderer mirrors the gateway reporter's document.
    #[test]
    fn junit_xml_mirrors_the_gateway_reporter() {
        let xml = to_junit_xml(&sample());
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(
            xml.contains(
                r#"<testsuites tests="3" failures="1" errors="0" skipped="1" time="1.500">"#
            ),
            "root attributes carry the counts: {xml}"
        );
        assert!(xml.contains(r#"<testsuite name="proj.foo_test" tests="3""#));
        assert!(
            xml.contains(r#"<testcase name="test_ok" classname="proj.foo_test" time="0.005">"#)
        );
        assert!(
            xml.contains(r#"<failure message="1 != 2 &lt;&amp;&gt;">Traceback ...</failure>"#),
            "the failure message is XML-escaped: {xml}"
        );
        assert!(xml.contains(r#"<skipped message="not today" />"#));
        assert!(xml.ends_with("</testsuites>"));
    }

    /// The console renderer mirrors the gateway reporter's layout.
    #[test]
    fn console_text_mirrors_the_gateway_reporter() {
        let text = to_console(&sample());
        assert!(text.starts_with(&"=".repeat(60)));
        assert!(text.contains("--- proj.foo_test ---"));
        assert!(text.contains("  PASS  test_ok  (5ms)"));
        assert!(text.contains("  FAIL  test_bad  (7ms)"));
        assert!(
            text.contains("         1 != 2 <&>"),
            "a failure prints its message: {text}"
        );
        assert!(text.contains("  SKIP  test_skip"));
        assert!(text.contains("Total: 3  Passed: 1  Failed: 1  Skipped: 1  Errors: 0  (1500ms)"));
    }

    /// Control characters are STRIPPED, not merely escaped (XML 1.0).
    #[test]
    fn escape_xml_strips_illegal_control_characters() {
        assert_eq!(escape_xml("a\u{0}b\u{7}c"), "abc");
        assert_eq!(escape_xml("keep\tthis\nand\rthat"), "keep\tthis\nand\rthat");
        assert_eq!(
            escape_xml(r#"<a href="x">&</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }
}
