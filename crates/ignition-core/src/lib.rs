//! Core library for `ign` — the Ignition 8.3+ gateway CLI.
//!
//! This crate holds everything the binary (and, from Phase 6, the TUI) shares:
//! config/profile management, the gateway HTTP client, actions, the typed
//! error taxonomy, and output models.
//!
//! Invariants (ARCHITECTURE.md layering, made structural in Phase 1):
//! - Core compiles without clap and without ratatui.
//! - Core never prints to stdout — it returns models; the binary renders.
//! - Actions are plain functions over injected [`GatewayApi`]-style seams, so
//!   the CLI and the TUI call the same code.
//!
//! Modules `client` is added by the later plans of Phase 1; `error` (the
//! LOCKED exit-code taxonomy + failure envelope), `output` (the LOCKED
//! success envelope), `config` (discovery, profiles, secrets), and
//! `actions` (the shared verb layer) are the contract core.

pub mod actions;
pub mod config;
pub mod error;
pub mod output;
