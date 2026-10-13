//! Fixed render policy: which resource types and files a declared tree may
//! never ship, however the manifest allowlist reads (Phase 21, DRND-04).

/// `(module, type)` pairs that render refuses even when the manifest lists
/// them. Rendering these can lock an administrator out of the gateway
/// (replacing the security level or property singletons, the user sources, or
/// the API tokens that tooling authenticates with), so they stay
/// gateway-owned. Spellings are pinned to the OpenAPI extract by the
/// `denied_types_exist_in_openapi_extract` test.
pub const DENIED_TYPES: &[(&str, &str)] = &[
    ("ignition", "security-properties"),
    ("ignition", "security-levels"),
    ("ignition", "api-token"),
    ("ignition", "user-source"),
];

/// File names (final path component) the gateway writes itself, so a
/// declared tree must not carry them. `config-mode.json` is written by the
/// gateway's mode API; shipping one would overwrite the gateway's mode.
pub const GATEWAY_OWNED_FILES: &[&str] = &["config-mode.json"];

/// Whether `module/ty` is on the deny list.
pub fn is_denied(module: &str, ty: &str) -> bool {
    DENIED_TYPES.iter().any(|(m, t)| *m == module && *t == ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_list_is_exact() {
        assert!(is_denied("ignition", "security-properties"));
        assert!(is_denied("ignition", "user-source"));
        assert!(!is_denied("ignition", "database-translator"));
        assert!(!is_denied("other", "api-token"));
    }
}
