//! The embedded Playwright scaffold `ign e2e init` writes (QUICK-tg4).
//!
//! The `TESTING_FILES` shape, verbatim: `(written_member_path,
//! include_str!(source))` pairs, with markers substituted at write
//! time. Compiling the scaffold into the binary is what makes
//! `ign e2e init` work on a machine that has never seen this repo.
//!
//! ## The `.tmpl` suffix (D9)
//!
//! Every source file on disk carries a `.tmpl` suffix, and the WRITTEN
//! name is the first element of each pair. This is not decoration:
//!
//! - A literal `.gitignore` inside `e2e-template/` would be honored by
//!   git against its own siblings, hiding the very files that must be
//!   version-controlled.
//! - A literal `package.json` there would be picked up by any npm
//!   workspace scan that walks this repo.
//!
//! One uniform rule avoids both hazards, and
//! [`tests::no_member_path_keeps_the_template_suffix`] pins that a
//! `.tmpl` suffix can never reach a user's disk.

use crate::error::CoreError;

/// The project whose WebDev testing routes the scaffold calls.
const PROJECT_MARKER: &str = "__IGN_PROJECT__";
/// The Perspective project the browser test navigates to.
const RUN_PROJECT_MARKER: &str = "__IGN_RUN_PROJECT__";
/// The gateway URL baked in as the `IGNITION_URL` fallback.
const GATEWAY_URL_MARKER: &str = "__IGN_GATEWAY_URL__";

/// The template suffix every source carries and no written member may.
pub const TEMPLATE_SUFFIX: &str = ".tmpl";

/// Total marker occurrences across the whole template tree.
///
/// Pinned so an edit that drops a marker (shipping a scaffold with a
/// literal `__IGN_PROJECT__` in it) or adds an unsubstituted one fails
/// CI rather than a user's first `npm test` — the `testing.rs`
/// precedent.
pub const TEMPLATE_MARKER_COUNT: usize = 11;

/// The scaffold — `(written_member_path, template_body)` pairs.
///
/// Written names, NOT source names: `gitignore.tmpl` lands as
/// `.gitignore`.
pub const E2E_TEMPLATE_FILES: &[(&str, &str)] = &[
    (
        "package.json",
        include_str!("../../e2e-template/package.json.tmpl"),
    ),
    (
        "playwright.config.mjs",
        include_str!("../../e2e-template/playwright.config.mjs.tmpl"),
    ),
    (
        "global-setup.mjs",
        include_str!("../../e2e-template/global-setup.mjs.tmpl"),
    ),
    (
        "lib/gateway.mjs",
        include_str!("../../e2e-template/lib/gateway.mjs.tmpl"),
    ),
    (
        "tests/example.spec.mjs",
        include_str!("../../e2e-template/tests/example.spec.mjs.tmpl"),
    ),
    (
        ".gitignore",
        include_str!("../../e2e-template/gitignore.tmpl"),
    ),
    (
        "README.md",
        include_str!("../../e2e-template/README.md.tmpl"),
    ),
];

/// Substitute the three markers.
///
/// The two project markers do not overlap (`__IGN_PROJECT__` is not a
/// substring of `__IGN_RUN_PROJECT__` — the fourth byte differs), so
/// the replacement order is irrelevant and no escaping is needed.
pub fn render_template(body: &str, project: &str, run_project: &str, gateway_url: &str) -> String {
    body.replace(RUN_PROJECT_MARKER, run_project)
        .replace(PROJECT_MARKER, project)
        .replace(GATEWAY_URL_MARKER, gateway_url)
}

/// Every member path, in write order.
pub fn member_paths() -> Vec<&'static str> {
    E2E_TEMPLATE_FILES.iter().map(|(path, _)| *path).collect()
}

/// Guard against a member path that would escape the target directory
/// (`..`, an absolute path). The members are compile-time constants, so
/// this can only fire on a bad edit — which is exactly when a write
/// outside the operator-named directory would be worst.
pub fn validate_member_paths() -> Result<(), CoreError> {
    for (path, _) in E2E_TEMPLATE_FILES {
        let p = std::path::Path::new(path);
        if p.is_absolute()
            || p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(CoreError::Internal(format!(
                "e2e template member {path:?} is not a relative in-tree path"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_marker_is_substituted() {
        let total: usize = E2E_TEMPLATE_FILES
            .iter()
            .map(|(_, body)| {
                body.matches(PROJECT_MARKER).count()
                    + body.matches(RUN_PROJECT_MARKER).count()
                    + body.matches(GATEWAY_URL_MARKER).count()
            })
            .sum();
        assert_eq!(
            total, TEMPLATE_MARKER_COUNT,
            "a template edit moved the marker count — update TEMPLATE_MARKER_COUNT \
             deliberately, or restore the marker that was dropped"
        );

        // And nothing survives a render.
        for (path, body) in E2E_TEMPLATE_FILES {
            let rendered = render_template(body, "ign-cli", "Flux", "https://gw:8043");
            for marker in [PROJECT_MARKER, RUN_PROJECT_MARKER, GATEWAY_URL_MARKER] {
                assert!(
                    !rendered.contains(marker),
                    "{path} still carries {marker} after rendering"
                );
            }
        }
    }

    /// A `.tmpl` suffix reaching a user's disk would ship them a
    /// `package.json.tmpl` npm ignores.
    #[test]
    fn no_member_path_keeps_the_template_suffix() {
        for path in member_paths() {
            assert!(
                !path.ends_with(TEMPLATE_SUFFIX),
                "member {path:?} would be written with the template suffix"
            );
        }
        assert_eq!(member_paths().len(), 7, "seven members");
        assert!(
            member_paths().contains(&".gitignore"),
            "gitignore.tmpl maps to the DOTTED name"
        );
        validate_member_paths().expect("every member is a relative in-tree path");
    }

    /// THE cross-file agreement (D2): the runner config and the test
    /// helper each carry their own copy of the localhost guard, on
    /// purpose (a cross-import between them is a load-order hazard).
    /// This test is what keeps the two copies from drifting — which
    /// would mean Playwright and the helper disagreeing about whether a
    /// gateway is local, and TLS being relaxed for one but not the
    /// other.
    #[test]
    fn the_localhost_guard_agrees_across_config_and_helper() {
        let render = |name: &str| {
            let (_, body) = E2E_TEMPLATE_FILES
                .iter()
                .find(|(path, _)| *path == name)
                .unwrap_or_else(|| panic!("no {name} member"));
            render_template(body, "ign-cli", "Flux", "http://localhost:9088")
        };
        let config = render("playwright.config.mjs");
        let helper = render("lib/gateway.mjs");

        for token in [
            "'localhost'",
            "'127.0.0.1'",
            "'::1'",
            "IGNITION_ALLOW_INSECURE_LOCAL",
        ] {
            assert!(
                config.contains(token),
                "playwright.config.mjs lost the guard token {token}"
            );
            assert!(
                helper.contains(token),
                "lib/gateway.mjs lost the guard token {token}"
            );
        }

        // The relaxation lives ONLY in the helper, and it is announced.
        assert!(
            helper.contains("NODE_TLS_REJECT_UNAUTHORIZED"),
            "the helper applies the relaxation"
        );
        assert!(
            helper.contains("NODE_EXTRA_CA_CERTS"),
            "the helper names the correct remote-gateway path"
        );
        assert!(
            helper.contains("process.stderr.write"),
            "the relaxation is announced on stderr, never silent"
        );
    }

    /// The scaffold's own `.gitignore` must hide the session file.
    #[test]
    fn the_scaffold_gitignores_its_session_state() {
        let (_, body) = E2E_TEMPLATE_FILES
            .iter()
            .find(|(path, _)| *path == ".gitignore")
            .expect("the gitignore member");
        assert!(body.contains(".auth/"), "the auth directory is ignored");
        assert!(body.contains("node_modules/"));
    }
}
