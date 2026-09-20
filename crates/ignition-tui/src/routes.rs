//! THE coverage registry: a static mapping of every CLI invocation
//! path to its TUI surface (Phase 6 research, Pattern 5 — the
//! structural-completeness proof's data source).
//!
//! Paths are space-separated leaf chains exactly matching clap's
//! subcommand chain ("logs loggers set" style). Screen plans
//! (06-02..06-06) append their families' rows; 06-06 completes the
//! table and lights the bidirectional clap-tree-walk CI test in
//! ignition-cli.

use crate::state::Screen;

/// How a CLI route maps onto the cockpit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapping {
    /// The action lives on this screen (reachable in the TUI).
    Screen(Screen),
    /// A streaming command (`logs -f`, `rig logs`): the TUI shows the
    /// equivalent stream IN-SCREEN — in-stream, not one-shot.
    Streamed,
    /// No TUI surface BY DESIGN (completions, raw-stdout pipelines like
    /// `tags export -o -`, the version warning path): out-of-band.
    ///
    /// RESERVED OutOfBand slugs (08-06 pre-declaration): `mcp` and `lsp`
    /// will join this set in Phase 14 — MCP stdio and LSP speak their own
    /// protocols on stdout. Their rows land TOGETHER with their clap
    /// commands (adding them earlier would be orphan rows and fail the
    /// clap walk by design). 09-03 added `api call`: raw passthrough is
    /// not a cockpit verb — the envelope IS the product (completions
    /// genre). 13-08 added `edit`: the kubectl-edit loop hands the
    /// terminal to a child $EDITOR and writes all prose to stderr —
    /// fulfilling the 08-06 reservation with its clap command in the
    /// same landing. 14-01 added `mcp`: the Model Context Protocol
    /// server's stdout stream IS the product (newline-delimited
    /// JSON-RPC 2.0 answered for an MCP client) — again the same genre,
    /// fulfilling that reservation with its clap command atomically.
    /// 14-03 added `lsp`: the LSP server's stdout stream IS the product
    /// (Content-Length-framed JSON-RPC 2.0 answered for nvim's LSP
    /// client) — fulfilling the last 08-06 reservation with its clap
    /// command in the same landing.
    OutOfBand,
}

/// One CLI leaf path → its mapping.
#[derive(Debug, Clone, Copy)]
pub struct CliRoute {
    /// Space-separated subcommand chain, clap-leaf-identical.
    pub path: &'static str,
    /// How the cockpit covers it.
    pub mapping: Mapping,
}

/// The registry. Seeded with the shell-known rows; grows per screen
/// plan until 06-06's coverage test demands completeness.
pub fn routes() -> &'static [CliRoute] {
    &[
        CliRoute {
            path: "tui",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "completions",
            mapping: Mapping::OutOfBand,
        },
        // 09-03: `ign api call` (EXT-01) — OutOfBand with written
        // justification: raw passthrough to arbitrary gateway REST
        // endpoints is NOT a cockpit verb — there is no screenable
        // surface for an arbitrary method/path/body, and the envelope
        // (gateway-verbatim data in, gateway-verbatim error bodies
        // out) IS the product. Same genre as `completions`: the CLI
        // talks to a consumer, not to a human at a dashboard. The
        // pinned OutOfBand test in ignition-cli's tui_coverage.rs was
        // extended to exactly [completions, api call] in the same
        // task (row + clap command land together — Pitfall 5).
        CliRoute {
            path: "api call",
            mapping: Mapping::OutOfBand,
        },
        // 13-08: `ign edit` (the 08-06 pre-declared reservation
        // FULFILLED) — OutOfBand with written justification: the
        // kubectl-edit loop hands the terminal to a child $EDITOR
        // process and writes ALL prose to stderr — there is no
        // cockpit surface for an editor round-trip (the edit loop IS
        // the product; completions/api-call genre). Edit's concrete
        // OutOfBand meaning: ZERO stdout bytes in every mode (the
        // byte-scan pin in ignition-cli's contract_edit.rs proves it
        // over the real binary). The pinned OutOfBand test in
        // tui_coverage.rs was extended to exactly [api call,
        // completions, edit] in the SAME task — row + clap command
        // land together (Pitfall 5; an orphan row would fail the clap
        // walk by design).
        CliRoute {
            path: "edit",
            mapping: Mapping::OutOfBand,
        },
        // 14-01: `ign mcp` (the 08-06 pre-declared reservation
        // FULFILLED) — OutOfBand with written justification: the Model
        // Context Protocol server's stdout stream IS the product —
        // newline-delimited JSON-RPC 2.0 answered for an MCP client
        // (completions/api-call/edit genre: the CLI talks to a
        // consumer, not to a human at a dashboard). OutOfBand means
        // "not the envelope render path", NOT "silent". The pinned
        // OutOfBand test in ignition-cli's tui_coverage.rs was
        // extended to exactly [api call, completions, edit, mcp] in
        // the SAME task — row + clap command land together (Pitfall
        // 5; an orphan row would fail the clap walk by design).
        CliRoute {
            path: "mcp",
            mapping: Mapping::OutOfBand,
        },
        // ADOPT-03: `ign adopt` — OutOfBand with written
        // justification: the bootstrap verb's product includes a
        // ONE-TIME plaintext token print (the env-fallback path) and
        // a password sourced from the caller's environment — a
        // cockpit screen persisting either is a redaction violation
        // by construction, and there is no dashboard surface for a
        // five-step bootstrap walk whose secret must never be
        // re-rendered (completions/api-call/edit/mcp genre: the verb
        // talks to a terminal consumer, not to a dashboard). The
        // pinned OutOfBand test in tui_coverage.rs was extended to
        // include `adopt` in the SAME task — row + clap command land
        // together (Pitfall 5).
        CliRoute {
            path: "adopt",
            mapping: Mapping::OutOfBand,
        },
        // 14-03: `ign lsp` (the 08-06 pre-declared reservation
        // FULFILLED) — OutOfBand with written justification: the LSP
        // server's stdout stream IS the product — Content-Length-
        // framed JSON-RPC 2.0 answered for an LSP client (nvim's;
        // completions/api-call/edit/mcp genre: the CLI talks to a
        // consumer, not to a human at a dashboard). OutOfBand means
        // "not the envelope render path", NOT "silent" — diagnostics
        // and log prose go to stderr per the LSP convention. The
        // pinned OutOfBand test in ignition-cli's tui_coverage.rs was
        // extended to exactly [api call, completions, edit, mcp, lsp]
        // in the SAME task — row + clap command land together
        // (Pitfall 5; an orphan row would fail the clap walk by
        // design).
        CliRoute {
            path: "lsp",
            mapping: Mapping::OutOfBand,
        },
        // 09-04: the three curated morning-check reads (EXT-02) —
        // Dashboard rows because they ARE morning-check verbs
        // (dashboard-shaped, like `status`/`doctor`; decision 3 kept
        // the TUI scope small — no new Diagnostics screen this
        // phase). Rows land in the SAME task as their clap commands
        // (Pitfall 5: a command family is atomic).
        CliRoute {
            path: "license status",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "redundancy status",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "gan status",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 09-05: the diagnostics-bundle family (EXT-02) — four
        // Dashboard rows (decision 3: no new screen this phase; the
        // verbs are gateway-support actions hosted beside
        // backup/restart). Rows land in the SAME task as their clap
        // commands (Pitfall 5: a command family is atomic).
        CliRoute {
            path: "diagnostics bundle generate",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "diagnostics bundle status",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "diagnostics bundle download",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "diagnostics bundle wait",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 06-02: the dashboard's read panels + its actions-menu verbs.
        // `sessions` is the BARE form (SessionsArgs.command is Option —
        // bare `ign sessions` IS the list action; there is no
        // `sessions list` leaf).
        CliRoute {
            path: "version",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "status",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "modules",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "metrics",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "connections",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "sessions",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "sessions terminate",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "wait gateway",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "wait restart",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "wait module",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "doctor",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "restart",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 06-02 Task 3: the profile family rides the switcher modal
        // (global `p` key — hosted on the dashboard).
        CliRoute {
            path: "profile use",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "profile list",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "profile add",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 06-03: the logs family. `logs` is the BARE form (LogsArgs is
        // Option<LogsCmd> at cli.rs — `follow` is a FLAG on it, not a
        // subcommand; there is NO `logs follow` leaf) — the tail screen
        // IS bare `ign logs`. `logs download` and the loggers subtree
        // map to the same screen (download/loggers run from the
        // screen's actions menu; the registry's Streamed kind stays
        // reserved for raw-pane cases like rig logs, 06-06).
        CliRoute {
            path: "logs",
            mapping: Mapping::Screen(Screen::Logs),
        },
        CliRoute {
            path: "logs download",
            mapping: Mapping::Screen(Screen::Logs),
        },
        CliRoute {
            path: "logs loggers",
            mapping: Mapping::Screen(Screen::Logs),
        },
        CliRoute {
            path: "logs loggers set",
            mapping: Mapping::Screen(Screen::Logs),
        },
        CliRoute {
            path: "logs loggers reset",
            mapping: Mapping::Screen(Screen::Logs),
        },
        // 06-03: the tags-alarms family — exact TagsAlarmsCommand
        // spellings (active / history / ack), all on the Alarms screen
        // (the 5 s poll IS active; history rides `h`; ack rides `a`).
        CliRoute {
            path: "tags alarms active",
            mapping: Mapping::Screen(Screen::Alarms),
        },
        CliRoute {
            path: "tags alarms history",
            mapping: Mapping::Screen(Screen::Alarms),
        },
        CliRoute {
            path: "tags alarms ack",
            mapping: Mapping::Screen(Screen::Alarms),
        },
        // 06-04: the tags family MINUS alarms (above) — every leaf
        // clap spells: the provider subtree (`tags provider
        // list|create|delete` — TagsProviderCommand), the top-level
        // browse/read/write/export/import leaves, the config subtree
        // (`tags config get|create|edit|delete`), the udt subtree
        // (`tags udt types|def`), and the historian leaf (`tags
        // history query`). `tags browse` IS the tree browser itself
        // and `tags read` IS the detail pane's on-demand read.
        //
        // OutOfBand note (06-06's rule): `tags export -o -` — the
        // FOURTH sanctioned stdout exception — is a FLAG VALUE on the
        // `tags export` leaf, NOT a distinct leaf. The leaf maps
        // Screen(Tags) (the TUI hosts the FILE-mode export; the
        // stdout pipe form stays CLI-only, hint-named in the export
        // form). The coverage test walks leaf PATHS only, so no
        // OutOfBand row exists for it — this comment IS the
        // documentation. Same convention, 07-04: `tags browse
        // --from-export` is a FLAG on the `tags browse` leaf (the
        // offline form stays CLI-only — no separate row).
        CliRoute {
            path: "tags provider list",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags provider create",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags provider delete",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags browse",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags read",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags write",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags config get",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags config create",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags config edit",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags config delete",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags udt types",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags udt def",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags export",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags import",
            mapping: Mapping::Screen(Screen::Tags),
        },
        CliRoute {
            path: "tags history query",
            mapping: Mapping::Screen(Screen::Tags),
        },
        // 06-05: the project/resource/webdev families — every leaf
        // exactly as clap spells it (ProjectCommand: list/new/copy/
        // rename/set/delete/export/import; ResourceCommand: list/get/
        // put/delete; WebdevCommand: deploy/status). `project list`
        // IS the Projects screen's table; `resource list`/`resource
        // get` are the detail drill-down; the act verbs ride the
        // `a` actions menu (guarded ones Confirm-gated, webdev
        // deploy deliberately ungated — the 05-03 decision).
        CliRoute {
            path: "project list",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project new",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project copy",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project rename",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project set",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project delete",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project export",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project import",
            mapping: Mapping::Screen(Screen::Projects),
        },
        // 07-01: the cross-gateway pair joins the Projects family —
        // diff is a read (chained two-profile input form); sync is
        // Confirm-gated (its `--yes` mirror). Both rebuild per-side
        // clients from the named profiles inside their workers.
        CliRoute {
            path: "project diff",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "project sync",
            mapping: Mapping::Screen(Screen::Projects),
        },
        // 13-07: the workspace family (checkout/status/push) — the
        // local edit loop over the project-export interchange. Normal
        // envelope verbs, NOT OutOfBand: the status table and the
        // guarded-push refusals ARE the product (agent-facing), the
        // same genre as `project diff`/`project sync`. Hosted on the
        // Projects screen beside the family they compose with
        // (checkout rides the project export; push rides the
        // overwrite import). Rows land in the SAME task as their clap
        // commands (Pitfall 5: a command family is atomic).
        CliRoute {
            path: "workspace checkout",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "workspace status",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "workspace push",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "resource list",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "resource get",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "resource put",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "resource delete",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "webdev deploy",
            mapping: Mapping::Screen(Screen::Projects),
        },
        CliRoute {
            path: "webdev status",
            mapping: Mapping::Screen(Screen::Projects),
        },
        // 06-06: the rig family — every RigCommand/TrialCommand leaf
        // exactly as clap spells it (there is no bare `rig` row:
        // RigArgs.command is required + arg_required_else_help; no
        // bare `rig trial` row either: TrialArgs.command is required
        // — only the children map). `rig logs` is THE raw-pane
        // Streamed case the Mapping kind exists for (compose
        // passthrough shown in-screen); every other verb lives on the
        // Rig screen — up/down/status/logs/trial status/snapshot
        // fire direct, reset/restore/trial reset Confirm-gated
        // (main.rs's `require_confirmation` set EXACTLY — in
        // particular `down` is deliberately UNGUARDED: compose down
        // keeps volumes).
        //
        // Out-of-band note (06-06's rule, the STATE list's traceable
        // tail): the flag-value/stream-form stdout exceptions —
        // `logs -f` NDJSON (a FLAG on the Screen-mapped `logs` leaf)
        // and `tags export -o -` (a FLAG VALUE on the Screen-mapped
        // `tags export` leaf) — are NOT distinct leaves and carry no
        // rows; the leaf-representable exceptions are `completions`,
        // `api call` (since 09-03), `edit` (since 13-08), `mcp`
        // (since 14-01), and `lsp` (since 14-03 — the pinned
        // OutOfBand test carries all five).
        //
        // Reserved OutOfBand slugs (08-06): `mcp` and `lsp` were
        // PRE-DECLARED for Phase 14 — MCP stdio and LSP speak their
        // own protocols on stdout (a cockpit would fight them for the
        // terminal). `edit` joined the set in 13-08 and `mcp` in
        // 14-01, each row landing TOGETHER with its clap command;
        // `lsp` joined in 14-03 the same way — the last 08-06
        // reservation is FULFILLED and the set is closed. Rows added
        // before their commands exist are orphans and the clap walk
        // refuses them by design — this comment plus the pinned
        // OutOfBand test ARE the pre-declaration.
        CliRoute {
            path: "rig up",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig down",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig reset",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig status",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig logs",
            mapping: Mapping::Streamed,
        },
        CliRoute {
            path: "rig trial status",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig trial reset",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig snapshot",
            mapping: Mapping::Screen(Screen::Rig),
        },
        CliRoute {
            path: "rig restore",
            mapping: Mapping::Screen(Screen::Rig),
        },
        // 07-02: the standalone backup pair joins the dashboard's
        // global verbs (gateway-level — the restart/doctor host).
        // Download fires direct (a streamed read); restore is
        // Confirm-gated (the 8th --yes-guarded CLI verb's mirror).
        CliRoute {
            path: "backup download",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "backup restore",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 07-02: the EAM read pair rides the dashboard's global
        // actions menu (results via the shared Result modal — no
        // dedicated screen); `eam tasks <NAME>` is the SAME leaf
        // (an Option positional on the tasks form).
        CliRoute {
            path: "eam history",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "eam tasks",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 07-02 Task 3: the guarded writes (there is no bare `eam
        // task` row — EamTaskCommand is required, the `rig trial`
        // shape). `new` walks the chained dashboard form; `force` is
        // Confirm-gated per the CLI's guard set.
        CliRoute {
            path: "eam task new",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "eam task force",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 10-04: the guarded lifecycle/mutation verbs — Dashboard
        // rows beside the rest of the eam family. Each walks an
        // input modal; the Confirm gate arms AFTER the blast-radius
        // preview fetch lands (the Confirm BODY is the preview) —
        // Confirm-gated per the CLI's --yes set, exactly like force.
        CliRoute {
            path: "eam task suspend",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "eam task resume",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "eam task cancel",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "eam task modify",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        CliRoute {
            path: "eam task delete",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 07-03: `ign script run` (SCRPT-01) — the row lands FRESH
        // in this plan (grep-verified: no pre-existing row). There
        // is no bare `script` row (ScriptCommand is required, the
        // `rig trial` shape); the verb rides the dashboard's global
        // actions menu — an Input modal (code-only; the TUI refuses
        // the --file/stdin forms per the crossterm raw-input rule),
        // UNGATED (CLI parity — no --yes exists on script run: the
        // deploy flag IS the opt-in).
        CliRoute {
            path: "script run",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // QUICK-p0g: `ign testing run` — the gateway test-suite verb.
        // The row lands FRESH in this change. Deliberately NOT a
        // dashboard menu action: it is an agent/CI verb whose product
        // is a results DOCUMENT (counts, per-module results, an
        // optional rendered report), not a modal round trip — a
        // one-line result modal would lose exactly what the verb is
        // for. Wiring it into ACTIONS behind a dedicated results pane
        // is a clean follow-up.
        CliRoute {
            path: "testing run",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // QUICK-tg4: `ign e2e doctor` — the browser-E2E diagnosis. The
        // row lands FRESH in this change. Deliberately NOT a dashboard
        // menu action, for the same reason `testing run` is excluded:
        // its product is a multi-row diagnosis DOCUMENT (six rows,
        // each with a detail and a hint), and a one-line result modal
        // would discard exactly the part a user came for. The
        // dashboard's own doctor pane is where this belongs when a
        // pane exists to host it.
        CliRoute {
            path: "e2e doctor",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // QUICK-tg4: `ign session login` — the live gateway session.
        // The row lands FRESH in this change. Deliberately NOT a
        // dashboard menu action: the verb's product IS a live
        // credential (session cookie value + CSRF token), and a TUI
        // modal has nowhere safe to put one — it would render the
        // secret into a terminal frame that scrollback, screen
        // recordings, and tmux capture all keep. The verb exists to
        // hand JSON to a machine (a Playwright `globalSetup`), so the
        // CLI's --json path is its whole surface.
        CliRoute {
            path: "session login",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
        // 07-04: `ign lint` — the local delegation (no gateway: the
        // worker needs NO client). Ungated, unstrict (the doctor
        // posture IS the TUI display contract — findings land in the
        // result modal as data); `--strict` and `--` passthrough
        // stay CLI forms (`?`-named in the input modal).
        CliRoute {
            path: "lint",
            mapping: Mapping::Screen(Screen::Dashboard),
        },
    ]
}

/// The single registry-path → menu-prose alias seam — 06-10's
/// display-prose rule made structural. The wait trio reads like a
/// restart variant in menu prose ("wait for restart complete"), so
/// those three paths carry display labels here; EVERY other path
/// returns `None`, meaning the Actions-menu label IS the clap-exact
/// path (the 09-08 Phase 9 entries included). The routes↔menu
/// parity test in ignition-cli's tui_coverage.rs keys off this
/// function in BOTH directions — a new display-prose label must be
/// declared here in the same change as its menu entry, or CI fails.
pub fn menu_label(path: &str) -> Option<&'static str> {
    match path {
        "wait gateway" => Some("wait for gateway up"),
        "wait restart" => Some("wait for restart complete"),
        "wait module" => Some("wait for module ready"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Mapping, routes};
    use crate::state::Screen;

    /// The scaffold compiles with all Mapping kinds represented and
    /// rows are unique; the dashboard's 06-02 families are present.
    #[test]
    fn routes_scaffold_has_unique_paths_and_all_mapping_kinds() {
        let routes = routes();
        assert!(routes.len() >= 2, "seed rows present");

        let mut paths: Vec<&str> = routes.iter().map(|route| route.path).collect();
        let count = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), count, "duplicate route paths are forbidden");

        assert!(
            routes
                .iter()
                .any(|route| matches!(route.mapping, Mapping::Screen(Screen::Dashboard)))
        );
        assert!(
            routes
                .iter()
                .any(|route| matches!(route.mapping, Mapping::OutOfBand))
        );
    }

    /// The 06-02 dashboard rows exist with the clap-true leaf spellings
    /// (bare `sessions` is the list; the terminate subcommand rides it).
    #[test]
    fn dashboard_rows_cover_the_06_02_families() {
        let paths: Vec<&str> = routes().iter().map(|route| route.path).collect();
        for expected in [
            "version",
            "status",
            "modules",
            "metrics",
            "connections",
            "sessions",
            "sessions terminate",
            "wait gateway",
            "wait restart",
            "wait module",
            "doctor",
            "restart",
        ] {
            assert!(
                paths.contains(&expected),
                "dashboard route row {expected:?} missing"
            );
        }
    }

    /// The 06-03 logs rows cover the FULL LogsCmd/LoggersCmd tree as
    /// clap spells it: bare `logs` (the tail screen), `logs download`,
    /// bare `logs loggers` (the list), and the `set`/`reset` leaves —
    /// all on the Logs screen. There is no `logs follow` leaf (a
    /// flag, not a subcommand) and no `logs loggers get` leaf (the
    /// bare form IS the list) — the walk is exhaustive over the
    /// leaves that exist.
    #[test]
    fn logs_rows_cover_the_06_03_family() {
        let rows: Vec<&super::CliRoute> = routes()
            .iter()
            .filter(|route| route.path.starts_with("logs"))
            .collect();
        let expected = [
            ("logs", Screen::Logs),
            ("logs download", Screen::Logs),
            ("logs loggers", Screen::Logs),
            ("logs loggers set", Screen::Logs),
            ("logs loggers reset", Screen::Logs),
        ];
        assert_eq!(
            rows.len(),
            expected.len(),
            "exactly the logs leaves that exist: {rows:?}"
        );
        for (path, screen) in expected {
            let row = rows
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("logs route row {path:?} missing"));
            assert!(
                matches!(row.mapping, super::Mapping::Screen(s) if s == screen),
                "{path} maps to {screen:?}"
            );
        }
    }

    /// The 06-03 tags-alarms rows carry the exact TagsAlarmsCommand
    /// spellings onto the Alarms screen.
    #[test]
    fn alarms_rows_cover_the_tags_alarms_family() {
        let expected = [
            ("tags alarms active", Screen::Alarms),
            ("tags alarms history", Screen::Alarms),
            ("tags alarms ack", Screen::Alarms),
        ];
        for (path, screen) in expected {
            let row = routes()
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("alarms route row {path:?} missing"));
            assert!(
                matches!(row.mapping, super::Mapping::Screen(s) if s == screen),
                "{path} maps to {screen:?}"
            );
        }
    }

    /// The 06-04 rows cover EVERY non-alarm tags leaf exactly as
    /// clap spells it (TagsProviderCommand/TagsConfigCommand/
    /// TagsUdtCommand/TagsHistoryCommand + the top-level leaves) —
    /// the family's registry completeness (the largest CLI family).
    /// There is no bare `tags` leaf (command is required) and no
    /// `tags export -o -` leaf (a FLAG VALUE, not a subcommand —
    /// documented at the row).
    #[test]
    fn tags_rows_cover_every_non_alarm_leaf() {
        let expected = [
            "tags provider list",
            "tags provider create",
            "tags provider delete",
            "tags browse",
            "tags read",
            "tags write",
            "tags config get",
            "tags config create",
            "tags config edit",
            "tags config delete",
            "tags udt types",
            "tags udt def",
            "tags export",
            "tags import",
            "tags history query",
        ];
        let rows: Vec<&super::CliRoute> = routes()
            .iter()
            .filter(|route| route.path.starts_with("tags"))
            .collect();
        assert_eq!(
            rows.len(),
            expected.len() + 3,
            "the tags rows are exactly the non-alarm leaves + the three 06-03 alarms rows: {rows:?}"
        );
        for path in expected {
            let row = rows
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("tags route row {path:?} missing"));
            assert!(
                matches!(row.mapping, super::Mapping::Screen(s) if s == Screen::Tags),
                "{path} maps to the Tags screen"
            );
        }
    }

    /// The 06-05 rows cover EVERY project/resource/webdev leaf
    /// exactly as clap spells it (ProjectCommand/ResourceCommand/
    /// WebdevCommand — command is required on all three, so there is
    /// no bare `project`/`resource`/`webdev` leaf) — the family
    /// completeness that feeds 06-06's clap-walk coverage test.
    /// 07-01 adds the `project diff` leaf (the cross-gateway read)
    /// and `project sync` (the guarded promotion).
    #[test]
    fn project_resource_webdev_rows_cover_every_leaf() {
        let expected = [
            ("project list", Screen::Projects),
            ("project new", Screen::Projects),
            ("project copy", Screen::Projects),
            ("project rename", Screen::Projects),
            ("project set", Screen::Projects),
            ("project delete", Screen::Projects),
            ("project export", Screen::Projects),
            ("project import", Screen::Projects),
            ("project diff", Screen::Projects),
            ("project sync", Screen::Projects),
            ("resource list", Screen::Projects),
            ("resource get", Screen::Projects),
            ("resource put", Screen::Projects),
            ("resource delete", Screen::Projects),
            ("webdev deploy", Screen::Projects),
            ("webdev status", Screen::Projects),
        ];
        for (path, screen) in expected {
            let row = routes()
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("projects route row {path:?} missing"));
            assert!(
                matches!(row.mapping, super::Mapping::Screen(s) if s == screen),
                "{path} maps to {screen:?}"
            );
        } // And exactly those rows exist (no extras under the three
        // family prefixes).
        for prefix in ["project", "resource", "webdev"] {
            let count = routes()
                .iter()
                .filter(|route| route.path.starts_with(prefix))
                .count();
            let expected_count = expected
                .iter()
                .filter(|(path, _)| path.starts_with(prefix))
                .count();
            assert_eq!(
                count, expected_count,
                "exactly the {prefix} leaves that exist"
            );
        }
    }

    /// The 09-05 diagnostics-bundle rows cover the FULL
    /// BundleCommand tree exactly as clap spells it (there is no bare
    /// `diagnostics`/`diagnostics bundle` row — both levels require
    /// their subcommand, the `rig trial` shape), all on the Dashboard
    /// (decision 3: no new screen this phase).
    #[test]
    fn diagnostics_rows_cover_the_bundle_family() {
        let rows: Vec<&super::CliRoute> = routes()
            .iter()
            .filter(|route| route.path.starts_with("diagnostics"))
            .collect();
        let expected = [
            "diagnostics bundle generate",
            "diagnostics bundle status",
            "diagnostics bundle download",
            "diagnostics bundle wait",
        ];
        assert_eq!(
            rows.len(),
            expected.len(),
            "exactly the bundle leaves that exist: {rows:?}"
        );
        for path in expected {
            let row = rows
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("diagnostics route row {path:?} missing"));
            assert!(
                matches!(row.mapping, super::Mapping::Screen(s) if s == Screen::Dashboard),
                "{path} maps to the Dashboard screen"
            );
        }
    }

    /// The 06-06 rows cover EVERY rig leaf exactly as clap spells it
    /// (RigCommand + the nested TrialCommand — both require their
    /// subcommand, so no bare `rig`/`rig trial` row exists) — the
    /// final family, completing the registry for the clap-walk
    /// coverage test.
    #[test]
    fn rig_rows_cover_every_leaf() {
        let expected = [
            ("rig up", Screen::Rig),
            ("rig down", Screen::Rig),
            ("rig reset", Screen::Rig),
            ("rig status", Screen::Rig),
            ("rig trial status", Screen::Rig),
            ("rig trial reset", Screen::Rig),
            ("rig snapshot", Screen::Rig),
            ("rig restore", Screen::Rig),
        ];
        for (path, screen) in expected {
            let row = routes()
                .iter()
                .find(|route| route.path == path)
                .unwrap_or_else(|| panic!("rig route row {path:?} missing"));
            assert!(
                matches!(row.mapping, super::Mapping::Screen(s) if s == screen),
                "{path} maps to {screen:?}"
            );
        }
        // And `rig logs` is the Streamed raw-pane case — the Mapping
        // kind's reason to exist.
        let logs = routes()
            .iter()
            .find(|route| route.path == "rig logs")
            .unwrap_or_else(|| panic!("rig logs route row missing"));
        assert_eq!(logs.mapping, super::Mapping::Streamed);
        // Exactly the nine rig rows exist (no extras).
        let count = routes()
            .iter()
            .filter(|route| route.path.starts_with("rig"))
            .count();
        assert_eq!(count, 9, "exactly the rig leaves that exist");
    }
}
