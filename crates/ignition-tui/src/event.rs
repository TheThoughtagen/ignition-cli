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
}

/// Worker shutdown convention: the receiving half of a `watch<bool>`
/// channel — a `true` send means stop. Workers select their gateway work
/// against `shutdown.changed()`; zero new deps (research Pattern 2b —
/// the watch-channel answer, not CancellationToken).
pub type Shutdown = tokio::sync::watch::Receiver<bool>;
