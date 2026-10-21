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

/// DAPL-07, Decision A: the `deploy apply` rolled-back verdict exception is
/// documented under `## Output contract (for agents)` in a dated `###`
/// section that names the envelope fields, exit 6 and Decision A, and the
/// verdict slug is NOT an exit-table row (it lives only in `data.slug`).
#[test]
fn rolled_back_verdict_exception_is_dated_and_documented() {
    let body = dated_exception_body("### The `deploy apply` rolled-back verdict exception (");
    for needle in [
        "data.ok",
        "data.slug",
        "data.cause",
        "deploy_rolled_back",
        "exit 6",
        "`ok: true`",
        "Decision A",
    ] {
        assert!(
            body.contains(needle),
            "exception text must mention {needle}: {body}"
        );
    }
    assert!(
        !section("Exit codes").contains("deploy_rolled_back"),
        "`deploy_rolled_back` is a verdict slug, never an exit-table row"
    );
    assert!(
        !CoreError::DEPLOY_SLUGS.contains(&"deploy_rolled_back"),
        "the verdict slug is not a CoreError slug"
    );
    // The exception heading sits under the output contract, and the README
    // tells an agent the `deploy apply` verb's own section exists. `lines()`
    // drops a CRLF too: git for Windows checks the README out with CRLF.
    assert!(
        README.lines().any(|line| line == "### `deploy apply`"),
        "the README has a `### `deploy apply`` section"
    );
}

/// The body of the dated `###` section under `## Output contract (for
/// agents)` whose heading starts with `prefix`; asserts the heading closes
/// with a `YYYY-MM-DD)` date.
fn dated_exception_body(prefix: &str) -> &'static str {
    let contract = section("Output contract (for agents)");
    let start = contract
        .find(prefix)
        .unwrap_or_else(|| panic!("no `{prefix}<YYYY-MM-DD>)` heading under the output contract"));
    let after = &contract[start + prefix.len()..];
    // `lines()` drops a CRLF too: git for Windows checks the README out with
    // CRLF line endings.
    let date = after
        .lines()
        .next()
        .and_then(|line| line.strip_suffix(')'))
        .expect("the heading closes with `)`");
    let bytes = date.as_bytes();
    assert!(
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit()),
        "the heading date must be YYYY-MM-DD, got {date:?}"
    );
    let body_start = &after[date.len()..];
    let body_end = body_start[3..]
        .find("\n### ")
        .map_or(body_start.len(), |i| i + 3);
    &body_start[..body_end]
}

/// DSURF-04: the `deploy verify` verdict exception is documented under
/// `## Output contract (for agents)` in a dated `###` section, names the
/// envelope fields and exit 6, and the verdict slug is NOT an exit-table row
/// (it lives only in `data.slug`).
#[test]
fn verify_verdict_exception_is_dated_and_documented() {
    let contract = section("Output contract (for agents)");
    let prefix = "### The `deploy verify` verdict exception (";
    let start = contract
        .find(prefix)
        .unwrap_or_else(|| panic!("no `{prefix}<YYYY-MM-DD>)` heading under the output contract"));
    let after = &contract[start + prefix.len()..];
    // `lines()` drops a CRLF too: git for Windows checks the README out with
    // CRLF line endings.
    let date = after
        .lines()
        .next()
        .and_then(|line| line.strip_suffix(')'))
        .expect("the heading closes with `)`");
    let bytes = date.as_bytes();
    assert!(
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit()),
        "the heading date must be YYYY-MM-DD, got {date:?}"
    );
    let body_start = &after[date.len()..];
    let body_end = body_start[3..]
        .find("\n### ")
        .map_or(body_start.len(), |i| i + 3);
    let body = &body_start[..body_end];
    for needle in ["data.ok", "deploy_verify_failed", "exit 6", "`ok: true`"] {
        assert!(
            body.contains(needle),
            "exception text must mention {needle}: {body}"
        );
    }
    assert!(
        !section("Exit codes").contains("deploy_verify_failed"),
        "`deploy_verify_failed` is a verdict slug, never an exit-table row"
    );
    assert!(
        !CoreError::DEPLOY_SLUGS.contains(&"deploy_verify_failed"),
        "the verdict slug is not a CoreError slug"
    );
}

/// The text of a `### ` section of the README, from its heading line to the
/// next `### ` or `## ` heading (CRLF-safe).
fn subsection(heading_line: &str) -> String {
    let mut lines = README.lines();
    let mut out = Vec::new();
    for line in lines.by_ref() {
        if line == heading_line {
            break;
        }
    }
    for line in lines {
        if line.starts_with("### ") || line.starts_with("## ") {
            break;
        }
        out.push(line);
    }
    assert!(!out.is_empty(), "README has no `{heading_line}` section");
    // Whitespace runs collapse, so a phrase the README wraps still matches.
    out.join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 23-14 (DAPL-08, DAPL-09): the `deploy rollback` section agrees with the
/// `deploy apply` section on the recovery command, snapshot removal and the
/// hand-deleted journal; carries the Linux live gap (FA-23-05) and the 176 s
/// wait after an ssh drop; and leaves the dated apply exception alone (a
/// rollback has no verdict exit).
#[test]
fn rollback_section_agrees_with_apply_section() {
    let apply = subsection("### `deploy apply`");
    let rollback = subsection("### `deploy rollback`");
    assert_eq!(
        README.matches("\n### `deploy rollback`").count(),
        1,
        "exactly one rollback section"
    );
    let command = "ign deploy rollback --run <id> --env <env>";
    assert!(apply.contains(command), "the apply section names {command}");
    assert!(
        rollback.contains(command),
        "the rollback section names {command}"
    );
    // Removing a snapshot makes a run unrollbackable: said once, in the
    // apply section; the rollback section points to it.
    assert!(apply.contains("can no longer be rolled back"));
    assert!(rollback.contains("Runs are never pruned"));
    // A journal is deleted by hand only when `rollback_failed` says so.
    assert!(apply.contains("Never delete a `journal.json` by hand unless the"));
    assert!(rollback.contains("rollback_failed"));
    assert!(rollback.contains("delete the journal at the path"));
    // The Linux live record (FA-23-05, run on ign-lnx-836 by 23-22): all five
    // live gates passed, so the old "unproven" claim is gone; what is still
    // not captured is the move across file systems.
    assert!(rollback.contains("FA-23-05"));
    assert!(rollback.contains("Linux ssh was run against a live host"));
    assert!(rollback.contains("0644 files and 0755 directories"));
    assert!(rollback.contains(
        "`ign deploy plan`, `apply`, `verify` and `rollback --run` against the real `production` collection"
    ));
    assert!(rollback.contains("read and execute (list) on `data` and `data/config`"));
    assert!(rollback.contains("different file systems"));
    assert!(
        !rollback.contains("unproven against a live host"),
        "the old unproven claim is gone"
    );
    assert!(
        !rollback.contains(
            "pass the shared host conformance table under a local `sh` (dash, with `shasum`) only"
        ),
        "the old, pre-live sentence is gone"
    );
    // The ssh-drop wait.
    assert!(rollback.contains("176 s"));
    assert!(rollback.contains("After an ssh drop"));
    assert!(apply.contains("**After an ssh drop.**"));
    // The recovery states the D-09..D-12 rules and the windows A9 and A10.
    for needle in [
        "cleared",
        "interrupted_run",
        "`--discard-live-changes`",
        "A9",
        "A10",
        "no Ctrl-C handler",
    ] {
        assert!(rollback.contains(needle), "rollback section lacks {needle}");
    }
    // The dated exception covers apply only.
    assert!(
        !rollback.contains("deploy_rolled_back"),
        "rollback has no verdict slug"
    );
    let plan = subsection("### `deploy plan`");
    assert!(
        plan.contains("deploy_journal_present") && plan.contains("`deploy rollback`"),
        "`deploy plan` says it refuses over a journal"
    );
}
