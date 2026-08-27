//! The cockpit's event vocabulary (Phase 6 research, Pattern 1).
//!
//! One enum, one channel: everything that can mutate [`crate::state`]
//! arrives as an [`AppEvent`] on the mpsc receiver the select loop owns.
//! Later screen plans (06-02..06-06) ADD payload variants here (Refresh
//! snapshot, LogLine, TagWatch, Alarms, ProfileChanged, ActionDone) —
//! the shell defines only what it needs.

use crossterm::event::Event;

/// Everything the select loop can feed to [`crate::update`].
#[derive(Debug)]
pub enum AppEvent {
    /// A raw crossterm terminal event forwarded by the select loop.
    Input(Event),
    /// The 250 ms staleness floor — the loop also draws per-event; the
    /// tick guarantees a redraw even when nothing else fires.
    Tick,
    /// A worker-reported failure (workers must send this instead of
    /// panicking — a panic inside `tokio::spawn` never unwinds the loop).
    Error(String),
    /// One streamed log entry from the tail worker (06-03). NOT
    /// era-stamped — the ring's turnover is the acceptance policy (the
    /// plan-locked decision: per-line eras buy nothing; the worker is
    /// stopped by its shutdown watch on screen exit / profile switch,
    /// and any last-gasp lines merely join the ring).
    LogLine(ignition_core::client::logs::LogEntry),
    /// A dashboard refresh snapshot (06-02). `era` is the worker's
    /// spawn-era; update drops stale-era events (a profile switch
    /// invalidates in-flight snapshots — Pitfall 9: no data from the
    /// previous profile ever lands). The snapshot is boxed — the panel
    /// payloads dwarf every other variant and the event moves through
    /// the channel on every refresh.
    Refresh {
        /// Era the worker was spawned under.
        era: u64,
        /// Per-panel data + per-panel errors (one failing endpoint
        /// degrades its panel only — never the whole dashboard).
        snapshot: Box<crate::workers::refresh::Snapshot>,
    },
    /// A one-shot dashboard action finished (06-02). The worker already
    /// serialized the typed result (`serde_json::to_string_pretty`) or
    /// the error's display string — the result modal renders it
    /// verbatim (the LOCKED one-mechanism result display).
    ActionDone {
        /// Era the worker was spawned under.
        era: u64,
        /// The menu label ("wait gateway") — the modal title.
        label: &'static str,
        /// Pretty JSON on success, the error message on failure.
        result: Result<String, String>,
    },
    /// A profile switch landed (06-02): the switch itself ran
    /// synchronously in update (all-local file I/O); this event is the
    /// era-gated confirmation that drives the status-line banner. Stale
    /// banners are dropped (Pitfall 9).
    ProfileChanged {
        /// Era of the NEW world (the switch already bumped the counter).
        era: u64,
        /// The newly active profile's name.
        name: String,
    },
    /// One alarms poll result (06-03): the active-alarm table's whole
    /// world. `Ok` replaces the table; `Err` degrades to the honest
    /// error state (data dropped — the poll-error answer to a dead or
    /// route-less gateway, per the per-panel degrade convention). Stale
    /// eras drop whole (Pitfall 9).
    Alarms {
        /// Era the worker was spawned under.
        era: u64,
        /// The active rows (gateway order), or the poll's error.
        result: Result<Vec<ignition_core::actions::tags::AlarmRow>, String>,
    },
    /// The Tags screen's provider list landed (06-04): `Ok` replaces
    /// the provider table, `Err` degrades to the honest error state.
    /// Stale eras drop whole (Pitfall 9).
    TagsProviders {
        /// Era the worker was spawned under.
        era: u64,
        /// The provider rows (gateway order), or the load's error.
        result: Result<Vec<ignition_core::actions::tags::TagProviderRow>, String>,
    },
    /// One tag-tree browse landed (06-04): `path` names the stack
    /// level the rows belong to (a popped level's late result drops
    /// whole — the level lookup IS the stale gate). Stale eras drop.
    TagsBrowse {
        /// Era the worker was spawned under.
        era: u64,
        /// The browse path this result fills (the level's identity).
        path: String,
        /// The level's entries, or the browse's error.
        result: Result<Vec<ignition_core::actions::tags::BrowseRow>, String>,
    },
    /// The detail pane's on-demand read (06-04). `seq` is the
    /// detail-open's request-id — a read for a pane the user already
    /// left (or replaced) drops; the era gates profile switches on
    /// top of it.
    TagDetailRead {
        /// Era the worker was spawned under.
        era: u64,
        /// The detail-open sequence the read belongs to.
        seq: u64,
        /// The read row, or the read's error.
        result: Result<ignition_core::actions::tags::TagReadRow, String>,
    },
}

/// Worker shutdown convention: the receiving half of a `watch<bool>`
/// channel — a `true` send means stop. Workers select their gateway work
/// against `shutdown.changed()`; zero new deps (research Pattern 2b —
/// the watch-channel answer, not CancellationToken).
pub type Shutdown = tokio::sync::watch::Receiver<bool>;
