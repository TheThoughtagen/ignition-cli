//! Data-driven coverage for the `ign deploy` command family (DSURF-01).
//!
//! Phase 21 lands this file; Phases 22-25 keep it green as `plan`, `pull`,
//! `verify`, `apply`, `rollback` and `projects` land. It walks the LIVE clap
//! tree (`Cli::command()`), so a new `deploy` leaf with no README Commands
//! row, no TUI route row, or a deploy slug missing from the README exit
//! table fails here instead of drifting.
//!
//! Every test collects ALL misses and fails with the full list. Only the
//! routes test needs the `tui` feature; the README and clap tests run under
//! `--no-default-features` too.

use clap::CommandFactory;
use ignition_cli::cli::Cli;
use ignition_core::error::CoreError;

const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"));

/// The `deploy` subcommand of the live clap tree.
fn deploy_command() -> Option<clap::Command> {
    Cli::command()
        .get_subcommands()
        .find(|c| c.get_name() == "deploy")
        .cloned()
}

/// Leaf rule copied from tui_coverage.rs: a node with no subcommands other
/// than clap's `help`, recursing into groups so nested verbs are covered.
/// Paths are space-joined without the `deploy` prefix; clap declaration
/// order.
fn leaves(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        if sub.get_subcommands().all(|c| c.get_name() == "help") {
            out.push(path.clone());
        }
        leaves(sub, &path, out);
    }
}

fn deploy_leaves() -> Vec<String> {
    let deploy = deploy_command().expect("the `deploy` subcommand exists");
    let mut out = Vec::new();
    leaves(&deploy, "", &mut out);
    out
}

/// The text of the README section that starts at the `## <heading>` line
/// beginning with `heading` and ends at the next `## ` line.
fn section(heading: &str) -> &'static str {
    let marker = format!("\n## {heading}");
    let start = README
        .find(&marker)
        .unwrap_or_else(|| panic!("README has no `## {heading}` section"))
        + 1;
    let rest = &README[start..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    &rest[..end]
}

/// Whether `line` mentions `ign deploy <leaf>` followed by a space or a
/// backtick, so a row for `plan` never satisfies a leaf `plan-all`.
fn mentions_leaf(line: &str, leaf: &str) -> bool {
    let needle = format!("ign deploy {leaf}");
    let mut from = 0;
    while let Some(i) = line[from..].find(&needle) {
        let end = from + i + needle.len();
        if matches!(line[end..].chars().next(), Some(' ') | Some('`')) {
            return true;
        }
        from = end;
    }
    false
}

#[test]
fn deploy_family_has_leaves() {
    let deploy = deploy_command().expect("the `deploy` subcommand is missing from the CLI");
    assert!(
        deploy.is_subcommand_required_set(),
        "`deploy` must require a subcommand (a bare `ign deploy` is not an action)"
    );
    assert!(
        !deploy_leaves().is_empty(),
        "`deploy` has no leaves; the coverage tests below would pass vacuously"
    );
}

#[test]
fn every_deploy_leaf_has_a_readme_commands_row() {
    // adjacency edge: one leaf's row never satisfies a leaf it prefixes
    assert!(mentions_leaf("| `ign deploy plan --env X` | x |", "plan"));
    assert!(mentions_leaf("| `ign deploy plan` | x |", "plan"));
    assert!(!mentions_leaf("| `ign deploy plan-all` | x |", "plan"));
    assert!(!mentions_leaf("| `ign deploy planner` | x |", "plan"));
    assert!(mentions_leaf(
        "| `ign deploy plan-all` | `ign deploy plan` |",
        "plan"
    ));

    let found = deploy_leaves();
    assert!(!found.is_empty(), "no deploy leaves to check");
    let commands = section("Commands");
    let missing: Vec<String> = found
        .iter()
        .filter(|leaf| {
            !commands
                .lines()
                .any(|line| line.starts_with('|') && mentions_leaf(line, leaf))
        })
        .map(|leaf| format!("deploy {leaf}"))
        .collect();
    assert!(
        missing.is_empty(),
        "deploy leaves with no README `## Commands` table row \
         (a row must contain `ign deploy <leaf>` followed by a space or a backtick): {missing:#?}"
    );
}

#[test]
fn every_deploy_slug_is_in_the_readme_exit_table() {
    assert!(
        !CoreError::DEPLOY_SLUGS.is_empty(),
        "DEPLOY_SLUGS is empty; the check would pass vacuously"
    );
    let table = section("Exit codes");
    let missing: Vec<&&str> = CoreError::DEPLOY_SLUGS
        .iter()
        .filter(|slug| !table.contains(&format!("`{slug}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "deploy slugs missing (backticked) from README `## Exit codes`: {missing:#?}"
    );
}

#[cfg(feature = "tui")]
#[test]
fn every_deploy_leaf_has_a_projects_route_row() {
    use ignition_tui::routes::{Mapping, routes};
    use ignition_tui::state::Screen;

    let found = deploy_leaves();
    assert!(!found.is_empty(), "no deploy leaves to check");
    let mut problems = Vec::new();

    for leaf in &found {
        let path = format!("deploy {leaf}");
        let rows: Vec<_> = routes().iter().filter(|r| r.path == path).collect();
        match rows.as_slice() {
            [row] if matches!(row.mapping, Mapping::Screen(Screen::Projects)) => {}
            [_] => problems.push(format!("{path}: row is not mapped to Screen::Projects")),
            [] => problems.push(format!("{path}: no routes() row")),
            many => problems.push(format!(
                "{path}: {} routes() rows, expected one",
                many.len()
            )),
        }
    }
    for row in routes() {
        if let Some(leaf) = row.path.strip_prefix("deploy ")
            && !found.iter().any(|l| l == leaf)
        {
            problems.push(format!("{}: route row with no clap leaf", row.path));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
