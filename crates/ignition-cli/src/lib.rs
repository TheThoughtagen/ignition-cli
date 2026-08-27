//! ignition-cli's library surface: the clap command tree, shared by
//! the `ign` binary and the integration tests. The TUI-coverage test
//! (tests/tui_coverage.rs, 06-06) walks `Cli::command()` in-process —
//! the SAME `CommandFactory` mechanism clap_complete uses — so the
//! registry-vs-tree proof runs against the compiled truth, never a
//! hand-copied list.
//!
//! The dispatch/render chassis stays binary-only (main.rs); only the
//! command definition is dual-published.

pub mod cli;
