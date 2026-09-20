//! Actions — the shared verb layer (ARCHITECTURE.md layering invariant:
//! CLI handlers and, from Phase 6, the TUI both call these).
//!
//! Actions NEVER print — they return serde models; rendering belongs to the
//! binary (and later the TUI).

pub mod adopt;
pub mod apicall;
pub mod backup;
pub mod connections;
pub mod diagnostics;
pub mod doctor;
pub mod eam;
pub mod edit;
pub mod gan;
pub mod inspect;
pub mod license;
pub mod lint;
pub mod logs;
pub mod profile;
pub mod projects;
pub mod redundancy;
pub mod resources;
pub mod restart;
pub mod rig;
pub mod script;
pub mod sessions;
pub mod tag_loss;
pub mod tags;
pub mod testing;
pub mod version;
pub mod webdev;
pub mod workspace;
