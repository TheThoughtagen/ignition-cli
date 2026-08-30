//! Embedded WebDev route bundle — the CLI's own gateway-side surface.
//!
//! Phase 5 ships five action-dispatch WebDev routes (tags, tagConfig,
//! alarms, tagHistory, scriptExec) whose sources live under
//! `crates/ignition-core/webdev/routes/` (inside the crate so the
//! published package embeds them). This module embeds them into the
//! binary at compile time so `ign webdev deploy` (05-03) can zip and
//! upload the bundle with no source checkout — the routes travel with
//! the binary.
//!
//! Layering: this module is pure data (constants + [`include_str!`]); the
//! deploy orchestration and the version handshake live in the actions
//! layer. [`ROUTE_BUNDLE_VERSION`] must equal every route's `ROUTE_VERSION`
//! constant and the `webdev/routes/VERSION` file — the contract tests
//! below pin all three copies together.
//!
//! scriptExec is deliberately NOT part of [`ROUTE_FILES`]: its source is a
//! TEMPLATE carrying the `__IGN_CLI_SECRET__` substitution marker
//! ([`SCRIPT_EXEC_TEMPLATE`]). Deploy substitutes the deploy-time secret
//! before packing it, and keeping it out of the always-on bundle makes an
//! unsubstituted deploy impossible by construction.

/// Version of the embedded route bundle — the `version` handshake action
/// in every route answers with this value (as `routeVersion`).
pub const ROUTE_BUNDLE_VERSION: &str = "1.1.0";

/// Minimum CLI version the deployed routes require (handshake `minCli`).
pub const MIN_CLI: &str = "1.0";

/// The always-on deploy set: `(zip_member_path, contents)` pairs.
///
/// Member paths are forward-slash zip paths in the Designer-native layout
/// the gateway import expects (`project.json` at the root, route folders
/// under `com.inductiveautomation.webdev/resources/cli/<route>/`). The 13
/// members are `project.json` plus four route folders — tags, tagConfig,
/// alarms, tagHistory — times three files each (`resource.json`,
/// `config.json`, `doPost.py`).
pub const ROUTE_FILES: &[(&str, &str)] = &[
    // Deploy project manifest.
    (
        "project.json",
        include_str!("../../webdev/routes/project.json"),
    ),
    // tags — live tag values (version/browse/read/write).
    (
        "com.inductiveautomation.webdev/resources/cli/tags/resource.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tags/resource.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/tags/config.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tags/config.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/tags/doPost.py",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tags/doPost.py"
        ),
    ),
    // tagConfig — configuration CRUD, UDTs, bulk export.
    (
        "com.inductiveautomation.webdev/resources/cli/tagConfig/resource.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/resource.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/tagConfig/config.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/config.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py"
        ),
    ),
    // alarms — active status, journal history, acknowledge.
    (
        "com.inductiveautomation.webdev/resources/cli/alarms/resource.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/alarms/resource.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/alarms/config.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/alarms/config.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/alarms/doPost.py",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/alarms/doPost.py"
        ),
    ),
    // tagHistory — historical tag value queries.
    (
        "com.inductiveautomation.webdev/resources/cli/tagHistory/resource.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagHistory/resource.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/tagHistory/config.json",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagHistory/config.json"
        ),
    ),
    (
        "com.inductiveautomation.webdev/resources/cli/tagHistory/doPost.py",
        include_str!(
            "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/tagHistory/doPost.py"
        ),
    ),
];

/// The scriptExec route TEMPLATE — secret-gated arbitrary script execution.
///
/// Kept separate from [`ROUTE_FILES`] because deploy (05-03) must
/// substitute the `__IGN_CLI_SECRET__` marker with the deploy-time hex
/// secret BEFORE packing this member: shipping the template unsubstituted
/// would arm the gate with a publicly-known placeholder value. The route
/// itself fail-closes on exactly that state, and this separation is the
/// structural guarantee it never happens.
pub const SCRIPT_EXEC_TEMPLATE: &str = include_str!(
    "../../webdev/routes/com.inductiveautomation.webdev/resources/cli/scriptExec/doPost.py"
);

#[cfg(test)]
mod tests {
    use super::*;

    // The repo-level VERSION file — the third copy of the handshake
    // version, pinned here so all three must move together.
    const VERSION_FILE: &str = include_str!("../../webdev/routes/VERSION");

    /// (1) Every always-on doPost.py carries the handshake constants, and
    /// they match the Rust embed — route sources and binary must never
    /// drift. (String-containment: Jython isn't parseable here.)
    #[test]
    fn route_sources_carry_the_embedded_handshake_constants() {
        let route_version = format!("ROUTE_VERSION = '{}'", ROUTE_BUNDLE_VERSION);
        let min_cli = format!("MIN_CLI = '{}'", MIN_CLI);
        let mut do_post_count = 0;
        for (name, contents) in ROUTE_FILES {
            if name.ends_with("doPost.py") {
                do_post_count += 1;
                assert!(
                    contents.contains(&route_version),
                    "{name}: missing {route_version}"
                );
                assert!(contents.contains(&min_cli), "{name}: missing {min_cli}");
            }
        }
        assert_eq!(do_post_count, 4, "expected the four always-on dispatchers");
        assert_eq!(
            VERSION_FILE.trim(),
            ROUTE_BUNDLE_VERSION,
            "webdev/routes/VERSION drifted from ROUTE_BUNDLE_VERSION"
        );
    }

    /// (2) The always-on bundle must carry NO secret placeholder — the
    /// marker exists only in the scriptExec template.
    #[test]
    fn always_on_bundle_carries_no_secret_placeholder() {
        for (name, contents) in ROUTE_FILES {
            assert!(
                !contents.contains("__IGN_CLI_SECRET__"),
                "{name}: the secret placeholder must not ship in the always-on bundle"
            );
        }
    }

    /// (3) The scriptExec template is substitutable exactly once and
    /// fail-closed by default.
    #[test]
    fn script_exec_template_is_substitutable_and_fail_closed() {
        assert_eq!(
            SCRIPT_EXEC_TEMPLATE.matches("__IGN_CLI_SECRET__").count(),
            1,
            "the deploy substitution needs exactly one marker occurrence"
        );
        assert!(
            SCRIPT_EXEC_TEMPLATE.contains("SECRET = None"),
            "the template must keep its fail-closed SECRET default"
        );
    }

    /// (4) Zip member names use forward slashes only — a backslash would
    /// produce a broken member on every non-Windows zip reader and a
    /// differently-named one on Windows.
    #[test]
    fn member_names_use_forward_slashes_only() {
        for (name, _) in ROUTE_FILES {
            assert!(!name.contains('\\'), "backslash in member name: {name}");
        }
    }

    /// (5) The manifest is exactly the 13-member always-on set: one
    /// project.json plus four route folders × three files, all under the
    /// Designer-native route root.
    #[test]
    fn manifest_lists_exactly_thirteen_members() {
        assert_eq!(ROUTE_FILES.len(), 13);
        assert_eq!(
            ROUTE_FILES
                .iter()
                .filter(|(name, _)| *name == "project.json")
                .count(),
            1
        );
        assert_eq!(
            ROUTE_FILES
                .iter()
                .filter(|(name, _)| name.ends_with("doPost.py"))
                .count(),
            4
        );
        for (name, _) in ROUTE_FILES {
            assert!(
                *name == "project.json"
                    || name.starts_with("com.inductiveautomation.webdev/resources/cli/"),
                "member outside the Designer-native layout: {name}"
            );
        }
    }

    /// (6) The tagConfig route source keeps its provider-ROOT refusal
    /// (07-06): the pre-call bracket detection + RpcContext
    /// translation both refuse `provider_root_unsupported` —
    /// wiremock cannot execute the route's Python, so this source
    /// pin is the route-side regression guard (alongside the
    /// Rust-side denial mapping contract).
    #[test]
    fn tagconfig_route_source_refuses_provider_roots() {
        let (_, source) = ROUTE_FILES
            .iter()
            .find(|(name, _)| {
                *name == "com.inductiveautomation.webdev/resources/cli/tagConfig/doPost.py"
            })
            .expect("tagConfig doPost.py in the manifest");
        assert!(
            source.contains("provider_root_unsupported"),
            "the tagConfig route must keep its provider-root refusal"
        );
        assert!(
            source.contains("def is_provider_root("),
            "the bracket-form detector must stay nested inside doPost (byte-0 rule)"
        );
        assert!(
            source.contains("'No RpcContext' in traceback.format_exc()"),
            "the bare-form RpcContext translation must stay"
        );
    }
}
