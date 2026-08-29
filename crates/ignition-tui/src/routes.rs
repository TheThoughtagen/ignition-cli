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
        // documentation.
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
        // rows; the leaf-representable exception is `completions`
        // ONLY (the coverage test's OutOfBand sanity pin).
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
