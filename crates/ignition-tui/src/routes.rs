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
    ]
}

#[cfg(test)]
mod tests {
    use super::{Mapping, routes};
    use crate::state::Screen;

    /// The scaffold compiles with all Mapping kinds represented and
    /// rows are unique.
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
}
