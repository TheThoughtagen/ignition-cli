//! Restart + diagnostics-probe capabilities (02-05, HLTH-09/10/11).
//!
//! - [`RESTART_PATH`]: the one big red button — POST with
//!   `confirm=true`, `--yes`-guarded at the CLI seam (research Pitfall
//!   10: it takes the whole gateway down). The gateway answers 200
//!   with the literal body `true` almost immediately; the ~40 s wait
//!   is POLLER-side (the 02-04 engine owns it).
//! - [`SCAN_PROJECTS_PATH`]: igw-cli's harmless project-rescan write
//!   probe — `ign doctor --check-write` fires it (2xx = write
//!   permission, 403 = read-only token).
//! - [`SECURITY_PROPERTIES_PATH`] + [`WEBDEV_ROOT`]: doctor inputs —
//!   the security config singleton (the 403 three-part diagnosis's
//!   part 2: what the gateway's read/write permissions actually are)
//!   and the WebDev route-presence probe root.
//!
//! Deliberately ABSENT: `restart-tasks/pending`. Research is explicit
//! it is *required-restart* config (changes needing a restart), NOT
//! active-restart status — never use it as a restart-progress signal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// POST path of the restart capability (query param `confirm=true`).
pub(crate) const RESTART_PATH: &str = "/data/api/v1/restart-tasks/restart";

/// POST path of the project-scan write probe (doctor `--check-write`).
pub(crate) const SCAN_PROJECTS_PATH: &str = "/data/api/v1/scan/projects";

/// GET path of the security-properties config singleton — the doctor's
/// permissions deep-dive (02-RESEARCH §Doctor inputs 5b; the resource
/// singleton read, same family 02-03 verified for the connection
/// lists).
#[cfg_attr(not(test), expect(dead_code))]
pub(crate) const SECURITY_PROPERTIES_PATH: &str =
    "/data/api/v1/resources/ignition/security-properties";

/// Root of the WebDev route surface — doctor probes
/// `/system/webdev/<route>` for presence (404 = absent).
pub(crate) const WEBDEV_ROOT: &str = "/system/webdev/";

/// GET `/data/api/v1/resources/ignition/security-properties` — the
/// gateway security config singleton. `readPermissions` /
/// `writePermissions` are raw passthrough: their populated value shape
/// was NOT live-captured (the research rig read them as config trees),
/// so doctor surfaces them verbatim rather than typing a guess.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityProperties {
    /// The read-permission wiring (passthrough; e.g. an AnyOf level
    /// tree), when reported.
    #[serde(
        rename = "readPermissions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub read_permissions: Option<serde_json::Value>,
    /// The write-permission wiring (passthrough), when reported.
    #[serde(
        rename = "writePermissions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub write_permissions: Option<serde_json::Value>,
    /// Unknown keys round-trip (passthrough-shaped `--json`).
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// The full probe path for one WebDev route: `/system/webdev/<route>`
/// (ignition-mcp's verified URL shape; 02-RESEARCH §Sources).
#[cfg_attr(not(test), expect(dead_code))]
pub(crate) fn webdev_route_path(route: &str) -> String {
    format!("{WEBDEV_ROOT}{route}")
}

#[cfg(test)]
mod tests {
    use super::{SecurityProperties, webdev_route_path};

    /// The doctor probe paths, contract-pinned (the plan's key_links:
    /// `security-properties` + `scan/projects` — every wire path in
    /// this codebase carries a literal pin).
    #[test]
    fn doctor_probe_paths_pinned() {
        assert_eq!(
            super::SECURITY_PROPERTIES_PATH,
            "/data/api/v1/resources/ignition/security-properties"
        );
        assert_eq!(super::WEBDEV_ROOT, "/system/webdev/");
        assert_eq!(super::SCAN_PROJECTS_PATH, "/data/api/v1/scan/projects");
        assert_eq!(super::RESTART_PATH, "/data/api/v1/restart-tasks/restart");
        assert_eq!(webdev_route_path("stacked"), "/system/webdev/stacked");
    }

    /// The singleton parses with both permission blocks surfaced under
    /// their gateway-native names, unknown keys passthrough.
    #[test]
    fn security_properties_parses_and_passes_through() {
        let props: SecurityProperties = serde_json::from_value(serde_json::json!({
            "readPermissions": {"anyOf": ["Authenticated/Roles/Administrator"]},
            "writePermissions": {"anyOf": ["Authenticated/Roles/Administrator"]},
            "secureChannelRequired": true
        }))
        .expect("singleton shape must parse");
        assert!(props.read_permissions.is_some());
        assert!(props.write_permissions.is_some());
        assert!(
            props.extra.contains_key("secureChannelRequired"),
            "unknown keys round-trip"
        );

        // Round-trip keeps the gateway-native key names.
        let round = serde_json::to_value(&props).expect("serialize");
        assert!(round.get("readPermissions").is_some());
        assert!(round.get("writePermissions").is_some());
    }

    /// A sparse singleton (no permission blocks) parses — they are
    /// Option on purpose.
    #[test]
    fn security_properties_tolerates_sparse_bodies() {
        let props: SecurityProperties =
            serde_json::from_value(serde_json::json!({"name": "whk"})).expect("sparse body parses");
        assert_eq!(props.read_permissions, None);
        assert_eq!(props.write_permissions, None);
    }
}
