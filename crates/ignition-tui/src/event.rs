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
}

/// Worker shutdown convention: the receiving half of a `watch<bool>`
/// channel — a `true` send means stop. Workers select their gateway work
/// against `shutdown.changed()`; zero new deps (research Pattern 2b —
/// the watch-channel answer, not CancellationToken).
pub type Shutdown = tokio::sync::watch::Receiver<bool>;
