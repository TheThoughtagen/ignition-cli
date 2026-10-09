//! The itop Elm-style model — PURE DATA over the latest gateway
//! sample, plus the table configuration (family / sort / filter /
//! selection) and the htop-style behavior flags (pause, interval).
//!
//! Like the cockpit's [`crate::state`]: no I/O, no async, no
//! terminals — every mutation flows through [`crate::itop::update`];
//! workers never touch this struct directly. The worker RAILS (client
//! handle, event sender, shutdown/pause watches) ride in the state so
//! `update` can spawn the same way the cockpit's update does; outside
//! a tokio runtime the spawns stand alone and nothing starts (the
//! 06-02 spawn-guard convention).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ignition_core::actions::sessions::SessionType;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::threads::{FormattedThreadDump, ThreadInfo};
use tokio::sync::{mpsc, watch};

use crate::itop::event::TopEvent;
use crate::itop::worker::TopSample;
use crate::ui::theme::{Palette, Theme, Tier};

/// Every live entity itop can show as one table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A healthy-list module (`ign modules`).
    Module,
    /// An attached Designer session.
    Designer,
    /// A Perspective browser session.
    Perspective,
    /// A Vision client.
    Vision,
    /// A database connection.
    Database,
    /// An OPC connection.
    Opc,
    /// A tag provider.
    Provider,
    /// One JVM thread from the on-demand thread dump (`6`) — the
    /// Gateway web UI's Diagnostics→Threads page, not part of the
    /// sample cadence.
    Thread,
}

impl RowKind {
    /// The KIND column label — lowercase, htop-process-genre.
    pub fn label(self) -> &'static str {
        match self {
            RowKind::Module => "module",
            RowKind::Designer => "designer",
            RowKind::Perspective => "perspective",
            RowKind::Vision => "vision",
            RowKind::Database => "database",
            RowKind::Opc => "opc",
            RowKind::Provider => "provider",
            RowKind::Thread => "thread",
        }
    }

    /// The deterministic KIND sort rank (module-first, provider-last)
    /// — grouping families together is the default view, so the rank
    /// must be a stable authored order, never enum declaration order
    /// by accident (same discipline as `Screen::ALL`).
    pub fn rank(self) -> u8 {
        match self {
            RowKind::Module => 0,
            RowKind::Designer => 1,
            RowKind::Perspective => 2,
            RowKind::Vision => 3,
            RowKind::Database => 4,
            RowKind::Opc => 5,
            RowKind::Provider => 6,
            // LAST: threads are the on-demand view, never part of the
            // grouped default table.
            RowKind::Thread => 7,
        }
    }

    /// The session [`SessionType`] a row can be killed through —
    /// `None` for every non-session kind (the kill gate is HERE, one
    /// site, so the update layer can never arm a terminate against a
    /// module or connection).
    pub fn session_type(self) -> Option<SessionType> {
        match self {
            RowKind::Designer => Some(SessionType::Designer),
            RowKind::Perspective => Some(SessionType::Perspective),
            RowKind::Vision => Some(SessionType::Vision),
            RowKind::Module | RowKind::Database | RowKind::Opc | RowKind::Provider => None,
            RowKind::Thread => None,
        }
    }
}

/// One table row — the flattened, sortable view of any live entity.
/// Columns render verbatim from the fields; there is no per-kind
/// branching at render time.
// PartialEq only: thread rows carry an f64 CPU percent (no Eq) — no
// site needs Eq on a Row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The entity family (the KIND column).
    pub kind: RowKind,
    /// The NAME column — the human identifier (module name, session
    /// user, connection name, provider name, thread name).
    pub name: String,
    /// The STATE column — the gateway's state-ish field verbatim
    /// (module state, authorized, enabled, provider health, thread
    /// state); never an invented verdict.
    pub state: String,
    /// Session uptime in epoch MILLISECONDS where the gateway reports
    /// one (designers/vision) — `None` renders `—` and sorts last.
    pub age_ms: Option<i64>,
    /// The DETAIL column — kind-specific supporting facts (module
    /// version, session project/address, connection healthchecks,
    /// provider tag count, thread daemon/system/waiting-for marks).
    pub detail: String,
    /// CPU PERCENT for thread rows (the dump's `cpuUsage`, the web
    /// UI's CPU column) — `None` renders `—` and sorts last.
    pub cpu: Option<f64>,
    /// The JVM thread id (thread rows) — the deadlocked list's key.
    pub tid: Option<i64>,
    /// The daemon flag (thread rows).
    pub daemon: Option<bool>,
    /// Whether the JVM's deadlock detection names this thread (thread
    /// rows; the honest `false` elsewhere).
    pub deadlocked: bool,
}

/// Which entity families the table shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Family {
    /// Every row (the default).
    #[default]
    All,
    /// Modules only.
    Modules,
    /// The three session families.
    Sessions,
    /// Database + OPC connections.
    Connections,
    /// Tag providers.
    Providers,
    /// The JVM's live threads (the on-demand `6` dump — NOT the
    /// sample's thread-count mix bars).
    Threads,
}

impl Family {
    /// Number-key order 1..=6 (also the help text order).
    pub const ALL: [Family; 6] = [
        Family::All,
        Family::Modules,
        Family::Sessions,
        Family::Connections,
        Family::Providers,
        Family::Threads,
    ];

    /// The footer label.
    pub fn label(self) -> &'static str {
        match self {
            Family::All => "all",
            Family::Modules => "modules",
            Family::Sessions => "sessions",
            Family::Connections => "connections",
            Family::Providers => "providers",
            Family::Threads => "threads",
        }
    }

    /// Whether a row survives this family's filter.
    pub fn matches(self, kind: RowKind) -> bool {
        match self {
            Family::All => true,
            Family::Modules => kind == RowKind::Module,
            Family::Sessions => {
                matches!(
                    kind,
                    RowKind::Designer | RowKind::Perspective | RowKind::Vision
                )
            }
            Family::Connections => matches!(kind, RowKind::Database | RowKind::Opc),
            Family::Providers => kind == RowKind::Provider,
            Family::Threads => kind == RowKind::Thread,
        }
    }

    /// The next family in the 1..=6 cycle (wraps).
    pub fn next(self) -> Family {
        let all = &Self::ALL;
        let idx = all
            .iter()
            .position(|f| *f == self)
            .expect("every Family variant is in ALL");
        all[(idx + 1) % all.len()]
    }
}

/// The table sort key (`s` cycles, `S` reverses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// KIND rank then NAME (the default — grouped like the cockpit).
    #[default]
    Kind,
    /// NAME (case-insensitive).
    Name,
    /// STATE (case-insensitive; unknown states sort among themselves).
    State,
    /// AGE descending (longest-lived first — htop's TIME+ instinct);
    /// rows without an age sort last in either direction.
    Age,
    /// CPU PERCENT descending (the threads view's default instinct —
    /// the hot thread leads, exactly like the web UI's CPU column);
    /// rows without a cpu sort last in either direction.
    Cpu,
    /// DETAIL (case-insensitive).
    Detail,
}

impl SortKey {
    /// The `s` cycle order (also the help text order).
    pub const ALL: [SortKey; 6] = [
        SortKey::Kind,
        SortKey::Name,
        SortKey::State,
        SortKey::Age,
        SortKey::Cpu,
        SortKey::Detail,
    ];

    /// The footer label.
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Kind => "kind",
            SortKey::Name => "name",
            SortKey::State => "state",
            SortKey::Age => "age",
            SortKey::Cpu => "cpu",
            SortKey::Detail => "detail",
        }
    }

    /// The next sort key in the cycle (wraps).
    pub fn next(self) -> SortKey {
        let all = &Self::ALL;
        let idx = all
            .iter()
            .position(|k| *k == self)
            .expect("every SortKey variant is in ALL");
        all[(idx + 1) % all.len()]
    }
}

/// The state columns colored as healthy (`success` slot) — `runnable`
/// is the JVM thread-state green (the web UI's Diagnostics→Threads).
pub const HEALTHY_STATES: [&str; 8] = [
    "active",
    "connected",
    "authorized",
    "enabled",
    "ready",
    "running",
    "healthy",
    "runnable",
];

/// The state columns colored as failing (`error` slot).
pub const FAILING_STATES: [&str; 7] = [
    "unauthorized",
    "disabled",
    "error",
    "quarantined",
    "faulted",
    "blocked",
    "failed",
];

/// A pending session terminate staged behind the Confirm modal — the
/// TUI-side answer to `ign sessions terminate --yes` (the cockpit's
/// Confirm-modal convention, scoped to itop's kill key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillTarget {
    /// The session family the terminate route takes.
    pub kind: SessionType,
    /// The session id (designer id / perspective sessionId / vision
    /// client id) — never re-derived at op time.
    pub id: String,
    /// The row's display label for the status line ("perspective
    /// admin@whiskeyhouse").
    pub label: String,
}

/// The modal surfaces itop owns. Same three shapes as the cockpit's
/// Modal (Confirm / input / static) plus the probe result modal —
/// the script probe's outcome is a DOCUMENT (twelve JMX rows), so it
/// renders as a modal, not a status line (the testing-run exclusion's
/// rule: a result document never shrinks to one line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// The `?`/F1 keymap overlay.
    Help,
    /// The `/` filter prompt — Enter applies (lowercased substring),
    /// Esc closes keeping the active filter.
    Filter {
        /// The edited buffer.
        buffer: String,
    },
    /// Yes/no confirmation for a session kill. `y` spawns the
    /// terminate op, Esc cancels.
    ConfirmKill {
        /// The staged terminate target.
        target: KillTarget,
    },
    /// The `e` scriptExec probe's result — Ok carries the pretty
    /// JSON document (JMX internals via gateway-side Jython), Err the
    /// honest refusal (route not configured / rejected / route error).
    ScriptProbe {
        /// Ok: the rendered document lines. Err: the refusal text.
        result: Result<String, String>,
    },
    /// The thread stack viewer (Enter on a thread row) — the Gateway
    /// web UI's per-thread stack pane. SCROLLABLE (a real stack
    /// outgrows any modal): j/k/arrows/pgup/pgdn move the offset,
    /// Esc/Enter closes. The lines freeze at open time — the dump is
    /// a snapshot, and the modal must show what the user read.
    ThreadStack {
        /// The thread's display name (the modal title).
        name: String,
        /// The rendered lines (header facts, hold/wait annotations,
        /// then the stack frames verbatim).
        lines: Vec<String>,
        /// The scroll offset (top visible line).
        scroll: usize,
    },
}

/// The thread-diagnostics snapshot — the `6` op's landing. Each call
/// degrades independently: a deadlocks failure never hides the dump,
/// a dump failure never hides the honest error.
#[derive(Debug, Clone)]
pub struct ThreadSnapshot {
    /// When the op landed (the dump-age footer readout).
    pub at: Instant,
    /// The formatted dump — `None` when the call failed (the error
    /// rides [`Self::dump_error`]).
    pub dump: Option<Box<FormattedThreadDump>>,
    /// Why the dump call failed, when it did.
    pub dump_error: Option<String>,
    /// Deadlocked thread ids — `Some(empty)` is the HEALTHY answer
    /// (the JVM's "no deadlocks"); `None` the call failed.
    pub deadlocked: Option<Vec<i64>>,
    /// Why the deadlocks call failed, when it did.
    pub deadlocks_error: Option<String>,
}

/// The DEFAULT sample cadence — itop's own, deliberately FASTER than
/// the cockpit's 5 s dashboard period (htop's identity is a live
/// monitor; 2 s reads as live without hammering the gateway with the
/// eight-call sample). Not the profile's `poll_interval_secs`: that
/// knob is documented as the dashboard cadence, and silently
/// inheriting it here would couple two products' tuning. +/- adjusts;
/// 1..=60 clamps.
pub const DEFAULT_INTERVAL_SECS: u64 = 2;

/// The sample-cadence floor and ceiling (seconds).
pub const MIN_INTERVAL_SECS: u64 = 1;
pub const MAX_INTERVAL_SECS: u64 = 60;

/// The history ring capacity — 240 samples ≈ 8 minutes at the default
/// 2 s cadence, the sparkline window. Sized once; the ring is the
/// only history itop keeps (no charts-endpoint bootstrap: the local
/// ring fills within minutes and htop bootstraps nothing).
pub const RING_CAPACITY: usize = 240;

/// The itop state. `update` is the ONLY writer; `ui` is a pure reader.
pub struct TopState {
    // ── Worker rails (the cockpit's AppState convention) ──────────
    /// The authed client (Session-constructed) — `None` only before
    /// run_loop arms the world.
    pub client: Option<Arc<ReqwestGatewayApi>>,
    /// The event sender the workers answer on.
    pub events_tx: Option<mpsc::UnboundedSender<TopEvent>>,
    /// The sample worker's shutdown watch (interval-change respawn).
    pub sample_shutdown: Option<watch::Sender<bool>>,
    /// The pause watch the worker reads each tick (pause without a
    /// respawn — flip the sender, the worker skips sampling).
    pub paused_tx: Option<watch::Sender<bool>>,
    /// The spawn-era counter (interval-change respawns bump it; stale
    /// samples drop).
    pub era: u64,

    // ── World identity ─────────────────────────────────────────────
    /// The resolved profile's name (footer).
    pub profile_name: Option<String>,
    /// The profile's URL (footer).
    pub profile_url: Option<String>,
    /// The resolved theme palette (context resolution; the render is
    /// slot-colored, never raw-colored).
    pub palette: Palette,

    // ── Data ───────────────────────────────────────────────────────
    /// The latest composed sample (per-section data + errors).
    pub last: Option<Box<TopSample>>,
    /// When the latest sample landed (the staleness readout).
    pub last_at: Option<Instant>,
    /// The one-shot refresh busy guard (keystrokes cannot stack
    /// samples; cleared when the next current-era Sample lands).
    pub refresh_busy: bool,
    /// CPU percent ring (the gauges endpoint's PERCENT scale).
    pub cpu_ring: VecDeque<f64>,
    /// Heap bytes ring.
    pub heap_ring: VecDeque<f64>,
    /// Non-heap bytes ring — fed ONLY by the charts bootstrap (the
    /// gateway's own history; the gauges endpoint exposes no non-heap
    /// gauge), so it renders the gateway-side trend, not the local one.
    pub nonheap_ring: VecDeque<f64>,
    /// Total thread-count ring (running + waiting + timed + blocked).
    pub thread_ring: VecDeque<u64>,
    /// Whether the charts bootstrap has landed (rings seeded once per
    /// world — a respawn re-seeds, a resume does not).
    pub history_seeded: bool,
    /// The thread-diagnostics snapshot (`6`) — ON-DEMAND, never part
    /// of the sample cadence (a hundreds-of-threads dump every 2 s
    /// would hammer the gateway; the web UI fetches on click, and so
    /// does itop).
    pub thread_dump: Option<Box<ThreadSnapshot>>,
    /// The thread-dump op's busy guard (keystrokes cannot stack
    /// dumps; cleared when the ThreadDiagnostics event lands —
    /// regardless of era, the one-shot convention).
    pub threads_busy: bool,

    // ── Table configuration ────────────────────────────────────────
    /// The active family filter.
    pub family: Family,
    /// The active sort key.
    pub sort_key: SortKey,
    /// Whether the sort is reversed.
    pub sort_rev: bool,
    /// The active substring filter (lowercased at apply time).
    pub filter: Option<String>,
    /// The selected row index into the VISIBLE list (clamped on every
    /// change — a filter/family flip can never strand it).
    pub selected: usize,

    // ── Behavior ───────────────────────────────────────────────────
    /// Whether sampling is paused (worker skips; the rings freeze).
    pub paused: bool,
    /// The sample cadence in seconds (clamped 1..=60).
    pub interval_secs: u64,
    /// The scriptExec probe's busy guard (keystrokes cannot stack
    /// probes; cleared when the current-era ScriptProbe lands).
    pub probe_busy: bool,
    /// The most recent probe outcome — `Ok` is the pretty JSON
    /// document, `Err` the honest refusal text. The `e` modal re-renders
    /// it until the next probe replaces it.
    pub script_result: Option<Result<String, String>>,

    // ── Chrome ─────────────────────────────────────────────────────
    /// The open modal, if any (modal keystrokes win — the cockpit's
    /// Focus::Modal arbitration, folded into one Option here).
    pub modal: Option<Modal>,
    /// The transient status line (op results, kill confirmations) —
    /// `is_error` colors it; a Sample arrival does NOT clear it (the
    /// live error readout rides the sample sections, not this line).
    pub status_msg: Option<(String, bool)>,
    /// Whether the loop should quit.
    pub should_quit: bool,
}

impl Default for TopState {
    fn default() -> Self {
        TopState {
            client: None,
            events_tx: None,
            sample_shutdown: None,
            paused_tx: None,
            era: 0,
            profile_name: None,
            profile_url: None,
            // The strictest authored tier — the cockpit AppState
            // Default convention (anything that reads wrong at Mono
            // is a bug, not a cosmetic nit).
            palette: Theme::by_name("default")
                .expect("default theme exists")
                .resolve(Tier::default()),
            last: None,
            last_at: None,
            refresh_busy: false,
            cpu_ring: VecDeque::with_capacity(RING_CAPACITY),
            heap_ring: VecDeque::with_capacity(RING_CAPACITY),
            nonheap_ring: VecDeque::with_capacity(RING_CAPACITY),
            thread_ring: VecDeque::with_capacity(RING_CAPACITY),
            history_seeded: false,
            thread_dump: None,
            threads_busy: false,
            family: Family::default(),
            sort_key: SortKey::default(),
            sort_rev: false,
            filter: None,
            selected: 0,
            paused: false,
            interval_secs: DEFAULT_INTERVAL_SECS,
            probe_busy: false,
            script_result: None,
            modal: None,
            status_msg: None,
            should_quit: false,
        }
    }
}

impl TopState {
    /// A fresh state with the world rails adopted in one block (the
    /// ResolvedContext adoption convention — run_loop's single
    /// assignment site).
    pub fn new(ctx: &crate::context::ResolvedContext) -> Self {
        TopState {
            client: Some(ctx.api.clone()),
            events_tx: None,
            sample_shutdown: None,
            paused_tx: None,
            era: 0,
            profile_name: Some(ctx.profile_name.clone()),
            profile_url: Some(ctx.profile_url.clone()),
            palette: ctx.palette,
            ..TopState::default()
        }
    }

    // ── Derivation (pure reads) ────────────────────────────────────

    /// The VISIBLE rows: the latest sample flattened, family-filtered,
    /// text-filtered, then sorted. The single derivation the table and
    /// the kill gate share (selection indexes into exactly this list).
    pub fn visible_rows(&self) -> Vec<Row> {
        // The threads view draws from the ON-DEMAND dump snapshot, not
        // the sample — the sample carries thread COUNTS (the mix
        // bars), never a per-thread list. Empty until `6` lands a
        // dump (the ui explains the empty state honestly).
        if self.family == Family::Threads {
            let Some(snapshot) = &self.thread_dump else {
                return Vec::new();
            };
            let Some(dump) = &snapshot.dump else {
                return Vec::new();
            };
            let deadlocked = snapshot.deadlocked.as_deref().unwrap_or(&[]);
            let mut rows = flatten_thread_dump(dump, deadlocked);
            if let Some(filter) = &self.filter {
                rows.retain(|row| row.matches_filter(filter));
            }
            let key = self.sort_key;
            let rev = self.sort_rev;
            rows.sort_by(|a, b| {
                let ord = key.compare(a, b);
                if rev { ord.reverse() } else { ord }
            });
            return rows;
        }
        let Some(sample) = self.last.as_deref() else {
            return Vec::new();
        };
        let mut rows = flatten_sample(sample);
        rows.retain(|row| self.family.matches(row.kind));
        if let Some(filter) = &self.filter {
            rows.retain(|row| row.matches_filter(filter));
        }
        let key = self.sort_key;
        let rev = self.sort_rev;
        rows.sort_by(|a, b| {
            let ord = key.compare(a, b);
            if rev { ord.reverse() } else { ord }
        });
        rows
    }

    /// How stale the latest sample is — `None` when nothing has ever
    /// landed (the footer's honest "no sample yet").
    pub fn staleness(&self, now: Instant) -> Option<Duration> {
        self.last_at.map(|at| now.saturating_duration_since(at))
    }

    /// The vitals pair (cpu percent, heap bytes) the header bars read
    /// — gauges first (the PERCENT home), heap falling back to the
    /// status overview's memory pair when the metrics section
    /// degraded. `None` when neither source has data (the bars render
    /// their error/empty state).
    pub fn vitals(&self) -> Option<(f64, f64, f64)> {
        let sample = self.last.as_deref()?;
        let cpu = sample.metrics.as_ref().map(|m| m.current.cpu).or_else(|| {
            sample
                .status
                .as_ref()
                .map(|s| s.overview.cpu_fraction * 100.0)
        });
        let heap = sample
            .metrics
            .as_ref()
            .map(|m| m.current.heap_memory)
            .or_else(|| {
                sample
                    .status
                    .as_ref()
                    .and_then(|s| s.overview.memory.first().copied())
                    .map(|used| used as f64)
            });
        let max = sample
            .metrics
            .as_ref()
            .map(|m| m.current.max_memory)
            .or_else(|| {
                sample
                    .status
                    .as_ref()
                    .and_then(|s| s.overview.memory.get(1).copied())
                    .map(|max| max as f64)
            });
        Some((cpu?, heap?, max?))
    }

    // ── Mutations (update is the only caller) ──────────────────────

    /// Adopt a fresh sample: store it, stamp it, feed the rings, clear
    /// the one-shot busy guard. Degraded sections feed no ring (a gap
    /// is honest; the ring just stops growing).
    pub fn apply_sample(&mut self, sample: Box<TopSample>, at: Instant) {
        // The charts bootstrap: an un-seeded world fills its rings from
        // the gateway's OWN history first (cpu percent / heap bytes /
        // non-heap bytes), so the sparklines open full instead of
        // growing from zero. Seeding happens once per world; the local
        // samples continue from there. Seeding runs BEFORE the live
        // push below so the current sample lands as the ring's newest
        // entry (push_capped would otherwise evict it at capacity and
        // the sparkline would render it at the oldest position).
        if let Some(history) = &sample.history
            && !self.history_seeded
        {
            for point in &history.cpu_datapoints {
                push_capped(&mut self.cpu_ring, point.value);
            }
            for point in &history.heap_memory_datapoints {
                push_capped(&mut self.heap_ring, point.value);
            }
            for point in &history.non_heap_memory_datapoints {
                push_capped(&mut self.nonheap_ring, point.value);
            }
            self.history_seeded = true;
        }
        if let Some(metrics) = &sample.metrics {
            push_capped(&mut self.cpu_ring, metrics.current.cpu);
            push_capped(&mut self.heap_ring, metrics.current.heap_memory);
            let threads = &metrics.threads;
            let total = (threads.running.max(0)
                + threads.waiting.max(0)
                + threads.timed_waiting.max(0)
                + threads.blocked.max(0)) as u64;
            push_capped(&mut self.thread_ring, total);
        }
        self.last = Some(sample);
        self.last_at = Some(at);
        self.refresh_busy = false;
    }

    /// Clamp-adjust the interval by `delta` seconds. Returns whether
    /// the value CHANGED (the respawn trigger — an unchanged clamp hit
    /// must not churn the worker).
    pub fn adjust_interval(&mut self, delta: i64) -> bool {
        let next = (self.interval_secs as i64 + delta)
            .clamp(MIN_INTERVAL_SECS as i64, MAX_INTERVAL_SECS as i64);
        let changed = next != self.interval_secs as i64;
        self.interval_secs = next as u64;
        changed
    }

    /// Toggle pause. Returns whether the flag CHANGED (the pause-watch
    /// sync trigger).
    pub fn toggle_pause(&mut self) -> bool {
        self.paused = !self.paused;
        true
    }

    /// Cycle the sort key (`s`).
    pub fn cycle_sort(&mut self) {
        self.sort_key = self.sort_key.next();
    }

    /// Toggle the sort direction (`S`).
    pub fn toggle_sort_dir(&mut self) {
        self.sort_rev = !self.sort_rev;
    }

    /// Cycle the family (Shift-F not bound — number keys jump
    /// directly; this exists for completeness and tests).
    pub fn cycle_family(&mut self) {
        self.family = self.family.next();
    }

    /// Apply a text filter (lowercased substring). An empty buffer
    /// clears.
    pub fn set_filter(&mut self, raw: &str) {
        let trimmed = raw.trim().to_lowercase();
        self.filter = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
        self.selected = 0;
    }

    /// Clamp the selection into the visible list's bounds.
    pub fn clamp_selection(&mut self) {
        let len = self.visible_rows().len();
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    /// Move the selection by `delta` rows (clamped).
    pub fn move_selection(&mut self, delta: i64) {
        let len = self.visible_rows().len() as i64;
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as i64 + delta).clamp(0, len - 1);
        self.selected = next as usize;
    }

    /// The kill target for the currently selected row — `None` when
    /// the list is empty, the selection is out of bounds, or the row
    /// is not a session (modules/connections/providers have no kill).
    pub fn armed_kill(&self) -> Option<KillTarget> {
        let rows = self.visible_rows();
        let row = rows.get(self.selected)?;
        let kind = row.kind.session_type()?;
        Some(KillTarget {
            kind,
            id: row_id(row).to_string(),
            label: format!("{} {}", row.kind.label(), row.name),
        })
    }

    /// The selected thread row's stack-viewer modal — `None` unless
    /// the threads view holds a dump AND the selected row is a thread
    /// (`armed_kill`'s shape for the Enter key). The lines freeze at
    /// open time: the modal shows the snapshot the user read, even if
    /// a later `6` re-dump lands underneath.
    pub fn armed_thread_stack(&self) -> Option<Modal> {
        let rows = self.visible_rows();
        let row = rows.get(self.selected)?;
        let tid = row.tid?;
        let snapshot = self.thread_dump.as_ref()?;
        let dump = snapshot.dump.as_ref()?;
        let thread = dump.threads.iter().find(|thread| thread.id == Some(tid))?;
        Some(Modal::ThreadStack {
            name: thread.name.clone(),
            lines: thread_stack_lines(thread, row.deadlocked),
            scroll: 0,
        })
    }
}

impl Row {
    /// The NON-thread row constructor — the five shared columns, the
    /// thread-only fields defaulted (one honest call site per family
    /// instead of four `None`/`false` fields of noise).
    fn base(
        kind: RowKind,
        name: String,
        state: String,
        age_ms: Option<i64>,
        detail: String,
    ) -> Row {
        Row {
            kind,
            name,
            state,
            age_ms,
            detail,
            cpu: None,
            tid: None,
            daemon: None,
            deadlocked: false,
        }
    }

    /// Whether the row survives a lowercased substring filter — the
    /// haystack is every rendered column joined with spaces, so a
    /// filter like `persp active` matches only what the eye could
    /// have matched on screen.
    pub fn matches_filter(&self, needle: &str) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.kind.label(),
            self.name,
            self.state,
            self.detail
        )
        .to_lowercase();
        haystack.contains(needle)
    }
}

impl SortKey {
    /// The comparison for one key. Age's rule: `None` sorts LAST in
    /// both directions (no data is never "oldest" nor "newest" — the
    /// honest placement).
    pub fn compare(self, a: &Row, b: &Row) -> std::cmp::Ordering {
        match self {
            SortKey::Kind => a
                .kind
                .rank()
                .cmp(&b.kind.rank())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::State => a
                .state
                .to_lowercase()
                .cmp(&b.state.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortKey::Age => match (a.age_ms, b.age_ms) {
                (Some(x), Some(y)) => y.cmp(&x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
            // CPU DESC (hot first). The f64 domain has no Eq — NaN
            // (never reported by the gateway) falls back to Equal and
            // the name tiebreak; `None` sorts last, the Age rule.
            SortKey::Cpu => match (a.cpu, b.cpu) {
                (Some(x), Some(y)) => y
                    .partial_cmp(&x)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            },
            SortKey::Detail => a
                .detail
                .to_lowercase()
                .cmp(&b.detail.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        }
    }
}

/// Push one value into a ring, dropping the oldest beyond
/// [`RING_CAPACITY`].
fn push_capped<T>(ring: &mut VecDeque<T>, value: T) {
    if ring.len() >= RING_CAPACITY {
        ring.pop_front();
    }
    ring.push_back(value);
}

/// The row's terminate id — the session id rides DETAIL for sessions
/// (`id` prefix) so it stays visible AND recoverable for the kill op
/// without a parallel row store. Module/connection/provider rows have
/// no terminate route.
fn row_id(row: &Row) -> &str {
    // detail carries "id:<id> …" for session rows (flattened in
    // flatten_sample); extract after the marker.
    let rest = row.detail.strip_prefix("id:").unwrap_or("");
    match rest.split_once(' ') {
        Some((id, _)) => id,
        None => rest,
    }
}

/// Flatten one composed sample into table rows — THE derivation
/// mapping each section's model onto the shared Row shape. Field
/// choices are honest: the gateway's own state-ish field verbatim
/// (never an invented verdict), fallbacks only for absent names.
pub fn flatten_sample(sample: &TopSample) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(modules) = &sample.modules {
        for module in &modules.items {
            let name = if module.name.is_empty() {
                module.id.clone()
            } else {
                module.name.clone()
            };
            let state = module.state.clone().unwrap_or_else(|| "unknown".into());
            let mut detail = module.version.clone();
            if detail.is_empty() {
                detail = module.id.clone();
            }
            if let Some(license) = &module.license_state
                && !license.is_empty()
            {
                detail.push_str(" · ");
                detail.push_str(license);
            }
            rows.push(Row::base(RowKind::Module, name, state, None, detail));
        }
    }
    if let Some(sessions) = &sample.sessions {
        for designer in &sessions.designers {
            let name = first_non_empty(&[&designer.user, &designer.address, &designer.id]);
            rows.push(Row::base(
                RowKind::Designer,
                name.to_string(),
                "connected".into(),
                Some(designer.uptime),
                session_detail(&designer.project, &designer.address, &designer.id, None),
            ));
        }
        for perspective in &sessions.perspective {
            let name = if perspective.username.is_empty() {
                "anonymous".into()
            } else {
                perspective.username.clone()
            };
            let state = if perspective.authorized {
                "authorized".into()
            } else {
                "unauthorized".into()
            };
            rows.push(Row::base(
                RowKind::Perspective,
                name,
                state,
                None,
                session_detail(
                    &perspective.project,
                    &perspective.client_address,
                    &perspective.id,
                    Some(perspective.active_pages),
                ),
            ));
        }
        for vision in &sessions.vision {
            let name = first_non_empty(&[&vision.user, &vision.address, &vision.id]);
            rows.push(Row::base(
                RowKind::Vision,
                name.to_string(),
                "connected".into(),
                Some(vision.uptime),
                session_detail(&vision.project, &vision.address, &vision.id, None),
            ));
        }
    }
    if let Some(connections) = &sample.connections {
        for connection in &connections.database {
            rows.push(Row::base(
                RowKind::Database,
                connection.name.clone(),
                if connection.enabled {
                    "enabled".into()
                } else {
                    "disabled".into()
                },
                None,
                healthcheck_summary(&connection.healthchecks),
            ));
        }
        for connection in &connections.opc {
            rows.push(Row::base(
                RowKind::Opc,
                connection.name.clone(),
                if connection.enabled {
                    "enabled".into()
                } else {
                    "disabled".into()
                },
                None,
                healthcheck_summary(&connection.healthchecks),
            ));
        }
    }
    if let Some(providers) = &sample.providers {
        for provider in &providers.providers {
            let state = provider.health.clone().unwrap_or_else(|| {
                if provider.enabled {
                    "enabled".into()
                } else {
                    "disabled".into()
                }
            });
            let detail = match provider.tag_count {
                Some(count) => format!("{count} tags"),
                None => String::new(),
            };
            rows.push(Row::base(
                RowKind::Provider,
                provider.name.clone(),
                state,
                None,
                detail,
            ));
        }
    }
    rows
}

/// First non-empty string of a candidate list — the session NAME
/// fallback chain (user → address → id).
fn first_non_empty<'a>(candidates: &[&'a str]) -> &'a str {
    candidates
        .iter()
        .copied()
        .find(|candidate| !candidate.is_empty())
        .unwrap_or("")
}

/// Flatten a formatted thread dump into rows — the Gateway web UI's
/// Diagnostics→Threads table. CPU rides the dump's `cpuUsage` (the
/// web column's percent scale); the JVM's own deadlock detection marks
/// its threads. Fields the gateway omits render as their honest
/// defaults (`—`, empty detail) — never invented verdicts.
pub fn flatten_thread_dump(dump: &FormattedThreadDump, deadlocked: &[i64]) -> Vec<Row> {
    dump.threads
        .iter()
        .map(|thread| {
            let name = if thread.name.is_empty() {
                format!(
                    "tid-{}",
                    thread.id.map(|id| id.to_string()).unwrap_or("?".into())
                )
            } else {
                thread.name.clone()
            };
            let state = if thread.state.is_empty() {
                "unknown".into()
            } else {
                thread.state.clone()
            };
            Row {
                kind: RowKind::Thread,
                name,
                state,
                age_ms: None,
                detail: thread_detail(thread),
                cpu: thread.cpu_usage,
                tid: thread.id,
                daemon: Some(thread.daemon),
                deadlocked: thread.id.is_some_and(|id| deadlocked.contains(&id)),
            }
        })
        .collect()
}

/// The thread row's DETAIL text — the supporting facts the web UI's
/// thread table shows: daemon, system/scope markers, and the monitor
/// evidence (waiting-for + held locks). Lock strings pass through
/// verbatim (the table clips; a mangled lock id helps nobody).
fn thread_detail(thread: &ThreadInfo) -> String {
    let mut parts = Vec::new();
    if thread.daemon {
        parts.push("daemon".to_string());
    }
    if !thread.system.is_empty() {
        parts.push(format!("system:{}", thread.system));
    }
    if !thread.scope.is_empty() {
        parts.push(format!("scope:{}", thread.scope));
    }
    if let Some(lock) = thread.waiting_for.as_ref().and_then(|w| w.lock.clone()) {
        parts.push(format!("waiting {lock}"));
    }
    for monitor in &thread.locked_monitors {
        if let Some(lock) = &monitor.lock {
            parts.push(format!("holds {lock}"));
        }
    }
    parts.join(" · ")
}

/// The stack-viewer modal's lines for one thread — header facts, the
/// hold/wait annotations, then the stack frames VERBATIM (the web
/// UI's stack pane content; a mangled frame is a bug, not a summary).
pub(crate) fn thread_stack_lines(thread: &ThreadInfo, deadlocked: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut header = vec![format!("state {}", thread.state)];
    if thread.daemon {
        header.push("daemon".to_string());
    }
    if let Some(cpu) = thread.cpu_usage {
        header.push(format!("cpu {cpu:.1}%"));
    }
    if let Some(id) = thread.id {
        header.push(format!("tid {id}"));
    }
    if !thread.scope.is_empty() {
        header.push(format!("scope {}", thread.scope));
    }
    lines.push(header.join(" · "));
    if deadlocked {
        lines.push("⚠ DEADLOCKED — the JVM's deadlock detection owns this thread".to_string());
    }
    for monitor in &thread.locked_monitors {
        match (&monitor.lock, &monitor.frame) {
            (Some(lock), Some(frame)) => lines.push(format!("holds {lock} — {frame}")),
            (Some(lock), None) => lines.push(format!("holds {lock}")),
            (None, Some(frame)) => lines.push(format!("holds a monitor — {frame}")),
            (None, None) => {}
        }
    }
    if let Some(lock) = thread.waiting_for.as_ref().and_then(|w| w.lock.clone()) {
        lines.push(format!("waiting for {lock}"));
    }
    if !thread.stacktrace.is_empty() {
        lines.push(String::new());
        for frame in &thread.stacktrace {
            lines.push(frame.clone());
        }
    } else {
        lines.push(String::new());
        lines.push("(the dump carries no stack for this thread)".to_string());
    }
    lines
}

/// The session DETAIL text: project, address, active pages (when the
/// family reports them), and the `id:` prefix the kill op recovers
/// the terminate id from (visible AND machine-readable — no parallel
/// row store to keep in sync).
fn session_detail(project: &str, address: &str, id: &str, pages: Option<i64>) -> String {
    let mut parts = vec![format!("id:{id}")];
    if !project.is_empty() {
        parts.push(project.to_string());
    }
    if !address.is_empty() {
        parts.push(address.to_string());
    }
    if let Some(pages) = pages {
        parts.push(format!("{pages}p"));
    }
    parts.join(" · ")
}

/// Compact a raw healthchecks Value into the DETAIL column — the
/// gateway's own map, verbatim-shaped, abbreviated only in length
/// (first pairs, joined; `no healthcheck` when absent/empty). NEVER
/// interpreted: a `{status: "UNHEALTHY", …}` renders as
/// `status=UNHEALTHY`, the gateway's own words. Length abbreviation
/// covers VALUES too (48 chars, `…`-marked) — a live gateway's
/// healthcheck result is a nested object with stacktraces, and the
/// verbatim dump floods the row (found in the first live run).
fn healthcheck_summary(healthchecks: &serde_json::Value) -> String {
    /// The value cap per rendered pair (chars, `…`-marked).
    const VALUE_CAP: usize = 48;
    let clip = |text: String| {
        if text.chars().count() > VALUE_CAP {
            let head: String = text.chars().take(VALUE_CAP).collect();
            format!("{head}…")
        } else {
            text
        }
    };
    match healthchecks.as_object() {
        Some(map) if !map.is_empty() => {
            let pairs: Vec<String> = map
                .iter()
                .take(3)
                .map(|(key, value)| match value.as_str() {
                    Some(text) => format!("{key}={}", clip(text.to_string())),
                    None => format!("{key}={}", clip(value.to_string())),
                })
                .collect();
            let mut summary = pairs.join(" · ");
            if map.len() > 3 {
                summary.push_str(&format!(" · +{}", map.len() - 3));
            }
            summary
        }
        _ => "no healthcheck".into(),
    }
}

/// Format bytes as a human size (B / KiB / MiB / GiB / TiB) — the
/// header bars' and heap sparkline's scale label. 1024-based (the JVM
/// heap convention), one decimal below 10 units, none above.
pub fn human_bytes(bytes: f64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 0.0 || !bytes.is_finite() {
        return "? B".into();
    }
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes:.0} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// Format epoch milliseconds as a human age — the AGE column and the
/// uptime readout. `d h m s` condensed htop-style: the two most
/// significant units only ("2d 03h", "5m 12s", "42s").
pub fn human_ms(ms: i64) -> String {
    if ms < 0 {
        return "?".into();
    }
    let total_secs = ms / 1000;
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    if days > 0 {
        format!("{days}d {hours:02}h")
    } else if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itop::worker::TopSample;
    use ignition_core::actions::{inspect, sessions, tags};

    fn point(value: f64) -> ignition_core::client::metrics::Datapoint {
        ignition_core::client::metrics::Datapoint {
            hist_id: 0,
            timestamp: 0,
            value,
        }
    }

    fn point_bytes(value: f64) -> ignition_core::client::metrics::Datapoint {
        point(value)
    }

    /// A minimal sample builder — the sections tests populate, the
    /// rest None (per-section degradation is the compose contract, so
    /// a row-level fixture exercises exactly one section).
    fn sample_with(rows: impl FnOnce(&mut TopSample)) -> TopSample {
        let mut sample = TopSample::default();
        rows(&mut sample);
        sample
    }

    fn module_row(name: &str, state: Option<&str>, version: &str) -> Row {
        Row::base(
            RowKind::Module,
            name.to_string(),
            state.unwrap_or("unknown").to_string(),
            None,
            version.to_string(),
        )
    }

    /// human_bytes' boundaries: sub-KiB exact, one decimal below 10
    /// units, none above, negatives refused honestly.
    #[test]
    fn human_bytes_formats_the_boundaries() {
        assert_eq!(human_bytes(0.0), "0 B");
        assert_eq!(human_bytes(512.0), "512 B");
        assert_eq!(human_bytes(1024.0), "1.0 KiB");
        assert_eq!(human_bytes(240_000_000.0), "229 MiB");
        assert_eq!(human_bytes(1_073_741_824.0), "1.0 GiB");
        assert_eq!(human_bytes(10.0 * 1024.0 * 1024.0), "10 MiB");
        assert_eq!(human_bytes(-5.0), "? B");
        assert_eq!(human_bytes(f64::NAN), "? B");
    }

    /// human_ms condenses to the two most significant units.
    #[test]
    fn human_ms_condenses_to_two_units() {
        assert_eq!(human_ms(42_000), "42s");
        assert_eq!(human_ms(5 * 60_000 + 12_000), "5m 12s");
        assert_eq!(human_ms(3 * 3_600_000 + 11 * 60_000), "3h 11m");
        assert_eq!(human_ms(2 * 86_400_000 + 3 * 3_600_000), "2d 03h");
        assert_eq!(human_ms(-1), "?");
    }

    /// Flatten maps every section onto rows with honest fallbacks:
    /// the module's absent name falls back to its id, the perspective
    /// anonymous user reads as "anonymous", the kill id rides the
    /// detail prefix.
    #[test]
    fn flatten_sample_maps_all_sections() {
        let sample = sample_with(|sample| {
            sample.modules = Some(inspect::ModulesResult {
                items: vec![ignition_core::client::status::ModuleInfo {
                    id: "com.inductiveautomation.perspective".into(),
                    name: String::new(),
                    version: "8.3.6".into(),
                    state: Some("ACTIVE".into()),
                    license_state: Some("ACTIVATED".into()),
                    vendor_name: None,
                    startup_time: None,
                    extra: Default::default(),
                }],
                quarantined: false,
            });
            sample.providers = Some(tags::TagProvidersResult {
                providers: vec![tags::TagProviderRow {
                    name: "default".into(),
                    enabled: true,
                    tag_count: Some(12_341),
                    health: None,
                    managed: false,
                }],
            });
        });
        let rows = flatten_sample(&sample);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].name, "com.inductiveautomation.perspective",
            "absent name → id"
        );
        assert_eq!(rows[0].state, "ACTIVE");
        assert_eq!(rows[0].detail, "8.3.6 · ACTIVATED");
        assert_eq!(rows[1].kind, RowKind::Provider);
        assert_eq!(rows[1].state, "enabled", "no health → enabled flag speaks");
        assert_eq!(rows[1].detail, "12341 tags");
    }

    /// The kill id round-trips through the detail prefix: arm →
    /// target carries the exact session id the terminate route takes.
    #[test]
    fn kill_target_recovers_the_session_id() {
        let sample = sample_with(|sample| {
            sample.sessions = Some(sessions::SessionsResult {
                designers: vec![],
                perspective: vec![ignition_core::client::sessions::PerspectiveSession {
                    id: "ps-1".into(),
                    username: "admin".into(),
                    authorized: true,
                    project: "whiskeyhouse".into(),
                    client_address: "10.0.0.9:55321".into(),
                    last_comm: 0,
                    active_pages: 2,
                    user_agent: String::new(),
                    extra: Default::default(),
                }],
                vision: vec![],
            });
        });
        let rows = flatten_sample(&sample);
        assert_eq!(
            rows[0].detail,
            "id:ps-1 · whiskeyhouse · 10.0.0.9:55321 · 2p"
        );
        assert_eq!(row_id(&rows[0]), "ps-1");
        assert_eq!(
            rows[0].kind.session_type(),
            Some(SessionType::Perspective),
            "session rows arm a terminate"
        );
        assert_eq!(
            module_row("M", Some("ACTIVE"), "1.0").kind.session_type(),
            None,
            "module rows never arm a kill"
        );
    }

    /// Sort: Kind groups by authored rank; Age puts longest first and
    /// never treats no-data as an extreme; reversal flips comparisons
    /// but keeps no-data last.
    #[test]
    fn sorting_rules_hold() {
        let mut a = module_row("Alpha", Some("ACTIVE"), "1");
        let mut b = module_row("beta", Some("DISABLED"), "1");
        a.kind = RowKind::Designer;
        b.kind = RowKind::Module;
        assert_eq!(SortKey::Kind.compare(&a, &b), std::cmp::Ordering::Greater);

        let mut aged = module_row("Gamma", None, "1");
        aged.age_ms = Some(5_000);
        let unaged = module_row("Delta", None, "1");
        assert_eq!(
            SortKey::Age.compare(&aged, &unaged),
            std::cmp::Ordering::Less,
            "aged sorts before unaged"
        );
        assert_eq!(
            SortKey::Age.compare(&unaged, &aged),
            std::cmp::Ordering::Greater,
            "unaged sorts last in BOTH directions"
        );
    }

    /// The healthcheck summary clips long VALUES (live discovery: a
    /// real gateway's result is a nested object with stacktraces — the
    /// verbatim dump flooded the row) but never interprets the shape.
    #[test]
    fn healthcheck_summary_clips_long_values() {
        let nested = serde_json::json!({
            "status": {"name": "db.status", "result": {"healthy": false, "message": "Faulted"}}
        });
        let summary = healthcheck_summary(&nested);
        assert!(summary.starts_with("status={"), "shape prefix verbatim");
        assert!(summary.ends_with('…'), "long value clipped with …");
        assert!(summary.chars().count() < 120, "row stays readable");
        let flat = serde_json::json!({"status": "HEALTHY"});
        assert_eq!(healthcheck_summary(&flat), "status=HEALTHY");
        let empty = serde_json::json!({});
        assert_eq!(healthcheck_summary(&empty), "no healthcheck");
    }

    /// The filter matches across every rendered column, lowercase.
    #[test]
    fn filter_spans_all_columns() {
        let row = module_row("Perspective", Some("ACTIVE"), "8.3.6");
        assert!(row.matches_filter("persp"));
        assert!(row.matches_filter("active"));
        assert!(row.matches_filter("8.3"));
        assert!(!row.matches_filter("vision"));
    }

    /// Interval clamps 1..=60 and reports whether anything changed —
    /// a clamp hit at the boundary must not churn the worker.
    #[test]
    fn interval_clamps_and_reports_change() {
        let mut state = TopState::default();
        assert!(state.adjust_interval(-10), "2 → 1 changed");
        assert_eq!(state.interval_secs, 1);
        assert!(!state.adjust_interval(-10), "1 is the floor — no change");
        assert!(state.adjust_interval(100), "1 → 60 changed");
        assert_eq!(state.interval_secs, 60);
        assert!(!state.adjust_interval(10), "60 is the ceiling — no change");
    }

    /// Rings cap at RING_CAPACITY, dropping the oldest.
    #[test]
    fn rings_cap_at_capacity() {
        let mut state = TopState::default();
        for i in 0..(RING_CAPACITY + 10) {
            push_capped(&mut state.cpu_ring, i as f64);
        }
        assert_eq!(state.cpu_ring.len(), RING_CAPACITY);
        assert_eq!(state.cpu_ring.front(), Some(&10.0), "oldest dropped");
        assert_eq!(
            state.cpu_ring.back(),
            Some(&((RING_CAPACITY + 9) as f64)),
            "newest kept"
        );
    }

    /// The charts bootstrap seeds the rings ONCE per world: an
    /// un-seeded state fills cpu/heap/non-heap from the gateway's own
    /// history; later history-carrying samples are ignored (local
    /// samples own the rings from there).
    #[test]
    fn charts_bootstrap_seeds_rings_once() {
        let mut state = TopState::default();
        let seeded = TopSample {
            history: Some(ignition_core::client::metrics::PerformanceCharts {
                cpu_datapoints: vec![point(3.5), point(4.5)],
                heap_memory_datapoints: vec![point_bytes(230_000_000.0)],
                non_heap_memory_datapoints: vec![point_bytes(52_000_000.0)],
            }),
            ..TopSample::default()
        };
        state.apply_sample(Box::new(seeded), std::time::Instant::now());
        assert!(state.history_seeded);
        assert_eq!(state.cpu_ring.len(), 2);
        assert_eq!(state.heap_ring.len(), 1);
        assert_eq!(state.nonheap_ring.len(), 1);
        assert_eq!(state.cpu_ring.front(), Some(&3.5));

        // A later history-carrying sample does NOT reseed.
        let again = TopSample {
            history: Some(ignition_core::client::metrics::PerformanceCharts {
                cpu_datapoints: vec![point(99.0)],
                heap_memory_datapoints: vec![],
                non_heap_memory_datapoints: vec![],
            }),
            ..TopSample::default()
        };
        state.apply_sample(Box::new(again), std::time::Instant::now());
        assert_eq!(state.cpu_ring.front(), Some(&3.5), "no reseed");
        assert_eq!(state.cpu_ring.len(), 2);
    }

    /// Selection clamps into the visible list on every change.
    #[test]
    fn selection_clamps_into_bounds() {
        let mut state = TopState {
            last: Some(Box::new(sample_with(|sample| {
                sample.providers = Some(tags::TagProvidersResult {
                    providers: (0..3)
                        .map(|i| tags::TagProviderRow {
                            name: format!("p{i}"),
                            enabled: true,
                            tag_count: None,
                            health: None,
                            managed: false,
                        })
                        .collect(),
                });
            }))),
            ..TopState::default()
        };
        state.move_selection(10);
        assert_eq!(state.selected, 2, "clamped to the last row");
        state.move_selection(-1);
        assert_eq!(state.selected, 1);
        state.filter = Some("zzz".into());
        state.clamp_selection();
        assert_eq!(state.selected, 0, "an empty visible list collapses to 0");
    }
}
