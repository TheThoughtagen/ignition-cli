//! Core library for `ign` — the Ignition 8.3+ gateway CLI.
//!
//! This crate holds everything the binary (and, from Phase 6, the TUI) shares:
//! config/profile management, the gateway HTTP client, actions, the typed
//! error taxonomy, and output models.
//!
//! Invariants (ARCHITECTURE.md layering, made structural in Phase 1):
//! - Core compiles without clap and without ratatui.
//! - Core never prints to stdout — it returns models; the binary renders.
//! - Actions are plain functions over injected [`client::GatewayApi`]-style
//!   seams, so the CLI and the TUI call the same code.
//!
//! Module map: `error` (the LOCKED exit-code taxonomy + failure envelope),
//! `output` (the LOCKED success envelope), `config` (discovery, profiles,
//! secrets, rigs), `client` (the [`client::GatewayApi`] seam + GatewayInfo),
//! `session` (the [`session::Session`] execution seam — the ONE resolution
//! choreography every auth/gateway client flows through), `poll` (the
//! shared wait/retry engine), `rig` (the compose shell-out
//! engine — runner seam, discovery, pre-flight), `actions` (the shared
//! verb layer), `webdev` (the embedded route bundle `ign webdev deploy`
//! zips — pure data, no I/O), and `module` (the pinned third-party
//! `.modl` artifact registry, fetch, and cache-verify seam — Phase 15).

pub mod actions;
pub mod client;
pub mod config;
pub mod e2e;
pub mod error;
pub mod module;
pub mod output;
pub mod poll;
pub mod rig;
pub mod session;
pub mod webdev;
