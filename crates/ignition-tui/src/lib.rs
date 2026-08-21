//! TUI cockpit for `ign` — placeholder crate.
//!
//! The real cockpit arrives in Phase 6. Until then this crate carries no
//! dependencies (no ratatui) and only a stub entry point, so the workspace
//! shape and the `tui` feature gate in `ignition-cli` stay final.

/// Placeholder TUI entry point; the cockpit is implemented in Phase 6.
///
/// # Panics
///
/// Always panics — the TUI does not exist yet.
pub fn run() -> ! {
    unimplemented!("TUI cockpit arrives in Phase 6")
}
