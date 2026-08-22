//! Actions — the shared verb layer (ARCHITECTURE.md layering invariant:
//! CLI handlers and, from Phase 6, the TUI both call these).
//!
//! Actions NEVER print — they return serde models; rendering belongs to the
//! binary (and later the TUI).

pub mod connections;
pub mod doctor;
pub mod inspect;
pub mod logs;
pub mod profile;
pub mod projects;
pub mod resources;
pub mod restart;
pub mod rig;
pub mod sessions;
pub mod version;
