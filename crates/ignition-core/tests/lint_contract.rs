//! Contract for the `ign lint` delegation (07-04, INTR-02) — PATH
//! discovery + tokio::process arg-vector spawn + the doctor
//! posture, proven against FIXTURE EXECUTABLES on a controlled PATH
//! (a temp dir + a serialized env guard — the tests never touch the
//! machine's real PATH in parallel).
//!
//! THE crown pins:
//! - **the posture**: a child exiting 1 with a JSON report still
//!   exits Ok from the action — findings + `child_exit_code` + the
//!   parsed report ride as DATA;
//! - **the strict flag**: the result carries the passthrough exit
//!   (`strict_exit_code`), masked to the shell-signal range;
//! - **the absent tool**: `lint_tool_absent` (exit 6) with the
//!   install hint, ZERO spawns (an empty PATH has nothing to find);
//! - **the arg vector**: the fake tool RECORDS its argv; the
//!   delegation proves `--report-format json --target <path>` rides
//!   EXACTLY (never a shell string).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ignition_core::actions::lint::{find_lint_tool, lint_run};
use ignition_core::error::CoreError;

/// PATH-mutating tests serialize on ONE mutex, HELD for the guard's
/// whole lifetime (env is process-global and lib tests run in
/// parallel threads; edition 2024 makes `set_var` unsafe for exactly
/// this reason — under this lock it is sound). PATH restores on drop;
/// LINT_ARGV_FILE removals happen inside each test while the lock is
/// held.
static PATH_LOCK: Mutex<()> = Mutex::new(());

struct PathGuard(
    std::ffi::OsString,
    Option<std::sync::MutexGuard<'static, ()>>,
);

impl PathGuard {
    fn set(dirs: &[&Path]) -> Self {
        let guard = PATH_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let original: std::ffi::OsString = std::env::var_os("PATH").unwrap_or_default();
        let joined = std::env::join_paths(dirs).expect("join test PATH");
        unsafe { std::env::set_var("PATH", joined) };
        Self(original, Some(guard))
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        unsafe { std::env::set_var("PATH", &self.0) };
        drop(self.1.take()); // release AFTER PATH restored
    }
}

/// Write an executable fake `ignition-lint`. SH BUILTINS ONLY — the
/// child inherits the ISOLATED PATH (the tempdir alone), so external
/// binaries like `cat` would not resolve; the payload rides env vars
/// the test sets under the PATH lock. Behavior: record argv to
/// `$LINT_ARGV_FILE` (one arg per line), print `$LINT_STDOUT`,
/// print `$LINT_STDERR` on stderr, exit `code`.
fn fake_tool(dir: &Path, code: i32) -> PathBuf {
    let tool = dir.join("ignition-lint");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$LINT_ARGV_FILE\"\nprintf '%s' \"$LINT_STDOUT\"\nprintf '%s' \"$LINT_STDERR\" >&2\nexit {code}\n"
    );
    std::fs::write(&tool, script).expect("write fake tool");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tool).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tool, perms).expect("chmod");
    }
    tool
}

/// The env payload triplet under the PATH lock: argv file, stdout
/// text, stderr text (restored on drop).
struct ToolEnv;

impl ToolEnv {
    fn set(dir: &Path, stdout_text: &str, stderr_text: &str) {
        unsafe {
            std::env::set_var("LINT_ARGV_FILE", dir.join("argv.txt"));
            std::env::set_var("LINT_STDOUT", stdout_text);
            std::env::set_var("LINT_STDERR", stderr_text);
        }
    }
}

/// A JSON report shaped like ignition-lint's `--report-format json`.
const REPORT: &str = concat!(
    r#"{"issues":[{"severity":"error","code":"P001","message":"bad name","file_path":"views/Dashboard/view.json","line_number":3},"#,
    r#"{"severity":"warning","code":"N010","message":"worse name","file_path":"views/Dashboard/view.json","line_number":9}],"#,
    r#""summary":{"errors":1,"warnings":1}}"#
);

/// The recorded argv (the fake tool wrote one line per arg).
fn recorded_argv(dir: &Path) -> Vec<String> {
    let raw = std::fs::read_to_string(
        std::env::var("LINT_ARGV_FILE")
            .map(PathBuf::from)
            .expect("harness sets LINT_ARGV_FILE"),
    )
    .expect("argv file");
    let _ = dir; // the argv file lives in the SAME tempdir lifecycle
    raw.lines().map(str::to_string).collect()
}

/// (a)+(d) THE posture + arg-vector proof: a tool exiting 1 with a
/// JSON report → the action returns Ok, the data carries
/// child_exit_code 1 + the parsed issues count + the report, and the
/// child saw EXACTLY `--report-format json --target <path>` (+ the
/// passthrough extras, verbatim, after them).
#[tokio::test]
async fn findings_ride_as_data_with_the_exact_arg_vector() {
    let dir = tempfile::tempdir().expect("tempdir");
    fake_tool(dir.path(), 1);
    let _guard = PathGuard::set(&[dir.path()]);
    ToolEnv::set(dir.path(), REPORT, "⚠ diagnostic line");

    let result = lint_run(
        &["src/views".to_string()],
        false,
        &["--profile".to_string(), "perspective".to_string()],
    )
    .await
    .expect("the doctor posture: the child RAN, the action succeeds");
    assert!(result.ran);
    assert_eq!(result.child_exit_code, Some(1));
    assert_eq!(result.issues_found, 2);
    let report = result.report.as_ref().expect("report parsed");
    assert_eq!(report["summary"]["errors"], serde_json::json!(1));
    assert_eq!(
        result.stdout.trim(),
        REPORT,
        "stdout rides verbatim alongside the parsed report"
    );
    assert!(result.stderr_preview.contains("diagnostic line"));
    assert_eq!(result.strict_exit_code(), None, "default posture");

    // (d) THE arg vector — recorded by the fake tool itself.
    assert_eq!(
        recorded_argv(dir.path()),
        vec![
            "--report-format",
            "json",
            "--target",
            "src/views",
            "--profile",
            "perspective",
        ],
        "arg-vector spawn, extras verbatim — never a shell string"
    );
    unsafe {
        std::env::remove_var("LINT_ARGV_FILE");
        std::env::remove_var("LINT_STDOUT");
        std::env::remove_var("LINT_STDERR");
    }
}

/// (b) `--strict` arms the passthrough: the same child run carries
/// `strict_exit_code() == Some(1)` (the binary decides the actual
/// process exit AFTER the envelope renders).
#[tokio::test]
async fn strict_mode_carries_the_child_exit_passthrough() {
    let dir = tempfile::tempdir().expect("tempdir");
    fake_tool(dir.path(), 1);
    let _guard = PathGuard::set(&[dir.path()]);
    ToolEnv::set(dir.path(), REPORT, "");

    let result = lint_run(&["p".to_string()], true, &[]).await.expect("ran");
    assert_eq!(result.child_exit_code, Some(1));
    assert_eq!(result.strict_exit_code(), Some(1), "the strict passthrough");

    // A CLEAN child in strict mode: passthrough 0 (the SUCCESS path).
    fake_tool(dir.path(), 0);
    ToolEnv::set(dir.path(), r#"{"issues":[],"summary":{"errors":0}}"#, "");
    let result = lint_run(&["p".to_string()], true, &[]).await.expect("ran");
    assert_eq!(result.child_exit_code, Some(0));
    assert_eq!(result.strict_exit_code(), Some(0));
    assert_eq!(result.issues_found, 0);
    unsafe {
        std::env::remove_var("LINT_ARGV_FILE");
        std::env::remove_var("LINT_STDOUT");
        std::env::remove_var("LINT_STDERR");
    }
}

/// (c) THE absent-tool refusal: an empty PATH discovers nothing →
/// `lint_tool_absent` (exit 6) with the install hint — and zero
/// spawns are possible (there is no executable to run).
#[tokio::test]
async fn absent_tool_refuses_with_the_install_hint() {
    let empty = tempfile::tempdir().expect("tempdir");
    let _guard = PathGuard::set(&[empty.path()]);
    assert!(find_lint_tool().is_none(), "nothing discoverable");

    let err = lint_run(&["p".to_string()], false, &[])
        .await
        .expect_err("absent refuses");
    assert_eq!(err.code(), "lint_tool_absent");
    assert_eq!(err.exit_code(), 6);
    let hint = err.hint().expect("hint required");
    assert!(
        hint.contains("uv tool install ignition-lint-toolkit"),
        "hint names the install command: {hint}"
    );
    assert!(
        hint.contains("github.com/TheThoughtagen/ignition-lint"),
        "hint names the repo: {hint}"
    );
}

/// An unparseable report degrades honestly: `report: null`,
/// `issues_found: 0`, stdout still verbatim — the child RAN, the
/// posture holds.
#[tokio::test]
async fn unparseable_report_degrades_to_null_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    fake_tool(dir.path(), 2);
    let _guard = PathGuard::set(&[dir.path()]);
    ToolEnv::set(dir.path(), "not json at all — usage error text", "boom");

    let result = lint_run(&["missing-dir".to_string()], false, &[])
        .await
        .expect("the child RAN — exit 2 is the child's usage error, data not failure");
    assert_eq!(result.child_exit_code, Some(2));
    assert_eq!(result.issues_found, 0);
    assert!(result.report.is_none());
    assert!(result.stdout.contains("usage error text"));
    unsafe {
        std::env::remove_var("LINT_ARGV_FILE");
        std::env::remove_var("LINT_STDOUT");
        std::env::remove_var("LINT_STDERR");
    }
}

/// A LONG stderr preview truncates (the cap keeps the result shape
/// bounded; the truncation marker is honest).
#[tokio::test]
async fn stderr_preview_truncates() {
    let dir = tempfile::tempdir().expect("tempdir");
    fake_tool(dir.path(), 1);
    let long_diag = "x".repeat(10_000);
    let _guard = PathGuard::set(&[dir.path()]);
    ToolEnv::set(dir.path(), REPORT, &long_diag);

    let result = lint_run(&["p".to_string()], false, &[]).await.expect("ran");
    assert!(result.stderr_preview.len() < long_diag.len());
    assert!(result.stderr_preview.contains("(truncated)"));
    unsafe {
        std::env::remove_var("LINT_ARGV_FILE");
        std::env::remove_var("LINT_STDOUT");
        std::env::remove_var("LINT_STDERR");
    }
}

/// Discovery order: the FIRST `ignition-lint` on PATH wins (a
/// decoy later on PATH never runs — the recorded argv proves which
/// one did).
#[tokio::test]
async fn discovery_takes_the_first_tool_on_path() {
    let first = tempfile::tempdir().expect("first");
    let second = tempfile::tempdir().expect("second");
    // Both tools report JSON; the FIRST exits 7 (a distinctive code)
    // so the child_exit_code identifies the winner.
    fake_tool(first.path(), 7);
    fake_tool(second.path(), 9);
    let _guard = PathGuard::set(&[first.path(), second.path()]);
    ToolEnv::set(first.path(), REPORT, "");

    let result = lint_run(&["p".to_string()], false, &[]).await.expect("ran");
    assert_eq!(result.child_exit_code, Some(7), "the FIRST PATH entry ran");
    assert!(
        result
            .tool
            .starts_with(first.path().display().to_string().as_str())
    );
    unsafe {
        std::env::remove_var("LINT_ARGV_FILE");
        std::env::remove_var("LINT_STDOUT");
        std::env::remove_var("LINT_STDERR");
    }
}

/// The discovery skip rules: a NON-executable file named
/// ignition-lint does not count (the executable-bit check).
#[cfg(unix)]
#[tokio::test]
async fn non_executable_candidate_is_skipped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let decoy = dir.path().join("ignition-lint");
    std::fs::write(&decoy, "#!/bin/sh\nexit 0\n").expect("write decoy");
    // mode 0o644 — a file, not an executable.
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&decoy).unwrap().permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&decoy, perms).unwrap();
    let _guard = PathGuard::set(&[dir.path()]);
    assert!(
        find_lint_tool().is_none(),
        "a non-executable file is not a tool"
    );

    let err = lint_run(&["p".to_string()], false, &[])
        .await
        .expect_err("still absent");
    assert!(matches!(err, CoreError::LintToolAbsent), "{err}");
}
