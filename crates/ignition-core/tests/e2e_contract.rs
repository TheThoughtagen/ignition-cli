//! Contract tests for `ign e2e` (QUICK-tg4).
//!
//! The toolchain rows are driven by a STUB PATH: a temp directory
//! holding executable shell scripts named `node` / `npm` / `npx` that
//! echo a version and record their argv. Nothing here needs a real
//! Node install, which is the point — CI, a container, and a developer
//! laptop must all produce the same rows.
//!
//! The environment is INJECTED (D10) rather than set on the process:
//! `std::env::set_var` is `unsafe` in edition 2024 and races under the
//! workspace's parallel test runs.

use std::path::{Path, PathBuf};

use ignition_core::actions::doctor::CheckStatus;
use ignition_core::actions::e2e::{E2eDoctorOptions, E2eEnv, e2e_doctor};

mod common;

// ---------------------------------------------------------------------------
// The stub-PATH harness.
// ---------------------------------------------------------------------------

/// Write an executable shell script that echoes `stdout_line` and
/// appends its argv to `<dir>/<name>.argv`.
#[cfg(unix)]
fn stub_tool(dir: &Path, name: &str, stdout_line: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    let recorder = dir.join(format!("{name}.argv"));
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '%s\\n' '{stdout_line}'\n",
        recorder.display()
    );
    std::fs::write(&path, body).expect("the stub writes");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("the stub is executable");
}

/// The recorded argv lines for a stub tool (empty when never invoked).
#[cfg(unix)]
fn recorded_argv(dir: &Path, name: &str) -> Vec<String> {
    std::fs::read_to_string(dir.join(format!("{name}.argv")))
        .map(|body| body.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// An `E2eEnv` whose PATH is exactly `dir` and whose browsers cache is
/// `cache` — no process state consulted.
fn env_with(dir: &Path, cache: Option<PathBuf>) -> E2eEnv {
    E2eEnv {
        path: Some(dir.as_os_str().to_os_string()),
        browsers_cache: cache,
    }
}

fn opts_for(dir: &Path) -> E2eDoctorOptions {
    E2eDoctorOptions {
        dir: dir.to_path_buf(),
        project: None,
    }
}

/// Find a row by name (the row set is a contract — a missing name is a
/// test failure, not an Option the test papers over).
fn row<'a>(
    result: &'a ignition_core::actions::e2e::E2eDoctorResult,
    name: &str,
) -> &'a ignition_core::actions::doctor::CheckResult {
    result
        .checks
        .iter()
        .find(|check| check.name == name)
        .unwrap_or_else(|| panic!("no {name:?} row in {:?}", result.checks))
}

// ---------------------------------------------------------------------------
// Doctor — the toolchain rows.
// ---------------------------------------------------------------------------

/// A complete toolchain: node 22 + npm on PATH, the runner installed,
/// a chromium in the cache.
#[cfg(unix)]
#[tokio::test]
async fn doctor_reports_ok_rows_for_a_complete_toolchain() {
    let tools = tempfile::tempdir().unwrap();
    stub_tool(tools.path(), "node", "v22.11.0");
    stub_tool(tools.path(), "npm", "10.9.0");

    let scaffold = tempfile::tempdir().unwrap();
    let marker = scaffold.path().join("node_modules/@playwright/test");
    std::fs::create_dir_all(&marker).unwrap();
    std::fs::write(marker.join("package.json"), "{}").unwrap();

    let cache = tempfile::tempdir().unwrap();
    std::fs::create_dir(cache.path().join("chromium-1148")).unwrap();

    let env = env_with(tools.path(), Some(cache.path().to_path_buf()));
    let result = e2e_doctor(None, &env, &opts_for(scaffold.path()))
        .await
        .expect("the diagnosis completes");

    assert_eq!(result.checks.len(), 6, "six rows, always");
    assert_eq!(
        result
            .checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "node",
            "npm",
            "playwright",
            "chromium",
            "testing-bundle",
            "trial"
        ],
        "the row ORDER is contract"
    );
    assert_eq!(row(&result, "node").status, CheckStatus::Ok);
    assert!(
        row(&result, "node").detail.contains("v22.11.0"),
        "the node row names the version: {}",
        row(&result, "node").detail
    );
    assert_eq!(row(&result, "npm").status, CheckStatus::Ok);
    assert_eq!(row(&result, "playwright").status, CheckStatus::Ok);
    assert_eq!(row(&result, "chromium").status, CheckStatus::Ok);

    // The version probe used an ARG VECTOR, not a shell string.
    let argv = recorded_argv(tools.path(), "node");
    assert_eq!(argv, vec!["--version"], "node was asked for its version");
}

/// Node 18 is below the floor: `fail`, with the install hint naming the
/// minimum and the install routes.
#[cfg(unix)]
#[tokio::test]
async fn old_node_fails_with_the_install_hint() {
    let tools = tempfile::tempdir().unwrap();
    stub_tool(tools.path(), "node", "v18.20.4");
    stub_tool(tools.path(), "npm", "9.0.0");
    let scaffold = tempfile::tempdir().unwrap();

    let env = env_with(tools.path(), None);
    let result = e2e_doctor(None, &env, &opts_for(scaffold.path()))
        .await
        .expect("the diagnosis completes");

    let node = row(&result, "node");
    assert_eq!(node.status, CheckStatus::Fail, "18 < 20");
    assert!(node.detail.contains("18"), "the detail names what it found");
    let hint = node.hint.as_deref().expect("a fail row carries a hint");
    assert!(hint.contains("20"), "the hint names the minimum: {hint}");
    assert!(
        hint.contains("nodejs.org") && hint.contains("brew") && hint.contains("nvm"),
        "the hint names the three install routes: {hint}"
    );
    assert_eq!(
        hint,
        ignition_core::actions::e2e::NODE_INSTALL_HINT,
        "the hint IS the shared const — `e2e init`'s refusal reuses it byte-identically"
    );
}

/// THE exit-contract proof: a host with no tools at all still produces
/// a complete diagnosis and an `Ok` result.
#[tokio::test]
async fn empty_path_still_exits_zero_with_fail_rows() {
    let empty = tempfile::tempdir().unwrap();
    let scaffold = tempfile::tempdir().unwrap();

    let env = env_with(empty.path(), None);
    let result = e2e_doctor(None, &env, &opts_for(scaffold.path()))
        .await
        .expect("a toolless host STILL completes the diagnosis — exit 0");

    assert_eq!(result.checks.len(), 6);
    assert_eq!(row(&result, "node").status, CheckStatus::Fail);
    assert_eq!(row(&result, "npm").status, CheckStatus::Fail);
    assert!(row(&result, "node").hint.is_some());
    assert!(row(&result, "npm").hint.is_some());
}

/// The two filesystem rows read the filesystem and nothing else.
#[tokio::test]
async fn playwright_and_chromium_rows_read_the_filesystem() {
    let tools = tempfile::tempdir().unwrap();
    let bare_scaffold = tempfile::tempdir().unwrap();
    let empty_cache = tempfile::tempdir().unwrap();

    // Nothing installed: both warn (never fail — a missing browser is
    // one command away, not a broken host).
    let env = env_with(tools.path(), Some(empty_cache.path().to_path_buf()));
    let result = e2e_doctor(None, &env, &opts_for(bare_scaffold.path()))
        .await
        .unwrap();
    assert_eq!(row(&result, "playwright").status, CheckStatus::Warn);
    assert!(
        row(&result, "playwright")
            .hint
            .as_deref()
            .unwrap()
            .contains("e2e init"),
        "the playwright hint names the init verb"
    );
    assert_eq!(row(&result, "chromium").status, CheckStatus::Warn);
    assert!(
        row(&result, "chromium")
            .hint
            .as_deref()
            .unwrap()
            .contains("--browsers"),
        "the chromium hint names the browsers flag"
    );

    // Installed: both ok.
    let ready = tempfile::tempdir().unwrap();
    let marker = ready.path().join("node_modules/@playwright/test");
    std::fs::create_dir_all(&marker).unwrap();
    std::fs::write(marker.join("package.json"), "{}").unwrap();
    let cache = tempfile::tempdir().unwrap();
    std::fs::create_dir(cache.path().join("chromium_headless_shell-1148")).unwrap();

    let env = env_with(tools.path(), Some(cache.path().to_path_buf()));
    let result = e2e_doctor(None, &env, &opts_for(ready.path()))
        .await
        .unwrap();
    assert_eq!(row(&result, "playwright").status, CheckStatus::Ok);
    assert_eq!(
        row(&result, "chromium").status,
        CheckStatus::Ok,
        "the headless shell counts as a chromium"
    );
}

/// No project and no client: both gateway rows `skip`, and the bundle
/// row's detail names the missing argument.
#[tokio::test]
async fn gateway_rows_skip_without_a_project_or_client() {
    let tools = tempfile::tempdir().unwrap();
    let scaffold = tempfile::tempdir().unwrap();

    let env = env_with(tools.path(), None);
    let result = e2e_doctor(None, &env, &opts_for(scaffold.path()))
        .await
        .unwrap();

    let bundle = row(&result, "testing-bundle");
    assert_eq!(bundle.status, CheckStatus::Skip);
    assert!(
        bundle.detail.contains("--project"),
        "the skip names the argument that would enable it: {}",
        bundle.detail
    );
    assert_eq!(row(&result, "trial").status, CheckStatus::Skip);
    assert!(
        row(&result, "trial").detail.contains("profile"),
        "the trial skip names what is missing"
    );
}

// ---------------------------------------------------------------------------
// Doctor — the gateway rows (wiremock-backed).
// ---------------------------------------------------------------------------

/// An undeployed bundle is a `fail` ROW — and the verb still returns
/// `Ok`. (405 is the live-proven 8.3 WebDev absent marker.)
#[tokio::test]
async fn bundle_row_fails_when_the_testing_route_is_absent() {
    let mock = common::IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path_regex(r".*/testing/run$"))
        .respond_with(wiremock::ResponseTemplate::new(405))
        .mount(&mock.server)
        .await;
    // The trial reads answer so the trial row does not dominate.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/trial"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "licenseMode": "Trial", "trialState": "AllInDemo",
                "trialSecondsLeft": 7199, "expired": false,
                "emergency": false, "emergencySecondsLeft": 0,
                "development": false, "developmentSecondsLeft": 0
            })),
        )
        .mount(&mock.server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/overview/banners"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "banners": [] })),
        )
        .mount(&mock.server)
        .await;

    let api = ignition_core::client::ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let tools = tempfile::tempdir().unwrap();
    let scaffold = tempfile::tempdir().unwrap();
    let env = env_with(tools.path(), None);
    let opts = E2eDoctorOptions {
        dir: scaffold.path().to_path_buf(),
        project: Some("Flux".into()),
    };

    let result = e2e_doctor(Some(&api), &env, &opts)
        .await
        .expect("an absent bundle is DATA — the diagnosis still completes");

    let bundle = row(&result, "testing-bundle");
    assert_eq!(bundle.status, CheckStatus::Fail);
    assert!(
        bundle.detail.contains("Flux"),
        "the detail names the project: {}",
        bundle.detail
    );
    let hint = bundle.hint.as_deref().expect("an actionable hint");
    assert!(
        hint.contains("--with-testing") || hint.contains("--testing"),
        "the hint names the deploy flag: {hint}"
    );
    assert_eq!(row(&result, "trial").status, CheckStatus::Ok);
}

/// An expired trial warns and names the reset verb.
#[tokio::test]
async fn trial_row_warns_when_the_trial_is_expired() {
    let mock = common::IgnitionMock::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/trial"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "licenseMode": "Trial", "trialState": "AllInDemo",
                "trialSecondsLeft": 0, "expired": true,
                "emergency": false, "emergencySecondsLeft": 0,
                "development": false, "developmentSecondsLeft": 0
            })),
        )
        .mount(&mock.server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/data/api/v1/overview/banners"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "banners": [] })),
        )
        .mount(&mock.server)
        .await;

    let api = ignition_core::client::ReqwestGatewayApi::for_tests(&mock.uri(), None);
    let tools = tempfile::tempdir().unwrap();
    let scaffold = tempfile::tempdir().unwrap();
    let env = env_with(tools.path(), None);

    let result = e2e_doctor(Some(&api), &env, &opts_for(scaffold.path()))
        .await
        .unwrap();

    let trial = row(&result, "trial");
    assert_eq!(
        trial.status,
        CheckStatus::Warn,
        "expired warns, never fails"
    );
    let hint = trial.hint.as_deref().expect("a hint");
    assert!(
        hint.contains("rig trial reset"),
        "the hint names the reset verb: {hint}"
    );
    // No --project: the bundle row skips even with a live client.
    assert_eq!(row(&result, "testing-bundle").status, CheckStatus::Skip);
}

// ---------------------------------------------------------------------------
// PATH discovery — the pure unit, including the non-unix shim branch.
// ---------------------------------------------------------------------------

/// `find_on_path_in` is the ONE discovery implementation now (it backs
/// both `ign lint` and the e2e rows), so its behavior is pinned here
/// directly rather than only through the doctor rows.
#[cfg(unix)]
#[test]
fn find_on_path_in_walks_the_injected_path_in_order() {
    use ignition_core::actions::lint::find_on_path_in;

    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    stub_tool(first.path(), "node", "v22.0.0");
    stub_tool(second.path(), "node", "v20.0.0");

    let joined = std::env::join_paths([first.path(), second.path()]).unwrap();
    let found = find_on_path_in(&joined, "node").expect("node is discoverable");
    assert_eq!(
        found,
        first.path().join("node"),
        "the FIRST PATH entry wins"
    );

    assert!(
        find_on_path_in(&joined, "definitely-not-a-real-tool").is_none(),
        "an absent tool is None, not a panic"
    );

    // A non-executable file with the right name is NOT a match.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("npm"), "not executable").unwrap();
    let only = std::env::join_paths([dir.path()]).unwrap();
    assert!(
        find_on_path_in(&only, "npm").is_none(),
        "a non-executable file is not a tool"
    );
}

// ---------------------------------------------------------------------------
// Init — the scaffold write.
// ---------------------------------------------------------------------------

use ignition_core::actions::e2e::{E2eFileStatus, E2eInitOptions, e2e_init, e2e_init_preview};

fn init_opts(dir: &Path) -> E2eInitOptions {
    E2eInitOptions {
        dir: dir.to_path_buf(),
        project: "ign-cli".into(),
        run_project: "Flux".into(),
        gateway_url: "http://localhost:9088".into(),
        browsers: false,
    }
}

/// A fresh directory gets every member, on disk, with the expected
/// relative layout and no surviving markers.
#[cfg(unix)]
#[tokio::test]
async fn init_writes_the_full_scaffold() {
    let tools = tempfile::tempdir().unwrap();
    stub_tool(tools.path(), "node", "v22.11.0");
    stub_tool(tools.path(), "npm", "10.9.0");
    let target = tempfile::tempdir().unwrap();
    let dir = target.path().join("e2e");

    let env = env_with(tools.path(), None);
    let result = e2e_init(&env, &init_opts(&dir)).await.expect("init lands");

    assert_eq!(result.files.len(), 7, "seven members");
    assert!(
        result
            .files
            .iter()
            .all(|file| file.status == E2eFileStatus::Written),
        "a fresh directory writes every member: {:?}",
        result.files
    );

    for member in [
        "package.json",
        "playwright.config.mjs",
        "global-setup.mjs",
        "lib/gateway.mjs",
        "tests/example.spec.mjs",
        ".gitignore",
        "README.md",
    ] {
        let path = dir.join(member);
        assert!(path.is_file(), "{member} is on disk at {}", path.display());
        let body = std::fs::read_to_string(&path).unwrap();
        for marker in [
            "__IGN_PROJECT__",
            "__IGN_RUN_PROJECT__",
            "__IGN_GATEWAY_URL__",
        ] {
            assert!(
                !body.contains(marker),
                "{member} shipped with an unsubstituted {marker}"
            );
        }
    }

    // No member reached disk with the template suffix.
    assert!(
        !dir.join("package.json.tmpl").exists() && !dir.join("gitignore.tmpl").exists(),
        "a .tmpl suffix reached disk"
    );
    // The substituted values actually landed.
    let helper = std::fs::read_to_string(dir.join("lib/gateway.mjs")).unwrap();
    assert!(helper.contains("/system/webdev/ign-cli/testing/tags"));
    let spec = std::fs::read_to_string(dir.join("tests/example.spec.mjs")).unwrap();
    assert!(spec.contains("Flux"), "the run project landed in the spec");

    // The npm leg ran with an arg vector.
    let argv = recorded_argv(tools.path(), "npm");
    assert_eq!(argv, vec!["install"], "npm install ran in the target");
    assert!(result.npm_install.as_ref().unwrap().ran);
    assert!(result.browsers.is_none(), "no --browsers, null field");
}

/// Idempotence: a second init reports every member skipped and leaves
/// every byte alone. This is what makes re-running safe over a suite
/// someone has edited.
#[cfg(unix)]
#[tokio::test]
async fn second_init_skips_every_member() {
    let tools = tempfile::tempdir().unwrap();
    stub_tool(tools.path(), "node", "v22.11.0");
    stub_tool(tools.path(), "npm", "10.9.0");
    let target = tempfile::tempdir().unwrap();
    let dir = target.path().join("e2e");
    let env = env_with(tools.path(), None);

    e2e_init(&env, &init_opts(&dir)).await.expect("first init");

    // A user edit that must survive.
    let spec = dir.join("tests/example.spec.mjs");
    std::fs::write(&spec, "// MY OWN TESTS\n").unwrap();
    let before: Vec<(String, String)> = ["package.json", "README.md", "tests/example.spec.mjs"]
        .iter()
        .map(|m| {
            (
                (*m).to_string(),
                std::fs::read_to_string(dir.join(m)).unwrap(),
            )
        })
        .collect();

    let second = e2e_init(&env, &init_opts(&dir)).await.expect("second init");

    assert!(
        second
            .files
            .iter()
            .all(|file| file.status == E2eFileStatus::Skipped),
        "every member skips on a re-init: {:?}",
        second.files
    );
    for (member, body) in before {
        assert_eq!(
            std::fs::read_to_string(dir.join(&member)).unwrap(),
            body,
            "{member} was modified by the second init"
        );
    }
    assert_eq!(
        std::fs::read_to_string(&spec).unwrap(),
        "// MY OWN TESTS\n",
        "the user's own test file survived"
    );
}

/// A non-empty directory that is not a scaffold is refused BEFORE any
/// write, and the refusal names something it found.
#[cfg(unix)]
#[tokio::test]
async fn foreign_directory_refuses_before_any_write() {
    let tools = tempfile::tempdir().unwrap();
    stub_tool(tools.path(), "node", "v22.11.0");
    stub_tool(tools.path(), "npm", "10.9.0");
    let target = tempfile::tempdir().unwrap();
    std::fs::write(target.path().join("important-notes.txt"), "do not delete").unwrap();
    std::fs::write(target.path().join("budget.xlsx"), "x").unwrap();

    let env = env_with(tools.path(), None);
    let err = e2e_init(&env, &init_opts(target.path()))
        .await
        .expect_err("a foreign directory refuses");

    assert_eq!(err.exit_code(), 2, "usage class");
    assert_eq!(err.code(), "invalid_input");
    let message = err.to_string();
    assert!(
        message.contains("budget.xlsx") || message.contains("important-notes.txt"),
        "the refusal names what it found: {message}"
    );

    // NOTHING was written and NOTHING was spawned.
    assert!(!target.path().join("package.json").exists());
    assert!(!target.path().join("playwright.config.mjs").exists());
    assert!(
        recorded_argv(tools.path(), "npm").is_empty(),
        "no installer was spawned"
    );
}

/// The preview lists every member and both command lines, and writes
/// nothing. (The confirmation refusal itself is `require_confirmation`'s
/// and is already pinned by the CLI suite.)
#[cfg(unix)]
#[tokio::test]
async fn preview_lists_every_member_and_both_commands_and_writes_nothing() {
    let tools = tempfile::tempdir().unwrap();
    stub_tool(tools.path(), "node", "v22.11.0");
    stub_tool(tools.path(), "npm", "10.9.0");
    let target = tempfile::tempdir().unwrap();
    let dir = target.path().join("e2e");

    let env = env_with(tools.path(), None);
    let mut opts = init_opts(&dir);
    opts.browsers = true;

    let preview = e2e_init_preview(&env, &opts).expect("the preview computes");

    for member in [
        "package.json",
        "playwright.config.mjs",
        "global-setup.mjs",
        "lib/gateway.mjs",
        "tests/example.spec.mjs",
        ".gitignore",
        "README.md",
    ] {
        assert!(preview.contains(member), "the preview lists {member}");
    }
    assert!(preview.contains("npm install"), "the npm command line");
    assert!(
        preview.contains("playwright install") && preview.contains("chromium"),
        "the browser command line: {preview}"
    );
    assert!(preview.contains("ign-cli") && preview.contains("Flux"));

    // `require_confirmation` concatenates " is destructive; rerun with
    // --yes to confirm" straight onto this string, so the preview must
    // END on a noun phrase. Ending on a command line produced
    // "npm install is destructive; rerun with --yes to confirm" — a
    // sentence about npm rather than about the scaffold write.
    assert!(
        preview.ends_with("this scaffold write"),
        "the preview must end on a noun phrase the confirmation suffix can \
         attach to; it ends with: {:?}",
        &preview[preview.len().saturating_sub(60)..]
    );

    // The preview is a DRY RUN: nothing on disk, nothing spawned.
    assert!(!dir.exists(), "the preview created no directory");
    assert!(
        recorded_argv(tools.path(), "npm").is_empty(),
        "the preview spawned nothing"
    );
}

/// A host with no node refuses exit 6 with the doctor's install hint —
/// the same string, byte for byte.
#[tokio::test]
async fn absent_node_refuses_with_the_install_hint() {
    let empty = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let dir = target.path().join("e2e");
    let env = env_with(empty.path(), None);

    let err = e2e_init(&env, &init_opts(&dir))
        .await
        .expect_err("no node refuses");

    assert_eq!(
        err.exit_code(),
        6,
        "target-state class, NOT the rig class 7"
    );
    assert_eq!(err.code(), "node_tool_absent");
    assert_eq!(
        err.hint().expect("a hint"),
        ignition_core::actions::e2e::NODE_INSTALL_HINT,
        "the refusal reuses the doctor's node hint byte-identically"
    );
    assert!(!dir.exists(), "nothing was written");

    // The preview refuses identically (the gate order is the same).
    let preview_err =
        e2e_init_preview(&env, &init_opts(&dir)).expect_err("the preview refuses too");
    assert_eq!(preview_err.code(), "node_tool_absent");
}
