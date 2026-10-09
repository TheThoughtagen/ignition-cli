//! The itop event vocabulary — the standalone monitor's Elm-style
//! inputs (Phase 6's [`crate::event::AppEvent`] shape, scoped to itop).
//!
//! One enum, one channel: everything that can mutate
//! [`crate::itop::TopState`] arrives as a [`TopEvent`] on the mpsc
//! receiver the select loop owns. The deliberate DIFFERENCE from the
//! cockpit: itop is a single-view app, so there is no per-screen
//! routing here — input, tick, sample, and op-result are the whole
//! world.

use crossterm::event::Event;

/// Everything the itop select loop can feed to
/// [`crate::itop::update`].
#[derive(Debug)]
pub enum TopEvent {
    /// A raw crossterm terminal event forwarded by the select loop.
    Input(Event),
    /// The redraw staleness floor (250 ms, the cockpit's TICK): the
    /// loop also draws per-event; the tick guarantees a redraw even
    /// when the gateway is quiet and no sample has landed.
    Tick,
    /// One composed gateway sample landed (the itop worker). `era` is
    /// the worker's spawn-era; update drops stale-era events (an
    /// interval-change respawn invalidates in-flight samples — the
    /// Pitfall-9 convention: no data from a dead world ever lands).
    /// The sample is boxed — the six-section payload dwarfs every
    /// other variant and moves through the channel on every tick.
    Sample {
        /// Era the worker was spawned under.
        era: u64,
        /// The composed sample (per-section data + per-section
        /// errors — one failing endpoint degrades its section only,
        /// never the whole sample).
        sample: Box<crate::itop::worker::TopSample>,
    },
    /// A one-shot session kill finished (the terminate worker). The
    /// worker already rendered the typed result or the error's
    /// display string — the status line shows it verbatim (the
    /// cockpit's one-mechanism result display, scoped to a line).
    Killed {
        /// Era the op was spawned under (stale ops drop).
        era: u64,
        /// The row's display label ("perspective admin@…") — the
        /// status line names the target either way.
        label: String,
        /// Ok with the action's own summary, Err with the error
        /// message.
        result: Result<String, String>,
    },
    /// The scriptExec diagnostic probe (`e`) finished — one gateway-
    /// side Jython snapshot (JMX internals no REST endpoint exposes)
    /// through the deployed route, or the honest refusal (route not
    /// configured / secret rejected / route error). The modal renders
    /// it; the status line summarizes.
    ScriptProbe {
        /// Era the probe was spawned under (stale probes drop).
        era: u64,
        /// The probe's outcome.
        result: Result<Box<ignition_core::actions::script::ScriptRunResult>, String>,
    },
    /// The thread-diagnostics op (`6`) finished — the formatted dump
    /// and the deadlocked-id list, each degraded INDEPENDENTLY (a
    /// deadlocks failure never hides the dump). A one-shot op: it
    /// applies regardless of era (not sample data — the kill/probe
    /// convention).
    ThreadDiagnostics {
        /// When the op landed (the dump-age footer readout).
        at: std::time::Instant,
        /// The formatted dump; Err the honest failure.
        dump: Result<Box<ignition_core::client::threads::FormattedThreadDump>, String>,
        /// Deadlocked thread ids — `Ok(empty)` is the HEALTHY answer
        /// (the JVM's "no deadlocks"); Err the honest failure.
        deadlocked: Result<Vec<i64>, String>,
    },
}
