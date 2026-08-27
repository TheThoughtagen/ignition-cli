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
}
