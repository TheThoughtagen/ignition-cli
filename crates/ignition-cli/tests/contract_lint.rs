//! Golden-file contract tests for `ign lint` (07-04, INTR-02) — the
//! binary surface of the ignition-lint delegation. The child tool is
//! a fixture executable on a CONTROLLED PATH (assert_cmd's env
//! override — the binary-level isolation the trait-level PATH lock
//! provides in ignition-core's lint_contract.rs). Pins:
//!
//! - **the absent-tool refusal golden**: empty PATH → exit 6
//!   `lint_tool_absent` with the install hint (zero spawns — no
//!   server, no matter: nothing to call);
//! - **the findings-as-data golden**: a fake tool exiting 1 with a
//!   JSON report → the command exits 0 (the doctor posture — the
//!   child RAN), profile NULL (no gateway involvement), data
//!   carrying ran/child_exit_code/issues_found/report/stdout/
//!   stderr_preview — ALL keys always;
//! - **the strict passthrough**: the same fake tool under `--strict`
//!   exits with the child's code LITERALLY (envelope still printed
//!   first) — asserted as a behavior (the exit code varies by child,
//!   so no inline golden: the README carries the note).

use std::path::Path;

use assert_cmd::Command;

/// Isolated config dir (lint never reads it, but the binary always
/// loads config — a broken one would refuse before dispatch).
fn isolated_config() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    (dir, path)
}

/// A fake `ignition-lint` in `dir`: records its argv to the
/// ABSOLUTE `dir/<tag>-argv.txt` (the child inherits the test's CWD,
/// so relative paths would land elsewhere), prints `$LINT_STDOUT`,
/// exits 1.
fn fake_tool(dir: &Path, tag: &str) {
    let tool = dir.join("ignition-lint");
    let argv_out = dir.join(format!("{tag}-argv.txt"));
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\nprintf '%s' \"$LINT_STDOUT\"\nexit 1\n",
        argv_out.display()
    );
    std::fs::write(&tool, script).expect("write fake tool");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tool).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tool, perms).expect("chmod");
    }
}

/// Spawn `ign lint` with the given PATH and stdout payload env.
fn ign_lint(path_var: &str, stdout_payload: &str, args: &[&str]) -> std::process::Output {
    let mut command = Command::cargo_bin("ign").expect("binary 'ign' not found");
    command
        .env("PATH", path_var)
        .env("LINT_STDOUT", stdout_payload);
    command.args(args).output().expect("spawn ign")
}

/// stdout minus the single trailing newline `println!` appends.
fn stdout_for_golden(out: &std::process::Output) -> &str {
    let stdout = std::str::from_utf8(&out.stdout).expect("utf-8 stdout");
    stdout.strip_suffix('\n').unwrap_or(stdout)
}

/// stderr's JSON envelope starting at the first `{` (log-tolerant parse).
fn stderr_envelope(out: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let start = stderr.find('{').unwrap_or(0);
    stderr[start..].to_string()
}

/// The JSON report the fake tool prints — a minimal, honest
/// ignition-lint `--report-format json` shape (one error issue; the
/// parsed `report` Value serializes key-SORTED and the raw `stdout`
/// string goldens under snapbox's backslash→slash normalization, so
/// the fixture stays minimal by design).
const REPORT: &str = r#"{"issues":[{"severity":"error"}],"summary":{"errors":1}}"#;

/// THE absent-tool refusal golden: an empty PATH discovers nothing →
/// exit 6 `lint_tool_absent`, the install hint, nothing on stdout.
#[test]
fn lint_tool_absent_refusal_golden() {
    let empty = tempfile::tempdir().expect("empty path dir");
    let (_config_dir, config) = isolated_config();
    std::fs::write(&config, "active = \"dev\"\n").expect("a loadable config");

    let out = ign_lint(
        empty.path().to_str().unwrap(),
        "",
        &["lint", "src", "--compact"],
    );
    assert_eq!(out.status.code(), Some(6), "target-state class");
    assert!(out.stdout.is_empty(), "errors never touch stdout");
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stderr_envelope(&out),
        snapbox::str![[r#"
{"ok":false,"profile":null,"error":{"code":"lint_tool_absent","message":"ignition-lint is not installed (no executable found on PATH)","endpoint":null,"hint":"install the linter: `uv tool install ignition-lint-toolkit` (or `pip install ignition-lint-toolkit`) — github.com/TheThoughtagen/ignition-lint; then re-run with ignition-lint on PATH"}}

"#]],
    );
}

/// THE findings-as-data golden: the child exits 1 with a JSON report
/// → the COMMAND exits 0 (the doctor posture), profile NULL (the
/// local-delegation contract), data carrying the full shape with
/// child_exit_code 1 + the parsed issues.
#[test]
fn lint_findings_ride_as_data_golden() {
    let tool_dir = tempfile::tempdir().expect("tool dir");
    fake_tool(tool_dir.path(), "data");
    let (_config_dir, config) = isolated_config();
    std::fs::write(&config, "active = \"dev\"\n").expect("a loadable config");

    let out = ign_lint(
        tool_dir.path().to_str().unwrap(),
        REPORT,
        &["lint", "src", "--compact"],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "the doctor posture: the child RAN — findings are data\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // NB: snapbox normalizes backslashes in ACTUAL output to forward
    // slashes — JSON escape pairs in the payload golden as doubled
    // forward slashes is NOT applicable here (this payload has none);
    // the [..] elides nothing dynamic in this golden.
    // NB: snapbox normalizes backslashes in ACTUAL output to forward
    // slashes — the `stdout` string's embedded JSON escapes golden as
    // `/"` (the 03-02 gotcha, applied to embedded JSON).
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"{"ok":true,"profile":null,"data":{"ran":true,"tool":"[..]ignition-lint","child_exit_code":1,"issues_found":1,"report":{"issues":[{"severity":"error"}],"summary":{"errors":1}},"stdout":"{/"issues/":[{/"severity/":/"error/"}],/"summary/":{/"errors/":1}}","stderr_preview":""}}"#]],
    );

    // The arg vector the child saw (recorded by the fake tool):
    // `--report-format json --target src` — the delegation's exact
    // spawn (binary-level proof, the trait-level twin in core).
    let argv = std::fs::read_to_string(tool_dir.path().join("data-argv.txt")).expect("argv");
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        vec!["--report-format", "json", "--target", "src"],
    );
}

/// The HUMAN render: the posture summary line + the report's summary
/// object + the stderr diagnostics passthrough.
#[test]
fn lint_human_render() {
    let tool_dir = tempfile::tempdir().expect("tool dir");
    fake_tool(tool_dir.path(), "human");
    // stderr rides via the tool's own printf? — the shared fake_tool
    // prints only stdout; assert the summary + count lines.
    let (_config_dir, config) = isolated_config();
    std::fs::write(&config, "active = \"dev\"\n").expect("a loadable config");

    let out = ign_lint(tool_dir.path().to_str().unwrap(), REPORT, &["lint", "src"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    snapbox::Assert::new().action_env("SNAPSHOTS").eq(
        stdout_for_golden(&out),
        snapbox::str![[r#"
lint: 1 issue(s), child exit 1
summary: {"errors":1}
"#]],
    );
}

/// The strict passthrough BEHAVIOR (no golden — the exit varies by
/// child; the README documents): the same exit-1 child under
/// `--strict` makes the PROCESS exit 1 with the envelope still
/// printed on stdout — the one sanctioned success-path exit
/// exception.
#[test]
fn lint_strict_passthrough_exits_with_the_child_code() {
    let tool_dir = tempfile::tempdir().expect("tool dir");
    fake_tool(tool_dir.path(), "strict");
    let (_config_dir, config) = isolated_config();
    std::fs::write(&config, "active = \"dev\"\n").expect("a loadable config");

    let out = ign_lint(
        tool_dir.path().to_str().unwrap(),
        REPORT,
        &["lint", "src", "--strict", "--compact"],
    );
    assert_eq!(out.status.code(), Some(1), "the child's code, literally");
    let stdout = stdout_for_golden(&out);
    assert!(
        stdout.starts_with(r#"{"ok":true,"profile":null,"data":{"ran":true"#),
        "the envelope still renders BEFORE the exit: {stdout}"
    );
}

/// The `--` passthrough rides verbatim after the mapped args (the
/// fake tool's recorded argv is the proof).
#[test]
fn lint_passthrough_args_ride_verbatim() {
    let tool_dir = tempfile::tempdir().expect("tool dir");
    fake_tool(tool_dir.path(), "pass");
    let (_config_dir, config) = isolated_config();
    std::fs::write(&config, "active = \"dev\"\n").expect("a loadable config");

    let out = ign_lint(
        tool_dir.path().to_str().unwrap(),
        REPORT,
        &[
            "lint",
            "a",
            "b",
            "--",
            "--profile",
            "perspective",
            "--fail-on",
            "warning",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let argv = std::fs::read_to_string(tool_dir.path().join("pass-argv.txt")).expect("argv");
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        vec![
            "--report-format",
            "json",
            "--target",
            "a",
            "--target",
            "b",
            "--profile",
            "perspective",
            "--fail-on",
            "warning",
        ],
        "paths map to --target pairs; extras ride verbatim after --"
    );
}
