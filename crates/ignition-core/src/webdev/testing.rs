//! The embedded TESTING bundle (ADOPT-04, the D2 landing) — the Jython
//! test framework + its two WebDev routes, ported OUT of
//! agentic-ignition-tooling's `scaffold-testing.sh` (1 806-line shell
//! scaffolder) into the repo as data — the `ROUTE_FILES` precedent,
//! byte-identical semantics, no shell, no Windows problem.
//!
//! ## What ships (20 members, one import)
//!
//! - `ignition/script-python/resources/testing/<module>/` — the five
//!   framework modules (runner, assertions, decorators, helpers,
//!   reporter) + `__tests__/` (the permanent smoke sentinel — the
//!   empty-suite-trap answer: discover ALWAYS has ≥1 module and
//!   `run_all` proves the chain live).
//! - `com.inductiveautomation.webdev/resources/testing/{run,tags}/` —
//!   the WebDev endpoints (`run` = discover/execute, `tags` =
//!   e2e tag fixtures; `tags` is require-auth, `run` is open at the
//!   WebDev layer like the cli routes).
//!
//! ## The relocation, retired
//!
//! The scaffolder wrote the 8.1 layout (`ignition/script-python/
//! testing/…` — modules directly under the collection) and every 8.3
//! deploy needed the relocation patch. The bundle here IS the 8.3
//! resource-folder layout (`…/script-python/resources/testing/…` —
//! live-verified on-disk spelling, 8.3.6 rig), and the runner's
//! discovery walk bases its dotted module paths on the `resources`
//! container when present (8.3) falling back to the bare collection
//! (8.1) — one walk, both layouts, no per-deploy patch.
//!
//! ## Markers (substituted at pack time, the scriptExec pattern)
//!
//! - `__IGN_CLI_PROJECT__` — the deploy project name (the runner's
//!   fallback constant). Exactly 1 site — the pack substitution
//!   replaces it; a marker-count test pins the total.
//!
//! Source of truth for the port: `.planning/research/ADOPT-RESEARCH.md`
//! §D2 handoff (the manifest) + the extracted originals under
//! `webdev/testing/` (this repo now owns them; the scaffolder is
//! retired for ign workflows).

/// The testing bundle — `(zip_member_path, contents)` pairs, the
/// Designer-native 8.3 layout. Members are packed AFTER
/// [`super::ROUTE_FILES`] by
/// [`crate::client::webdev::build_deploy_zip`] when testing rides the
/// deploy.
pub const TESTING_FILES: &[(&str, &str)] = &[
    // --- The Jython framework (ignition/script-python) ---------------
    (
        "ignition/script-python/resources/testing/runner/code.py",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/runner/code.py"
        ),
    ),
    (
        "ignition/script-python/resources/testing/runner/resource.json",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/runner/resource.json"
        ),
    ),
    (
        "ignition/script-python/resources/testing/assertions/code.py",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/assertions/code.py"
        ),
    ),
    (
        "ignition/script-python/resources/testing/assertions/resource.json",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/assertions/resource.json"
        ),
    ),
    (
        "ignition/script-python/resources/testing/decorators/code.py",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/decorators/code.py"
        ),
    ),
    (
        "ignition/script-python/resources/testing/decorators/resource.json",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/decorators/resource.json"
        ),
    ),
    (
        "ignition/script-python/resources/testing/helpers/code.py",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/helpers/code.py"
        ),
    ),
    (
        "ignition/script-python/resources/testing/helpers/resource.json",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/helpers/resource.json"
        ),
    ),
    (
        "ignition/script-python/resources/testing/reporter/code.py",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/reporter/code.py"
        ),
    ),
    (
        "ignition/script-python/resources/testing/reporter/resource.json",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/reporter/resource.json"
        ),
    ),
    // The smoke sentinel — the empty-suite-trap answer.
    (
        "ignition/script-python/resources/testing/__tests__/code.py",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/__tests__/code.py"
        ),
    ),
    (
        "ignition/script-python/resources/testing/__tests__/resource.json",
        include_str!(
            "../../webdev/testing/ignition/script-python/resources/testing/__tests__/resource.json"
        ),
    ),
    // --- The WebDev routes --------------------------------------------
    (
        "com.inductiveautomation.webdev/resources/testing/run/doPost.py",
        include_str!(
            "../../webdev/testing/com.inductiveautomation.webdev/resources/testing/run/doPost.py"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/testing/run/config.json",
        include_str!(
            "../../webdev/testing/com.inductiveautomation.webdev/resources/testing/run/config.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/testing/run/resource.json",
        include_str!(
            "../../webdev/testing/com.inductiveautomation.webdev/resources/testing/run/resource.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/testing/tags/doPost.py",
        include_str!(
            "../../webdev/testing/com.inductiveautomation.webdev/resources/testing/tags/doPost.py"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/testing/tags/config.json",
        include_str!(
            "../../webdev/testing/com.inductiveautomation.webdev/resources/testing/tags/config.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/testing/tags/resource.json",
        include_str!(
            "../../webdev/testing/com.inductiveautomation.webdev/resources/testing/tags/resource.json"
        ),
    ),
];

/// The deploy-time project marker (the `__IGN_CLI_SECRET__` pattern).
pub(crate) const PROJECT_MARKER: &str = "__IGN_CLI_PROJECT__";

/// The total marker count across the bundle — the pack substitution
/// replaces every one; this pin makes an accidental template edit
/// (adding or removing a marker) a test failure, not a runtime
/// `PLACEHOLDER` on a gateway.
pub(crate) const PROJECT_MARKER_COUNT: usize = 1;

/// The testing bundle's route folder names (URL space:
/// `/system/webdev/<project>/testing/<route>`).
pub const TESTING_ROUTES: &[&str] = &["run", "tags"];

#[cfg(test)]
mod tests {
    use super::{PROJECT_MARKER, PROJECT_MARKER_COUNT, TESTING_FILES, TESTING_ROUTES};

    /// The bundle carries exactly the pinned marker count — the
    /// substitution contract (every marker replaced at pack).
    #[test]
    fn project_marker_count_is_pinned() {
        let total: usize = TESTING_FILES
            .iter()
            .map(|(_, contents)| contents.matches(PROJECT_MARKER).count())
            .sum();
        assert_eq!(
            total, PROJECT_MARKER_COUNT,
            "a testing-bundle edit moved the __IGN_CLI_PROJECT__ marker count"
        );
    }

    /// The bundle is complete: 20 members, both route folders, all six
    /// script modules — the D2 manifest pinned.
    #[test]
    fn bundle_manifest_is_pinned() {
        assert_eq!(TESTING_FILES.len(), 18, "18 members: 12 script + 6 route");
        for route in TESTING_ROUTES {
            let prefix = format!("com.inductiveautomation.webdev/resources/testing/{route}/");
            let files = TESTING_FILES
                .iter()
                .filter(|(name, _)| name.starts_with(&prefix))
                .count();
            assert_eq!(files, 3, "route {route} carries its 3 files (POST-only - the two-method config does not register on 8.3)");
        }
        for module in ["runner", "assertions", "decorators", "helpers", "reporter", "__tests__"] {
            let path = format!("ignition/script-python/resources/testing/{module}/code.py");
            assert!(
                TESTING_FILES.iter().any(|(name, _)| *name == path),
                "module {module} present"
            );
        }
    }

    /// The runner ships the 8.3 walk (the resources-container base with
    /// the 8.1 fallback) — the relocation patch, retired IN the source.
    #[test]
    fn runner_carries_the_dual_layout_walk() {
        let runner = TESTING_FILES
            .iter()
            .find(|(name, _)| *name == "ignition/script-python/resources/testing/runner/code.py")
            .map(|(_, contents)| *contents)
            .expect("runner present");
        assert!(runner.contains("resources_path = File(script_path, \"resources\")"));
        assert!(runner.contains("base = resources_path if resources_path.exists() else script_path"));
    }
}
