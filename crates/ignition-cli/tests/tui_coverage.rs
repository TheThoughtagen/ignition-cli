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
//! The OutOfBand row set is exactly `["adopt", "api call",
//! "completions", "edit", "lsp", "mcp"]` — the LEAF-REPRESENTABLE
//! sanctioned stdout
//! exceptions. The flag-value / stream-form exceptions are NOT
//! distinct leaves and carry no rows: `logs -f` NDJSON is a FLAG on
//! the Screen-mapped `logs` leaf; `tags export -o -` is a FLAG VALUE
//! on the Screen-mapped `tags export` leaf; `rig logs` raw passthrough
//! maps as Streamed. The exceptions stay traceable through the
//! routes.rs comments.
//!
//! Reserved OutOfBand slugs (`mcp`, `lsp` — 08-06): sanctioned
//! out-of-band FUTURES, pre-declared with justification at the pinned
//! OutOfBand test and in routes.rs; rows land with their clap commands
//! in Phase 14, not before (orphan rows fail the walk by design).
//! `edit` joined the set in 13-08 and `mcp` joined in 14-01 — both
//! 08-06 reservations fulfilled with their clap commands in the same
//! landings. `lsp` joined in 14-03 the same way — the last 08-06
//! reservation is FULFILLED and the set is closed.

#![cfg(feature = "tui")]

use std::collections::BTreeSet;

use clap::CommandFactory;
use ignition_cli::cli::Cli;
use ignition_tui::routes::{Mapping, menu_label, routes};
use ignition_tui::state::{ACTIONS, Screen};

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

/// Mapping-kind sanity: the OutOfBand row set is pinned to the
/// leaf-representable sanctioned stdout exceptions — exactly
/// `["adopt", "api call", "completions", "edit", "lsp", "mcp"]`
/// (compared as a
/// SET: order-free by design, so a routes() re-ordering cannot churn
/// the pin).
///
/// Why `adopt` belongs here (ADOPT-03 justification): the bootstrap
/// verb's product includes a ONE-TIME plaintext token print (the
/// env-fallback path) and an env-sourced password — a cockpit screen
/// persisting either is a redaction violation by construction
/// (completions/api-call genre: the verb talks to a terminal
/// consumer, never to a dashboard).
///
/// Why `api call` belongs here (09-03 justification): raw passthrough
/// to ARBITRARY gateway REST endpoints is not a cockpit verb — there
/// is no screenable surface for an arbitrary method/path/body
/// combination, and the envelope (gateway-verbatim `data` in;
/// gateway-verbatim error bodies out on the exit-2 catch-all) IS the
/// product. Same genre as `completions`: the CLI speaks directly to a
/// consumer (a shell, an agent), not to a human at a dashboard.
///
/// Why `edit` belongs here (13-08 justification): the kubectl-edit
/// loop hands the terminal to a child $EDITOR process and writes ALL
/// prose to stderr — the edit loop IS the product, so no cockpit
/// surface can host it (completions/api-call genre). Edit's concrete
/// OutOfBand meaning is ZERO stdout bytes in every mode, byte-scan
/// pinned by contract_edit.rs over the real binary.
///
/// Why `mcp` belongs here (14-01 justification): the Model Context
/// Protocol server's stdout stream IS the product — newline-delimited
/// JSON-RPC 2.0 answered for an MCP client; OutOfBand means "not the
/// envelope render path", NOT "silent" (completions/api-call/edit
/// genre, one more consumer: the protocol client itself).
///
/// Why `lsp` belongs here (14-03 justification): the LSP server's
/// stdout stream IS the product — Content-Length-framed JSON-RPC 2.0
/// answered for an LSP client (nvim's); OutOfBand means "not the
/// envelope render path", NOT "silent" — the LSP convention routes
/// diagnostics and log prose to stderr (mcp genre, one more
/// protocol client).
///
/// These landings FULFILL the Phase-8 pre-declaration (08-06): `edit`
/// was reserved with written justification since 08-06 with zero rows
/// and landed with its clap command in 13-08; `mcp` was reserved the
/// same way and landed with its clap command in 14-01; `lsp` landed
/// with its clap command in 14-03 — orphan rows fail the sibling clap
/// walk by design, so each row and its clap command HAD to land
/// together, and they did. The 08-06 reservation set is closed.
#[test]
fn out_of_band_rows_are_pinned() {
    let mut out_of_band: Vec<&str> = routes()
        .iter()
        .filter(|route| matches!(route.mapping, Mapping::OutOfBand))
        .map(|route| route.path)
        .collect();
    out_of_band.sort_unstable();
    assert_eq!(
        out_of_band,
        vec!["adopt", "api call", "completions", "edit", "lsp", "mcp"],
        "the OutOfBand set must stay exactly [api call, completions, edit, mcp, lsp] — \
         a new member needs a written justification here AND in routes.rs, \
         landing in the SAME task as its clap command"
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
        "workspace",
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

/// The 13-07 workspace family rows land with their clap commands
/// (the family-atomicity pin, the diagnostics-rows shape): exactly
/// the three WorkspaceCommand leaves, all on the Projects screen
/// (normal envelope verbs — workspace is NOT OutOfBand; the pinned
/// OutOfBand set is untouched and `edit` joins it in 13-08, not
/// here).
#[test]
fn workspace_rows_cover_the_family() {
    let rows: Vec<&ignition_tui::routes::CliRoute> = routes()
        .iter()
        .filter(|route| route.path.starts_with("workspace"))
        .collect();
    let expected = ["workspace checkout", "workspace status", "workspace push"];
    assert_eq!(
        rows.len(),
        expected.len(),
        "exactly the workspace leaves that exist: {rows:?}"
    );
    for path in expected {
        let row = rows
            .iter()
            .find(|route| route.path == path)
            .unwrap_or_else(|| panic!("workspace route row {path:?} missing"));
        assert!(
            matches!(row.mapping, Mapping::Screen(Screen::Projects)),
            "{path} maps to the Projects screen"
        );
    }
}

/// THE routes↔menu parity contract for the Dashboard actions menu
/// (09-08 — the systemic blind-spot closer for UAT test 10).
///
/// 09-04/09-05 added `Mapping::Screen(Screen::Dashboard)` rows for the
/// seven Phase 9 verbs but never extended the hardcoded `ACTIONS`
/// const (nor added executor arms) — clap-walk parity held while the
/// verbs stayed unreachable from the menu. THIS test closes that
/// failure mode structurally: adding a Dashboard-mapped route without
/// a menu entry, or a menu entry without a Dashboard-mapped route,
/// fails CI in the SAME change.
///
/// Scope: the DASHBOARD screen only — where the gap actually bit.
/// Per-screen extension to Logs/Tags/Projects/Rig menus is
/// deliberately out of scope.
#[test]
fn dashboard_actions_menu_matches_registry() {
    /// Every Dashboard action-menu verb as its clap-exact route path
    /// (menu labels resolve through `menu_label`). Traced 1:1 against
    /// the current ACTIONS entries: the 15 v1.1.0-era labels minus the
    /// three display-prose wait labels (which key off the seam), plus
    /// the seven 09-04/09-05 verbs (clap-exact on both sides).
    const MENU_HOSTED: &[&str] = &[
        // 06-02: the core global verbs (the wait trio via the seam).
        "version",
        "connections",
        "wait gateway",
        "wait restart",
        "wait module",
        "doctor",
        "restart",
        // 07-02: the standalone backup pair + the EAM family.
        "backup download",
        "backup restore",
        "eam history",
        "eam tasks",
        "eam task new",
        "eam task force",
        // 10-04: the guarded lifecycle/mutation verbs.
        "eam task suspend",
        "eam task resume",
        "eam task cancel",
        "eam task modify",
        "eam task delete",
        // 07-03: the scriptExec verb.
        "script run",
        // 07-04: the local ignition-lint delegation.
        "lint",
        // 09-04: the curated morning-check reads.
        "license status",
        "redundancy status",
        "gan status",
        // 09-05: the diagnostics-bundle family.
        "diagnostics bundle generate",
        "diagnostics bundle status",
        "diagnostics bundle download",
        "diagnostics bundle wait",
    ];

    // (a) THE PINNED COUNT of Screen(Dashboard) route rows. A new
    // Dashboard-mapped row changes this number and fails CI — forcing
    // a conscious decision: either the new route hosts a menu verb
    // (extend MENU_HOSTED + ACTIONS + the executor arms in the same
    // change) or it belongs to one of the JUSTIFIED exclusions below
    // (extend the exclusion comments). The pinned test IS the
    // pre-declaration (the 08-06 OutOfBand pattern).
    //
    // Current exclusions (37 Dashboard rows − 27 menu verbs = 10):
    //   - `tui` — the cockpit ITSELF (launching the TUI is not a verb
    //     the TUI's menu can host).
    //   - `status`, `modules`, `metrics`, `sessions` (bare) — the
    //     dashboard PANELS: in-screen polling data, not menu actions.
    //   - `sessions terminate` — modal-driven from the sessions
    //     panel's row action (PendingAction::TerminateSession,
    //     Confirm-gated), not a menu-listed verb.
    //   - `profile use`, `profile list`, `profile add` — the profile
    //     switcher modal (the global `p` key, 06-02 Task 3), not the
    //     actions menu.
    //   - `testing run` (QUICK-p0g) — an agent/CI verb whose product
    //     is a results DOCUMENT (counts, per-module results, an
    //     optional rendered report), not a modal round trip; a
    //     one-line result modal would lose exactly what the verb is
    //     for. Wiring it into ACTIONS behind a dedicated results pane
    //     is a clean follow-up.
    let dashboard_rows: Vec<&str> = routes()
        .iter()
        .filter(|route| matches!(route.mapping, Mapping::Screen(Screen::Dashboard)))
        .map(|route| route.path)
        .collect();
    assert_eq!(
        dashboard_rows.len(),
        37,
        "a new Screen(Dashboard) route landed — extend MENU_HOSTED + ACTIONS \
         + the update.rs executor arms in the same change, or justify the \
         exclusion in this test's comment block: {dashboard_rows:#?}"
    );

    // (b) Every MENU_HOSTED path exists in routes() as a Dashboard row
    // (a menu verb with no registry row is the orphan direction).
    for path in MENU_HOSTED {
        let row = routes()
            .iter()
            .find(|route| route.path == *path)
            .unwrap_or_else(|| panic!("menu verb {path:?} has no routes() row"));
        assert!(
            matches!(row.mapping, Mapping::Screen(Screen::Dashboard)),
            "menu verb {path:?} must map Screen(Dashboard)"
        );
    }

    // (c) + (d) — resolve every MENU_HOSTED path through the SINGLE
    // menu_label seam once, then walk both directions over the same
    // (path, label) table.
    let menu_labels: Vec<(&str, &str)> = MENU_HOSTED
        .iter()
        .map(|&path| (path, menu_label(path).unwrap_or(path)))
        .collect();

    // (c) route↔menu direction: every MENU_HOSTED path resolves to a
    // label that IS an ACTIONS entry.
    for (_, label) in &menu_labels {
        assert!(
            ACTIONS.contains(label),
            "menu label {label:?} is not an ACTIONS entry (extend ACTIONS \
             + the update.rs executor arms in the same change)"
        );
    }

    // (d) menu↔route direction: every ACTIONS entry traces to
    // EXACTLY ONE MENU_HOSTED path through the same seam — no orphan
    // verbs, no label collisions. An ACTIONS entry with no route
    // behind it is exactly the 09-04/09-05 blind spot this test
    // exists for.
    for action in ACTIONS {
        let sources: Vec<&str> = menu_labels
            .iter()
            .filter(|(_, label)| *label == action)
            .map(|(path, _)| *path)
            .collect();
        assert_eq!(
            sources.len(),
            1,
            "ACTIONS entry {action:?} must trace to exactly one MENU_HOSTED \
             path via menu_label (found {sources:?})"
        );
    }

    // (e) Cardinality equality — the two lists cannot drift apart in
    // size without (a)-(d) or this assert failing.
    assert_eq!(
        ACTIONS.len(),
        MENU_HOSTED.len(),
        "ACTIONS and MENU_HOSTED must have equal cardinality"
    );
}
