//! Offline render of declared gateway trees (Phase 21).
//!
//! Invariants every `deploy` module keeps:
//! - **Pure and offline.** No network, no profile, no [`crate::session::Session`].
//!   Functions take bytes (or an injected file set) and return models.
//! - **Deterministic.** Ordered collections only, no clock reads, no
//!   randomness, no dependence on directory iteration order. The same
//!   input yields the same output or the same error.
//! - **Core never prints.** Findings ride [`crate::error::CoreError`]; the
//!   binary renders them.

pub mod apply;
pub mod exclude;
pub mod layout;
pub mod ledger;
pub mod lint;
pub mod manifest;
pub mod node;
#[cfg(any(windows, test))]
pub(crate) mod owner_acl;
pub mod pair;
pub mod plan;
pub mod project_artifact;
pub mod projects;
pub mod pull;
pub mod render;
pub mod rules;
pub mod target;
pub mod transport;
pub mod tree;
pub mod uuid;
pub mod verify;
pub mod yamlio;

/// Why `name` is not a safe single path component, or `None` when it is.
///
/// Used before any path is built from a declared name (`target.gateway`,
/// `target.mode`, tag providers, type halves), so a hostile value cannot
/// walk out of its directory (T-21-12). Refuses: empty, `.`, `..`, any `/`,
/// `\` or NUL, and any dot-leading name.
pub fn component_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("must not be empty")
    } else if name == "." || name == ".." {
        Some("must not be `.` or `..`")
    } else if name.contains('/') || name.contains('\\') {
        Some("must not contain a path separator")
    } else if name.contains('\0') {
        Some("must not contain a NUL byte")
    } else if name.starts_with('.') {
        Some("must not start with `.`")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::component_problem;

    #[test]
    fn safe_names_pass() {
        for ok in ["prod", "gw-1", "my_gateway", "a.b", "Ignition 8"] {
            assert_eq!(component_problem(ok), None, "{ok}");
        }
    }

    #[test]
    fn unsafe_names_are_refused() {
        for bad in [
            "", ".", "..", "a/b", "a\\b", "a\0b", ".hidden", "../x", "/abs",
        ] {
            assert!(component_problem(bad).is_some(), "{bad:?}");
        }
    }
}
