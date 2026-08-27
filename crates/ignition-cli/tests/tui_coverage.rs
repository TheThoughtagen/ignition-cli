//! THE structural-completeness proof (06-06, Success Criterion 1):
//! a CI test that walks the LIVE clap command tree and asserts
//! bidirectional equality with the TUI route registry.
//!
//! The phase's defining claim — "structurally complete because TUI
//! and CLI share the same actions layer" — becomes machine-enforced
//! here: adding a future CLI command without a TUI mapping FAILS CI
//! (the `missing` direction), and a registry row for a command that
//! no longer exists fails it too (the `orphans` direction).
//!
//! The walk uses clap's `CommandFactory` (`Cli::command()` — the
//! exact mechanism clap_complete uses for `ign completions`), so the
//! tree is the compiled truth, never a hand-copied list.
//!
//! ## The coverage rule (mirrors how routes() was written)
//!
//! A clap node REQUIRES a registry row when:
//!
//! (a) its subcommands minus clap's auto-generated `help` are empty
//!     (a true leaf — `status`, `project delete`, `rig up`, …), OR
//! (b) `!cmd.is_subcommand_required_set()` — the Option-subcommand
//!     case where the BARE form is itself an invocable action:
//!     `SessionsArgs.command: Option<SessionsCmd>` (cli.rs), ditto
//!     `LogsArgs` and `LoggersArgs` → bare `ign sessions`, `ign
//!     logs`, `ign logs loggers` are all real actions.
//!
//! Nodes that REQUIRE a subcommand (`wait`, `project`, `tags`, `rig`,
//! `rig trial`, … — non-Option command fields, which clap marks
//! `subcommand_required`) get no row of their own; only their
//! children do. Group-only nodes (`tags provider`, `tags config`,
//! …) are exactly this shape.
//!
//! ## The sanctioned stdout exceptions
//!
//! The OutOfBand row set is exactly `["completions"]` — the only
//! LEAF-REPRESENTABLE sanctioned stdout exception. The flag-value /
//! stream-form exceptions are NOT distinct leaves and carry no rows:
//! `logs -f` NDJSON is a FLAG on the Screen-mapped `logs` leaf;
//! `tags export -o -` is a FLAG VALUE on the Screen-mapped
//! `tags export` leaf; `rig logs` raw passthrough maps as Streamed.
//! The four-exception STATE list stays traceable through the
//! routes.rs comments.

#![cfg(feature = "tui")]

use std::collections::BTreeSet;

use clap::CommandFactory;
use ignition_cli::cli::Cli;
use ignition_tui::routes::{Mapping, routes};

/// Recurse the clap tree, collecting every ROW-REQUIRING node's
/// space-joined path. Skips clap's auto-generated `help` subcommand
/// everywhere (`disable_help_subcommand` is set nowhere — without
/// the skip, `help` would pollute the leaf set under every node).
fn walk(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        let has_real_children = sub
            .get_subcommands()
            .any(|child| child.get_name() != "help");
        if !has_real_children || !sub.is_subcommand_required_set() {
            out.push(path.clone());
        }
        walk(sub, &path, out);
    }
}

/// THE bidirectional proof: every row-requiring CLI node has a TUI
/// mapping, every registry row names a real CLI node.
#[test]
fn every_row_requiring_cli_node_is_mapped_and_no_orphans() {
    let mut cli_nodes = Vec::new();
    walk(&Cli::command(), "", &mut cli_nodes);
    assert!(!cli_nodes.is_empty(), "the walk found the tree");

    let cli_set: BTreeSet<&str> = cli_nodes.iter().map(String::as_str).collect();
    let registry_set: BTreeSet<&str> = routes().iter().map(|route| route.path).collect();

    // Direction 1 — missing: a row-requiring CLI node with NO TUI
    // mapping. This is the future-proofing direction: a new CLI
    // command lands without its registry row and CI refuses.
    let missing: Vec<&str> = cli_set.difference(&registry_set).copied().collect();
    assert!(
        missing.is_empty(),
        "CLI row-requiring nodes with no TUI mapping (add routes() rows or \
         an OutOfBand justification): {missing:#?}"
    );

    // Direction 2 — orphans: a registry row naming a CLI node that
    // does not exist (a renamed/removed command, a typo'd path).
    let orphans: Vec<&str> = registry_set.difference(&cli_set).copied().collect();
    assert!(
        orphans.is_empty(),
        "TUI registry rows with no CLI node (stale or misspelled paths): {orphans:#?}"
    );

    // Both directions green ⇒ set equality. Pin the cardinality so a
    // silently-skipped subtree (a walk regression) cannot hide.
    assert_eq!(
        cli_set.len(),
        registry_set.len(),
        "bidirectional equality over {} row-requiring nodes",
        registry_set.len()
    );
}

/// Mapping-kind sanity: the OutOfBand row set is EXACTLY the
/// `completions` leaf — the only leaf-representable sanctioned
/// stdout exception (the flag-value/stream-form exceptions are
/// documented in routes.rs comments, not rows).
#[test]
fn out_of_band_rows_are_exactly_the_completions_leaf() {
    let out_of_band: Vec<&str> = routes()
        .iter()
        .filter(|route| matches!(route.mapping, Mapping::OutOfBand))
        .map(|route| route.path)
        .collect();
    assert_eq!(
        out_of_band,
        vec!["completions"],
        "the OutOfBand set must stay exactly the completions leaf"
    );
}

/// The bare-invocable Option-subcommand forms are rows (the rule's
/// (b) branch, pinned by name so a derive change from Option to
/// required fails LOUDLY here instead of as a mysterious missing
/// row).
#[test]
fn bare_option_forms_are_row_requiring_nodes() {
    let mut cli_nodes = Vec::new();
    walk(&Cli::command(), "", &mut cli_nodes);
    let cli_set: BTreeSet<&str> = cli_nodes.iter().map(String::as_str).collect();
    for bare in ["sessions", "logs", "logs loggers"] {
        assert!(
            cli_set.contains(bare),
            "the bare form {bare:?} must be row-requiring (Option subcommand)"
        );
    }
    // And the required-subcommand groups are NOT rows (rule (b)'s
    // negative space): only their children map.
    for group in [
        "wait",
        "project",
        "tags",
        "rig",
        "rig trial",
        "profile",
        "resource",
        "webdev",
    ] {
        assert!(
            !cli_set.contains(group),
            "the required-subcommand group {group:?} must not be a row of its own"
        );
    }
}
