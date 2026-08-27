//! The cockpit's Elm-style model (Phase 6 research, Pattern 4).
//!
//! PURE DATA — no I/O, no async, no terminals. Every mutation flows
//! through [`crate::update`]; workers never touch this struct directly.
//! Per-screen data structs are added by their plans (06-02..06-06); the
//! shell owns the navigation chrome only.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use ignition_core::actions::sessions::{SessionType, SessionsResult};
use ignition_core::actions::tags::AlarmRow;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::logs::LogEntry;
use ratatui::widgets::TableState;
use tokio::sync::{mpsc, watch};

use crate::event::AppEvent;
use crate::workers::refresh::Snapshot;

/// Every screen the phase ships. ALL variants exist from day one so
/// later plans never edit the enum — they only grow what each variant
/// knows how to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Health at a glance: status/modules/metrics/sessions panels.
    #[default]
    Dashboard,
    /// Live tail + level filter + scrollback.
    Logs,
    /// Tag browser + live watch.
    Tags,
    /// Active alarms + ack flow.
    Alarms,
    /// Project/resource browser + actions.
    Projects,
    /// The local Docker compose rig.
    Rig,
}

impl Screen {
    /// Tab-bar order (also the Tab/Shift+Tab cycle order).
    pub const ALL: [Screen; 6] = [
        Screen::Dashboard,
        Screen::Logs,
        Screen::Tags,
        Screen::Alarms,
        Screen::Projects,
        Screen::Rig,
    ];

    /// The screen Tab lands on (wraps around).
    pub fn next(self) -> Screen {
        let all = &Self::ALL;
        let idx = all
            .iter()
            .position(|screen| *screen == self)
            .expect("every Screen variant is in ALL");
        all[(idx + 1) % all.len()]
    }

    /// The screen Shift+Tab (Backtab) lands on (wraps around).
    pub fn prev(self) -> Screen {
        let all = &Self::ALL;
        let idx = all
            .iter()
            .position(|screen| *screen == self)
            .expect("every Screen variant is in ALL");
        all[(idx + all.len() - 1) % all.len()]
    }

    /// The tab-bar label.
    pub fn title(self) -> &'static str {
        match self {
            Screen::Dashboard => "Dashboard",
            Screen::Logs => "Logs",
            Screen::Tags => "Tags",
            Screen::Alarms => "Alarms",
            Screen::Projects => "Projects",
            Screen::Rig => "Rig",
        }
    }
}

/// Which surface owns keystrokes right now (research Pattern 4: the
/// k9s/lazygit arbitration enum). Later screen plans grow their use of
/// `Table`/`Detail`; the shell owns `Normal` and `Modal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// No list/detail has the cursor — screen-global keys apply.
    #[default]
    Normal,
    /// A table/list widget has selection.
    Table,
    /// A detail pane is open.
    Detail,
    /// A modal owns every keystroke.
    Modal,
}

/// The three modal infrastructure shapes. Later plans EXTEND these with
/// acceptance payloads (wired through update, never stored futures) —
/// 06-02 adds the dashboard Actions menu and makes Result_ scrollable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// Yes/no confirmation — the TUI-side answer to the CLI's `--yes`
    /// guards (destructive verbs render this before their action). `y`
    /// accepts (executes [`AppState::dashboard`]'s `pending` action),
    /// Esc cancels.
    Confirm { title: String, body: String },
    /// A single-line text input, hand-rolled (no tui-input dep):
    /// char-append/backspace editing, Esc cancels, Enter accepts (the
    /// accepted buffer routes by `dashboard.pending_input`).
    Input { title: String, buffer: String },
    /// Read-only result/report lines (errors land here too). PgUp/PgDn
    /// scroll — the LOCKED one-mechanism result display every action
    /// verb shares (serde_json::to_string_pretty of the typed result).
    Result_ {
        title: String,
        lines: Vec<String>,
        scroll: u16,
    },
    /// The dashboard actions menu (06-02): the global verbs with a
    /// moving selection (Up/Down + Enter).
    Actions { selected: usize },
    /// The Logs screen's actions menu (06-03): the loggers family with
    /// a moving selection — the same shape as [`Modal::Actions`], its
    /// own list.
    LogsActions { selected: usize },
    /// The profile switcher (06-02): every configured profile name
    /// (BTreeMap order), the currently-active one marked, a moving
    /// selection. Enter switches, `a` opens the add form.
    Profiles {
        names: Vec<String>,
        active: Option<String>,
        selected: usize,
    },
    /// The profile add form (06-02): name + url fields, `field` is the
    /// one being edited (Tab toggles). Auth refs stay on the CLI form
    /// (the LOCKED modal-depth decision) — a hint line points there.
    ProfileAdd {
        name: String,
        url: String,
        field: usize,
    },
    /// The alarm-acknowledge form (06-03): `username` (REQUIRED — the
    /// 3-arg wire form needs it; Enter is a no-op until non-empty) and
    /// an optional `note`, Tab toggling between them. `event_id` is the
    /// ack TARGET — the selected row's id AS SHOWN (the action expands
    /// short prefixes itself, 05-08; the table shows full UUIDs).
    Ack {
        event_id: String,
        username: String,
        note: String,
        field: usize,
    },
}

/// What a confirmed modal executes — the TUI-side `--yes`. Modal accept
/// (`y`) looks here; cancel (Esc) clears it. The LOGGED verbs join the
/// set (the loggers family's two `--yes`-guarded mutations, 06-03) —
/// the TUI owns their confirmation, the action fns stay unguarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    /// `ign restart` — the guarded global verb.
    Restart,
    /// `ign sessions terminate` on the selected row.
    TerminateSession { kind: SessionType, id: String },
    /// `ign logs loggers set <logger> <LEVEL>` (Confirm ≡ `--yes`).
    LoggersSet { logger: String, level: String },
    /// `ign logs loggers reset` (Confirm ≡ `--yes`).
    LoggersReset,
}

/// What an accepted Input modal's buffer is for — the small-form router
/// (06-02 ships `wait module`'s id prompt; 06-03 adds the loggers
/// forms). The pending slots live on the dashboard's data but are
/// COCKPIT-global in practice: exactly one modal is open at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingInput {
    /// The module id for `wait module`.
    WaitModule,
    /// The optional substring search for `loggers list`.
    LoggersSearch,
    /// The `LOGGER LEVEL` line for `loggers set` (parsed before the
    /// Confirm gate arms).
    LoggersSetLine,
}

impl Modal {
    /// The modal's title line.
    pub fn title(&self) -> &str {
        match self {
            Modal::Confirm { title, .. }
            | Modal::Input { title, .. }
            | Modal::Result_ { title, .. } => title,
            Modal::Actions { .. } => "actions",
            Modal::LogsActions { .. } => "actions",
            Modal::Profiles { .. } => "profiles",
            Modal::ProfileAdd { .. } => "profile add",
            Modal::Ack { .. } => "ack alarm",
        }
    }
}

/// The client handle the workers target — a newtype ONLY because
/// `ReqwestGatewayApi` does not implement `Debug` and `AppState`
/// derives it. Storing a handle is data, not I/O; the actions still
/// run exclusively inside spawned workers.
pub struct ClientHandle(pub Arc<ReqwestGatewayApi>);

impl std::fmt::Debug for ClientHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientHandle(..)")
    }
}

/// One flattened, selectable row of the sessions panel (the terminate
/// target). The merged designers → perspective → vision order is THE
/// contract between render (table rows) and update (selection index →
/// terminate call) — both sides go through [`session_rows`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// The family (selects the terminate route).
    pub kind: SessionType,
    /// Session/client id.
    pub id: String,
    /// Authenticated user.
    pub user: String,
    /// Open project.
    pub project: String,
}

/// Flatten a sessions result into selectable rows, in the LOCKED
/// order (designers, then perspective, then vision).
pub fn session_rows(result: &SessionsResult) -> Vec<SessionRow> {
    let mut rows: Vec<SessionRow> = result
        .designers
        .iter()
        .map(|d| SessionRow {
            kind: SessionType::Designer,
            id: d.id.clone(),
            user: d.user.clone(),
            project: d.project.clone(),
        })
        .chain(result.perspective.iter().map(|p| SessionRow {
            kind: SessionType::Perspective,
            id: p.id.clone(),
            user: p.username.clone(),
            project: p.project.clone(),
        }))
        .chain(result.vision.iter().map(|v| SessionRow {
            kind: SessionType::Vision,
            id: v.id.clone(),
            user: v.user.clone(),
            project: v.project.clone(),
        }))
        .collect();
    rows.shrink_to_fit();
    rows
}

/// The dashboard actions menu entries, in menu order — the LOCKED list
/// of global verbs the dashboard hosts (update's executor and the modal
/// renderer both key off this order).
pub const ACTIONS: [&str; 7] = [
    "version",
    "connections",
    "wait gateway",
    "wait restart",
    "wait module",
    "doctor",
    "restart",
];

/// The Logs screen's actions menu entries (06-03) — the loggers
/// family. Labels are display-side; the route rows in
/// [`crate::routes`] carry the clap-exact spellings.
pub const LOG_ACTIONS: [&str; 3] = ["loggers list", "loggers set", "loggers reset"];

/// The dashboard screen's data (06-02).
#[derive(Debug, Default)]
pub struct DashboardData {
    /// Latest accepted snapshot (None until the first worker report —
    /// every panel then renders its Loading state).
    pub snapshot: Option<Snapshot>,
    /// When the last accepted snapshot landed — drives the "N s ago"
    /// status line; the 250 ms tick keeps it live without extra state.
    pub last_refresh: Option<Instant>,
    /// A refresh is in flight — the `r` keystroke's busy guard
    /// (keystrokes cannot stack duplicate refreshes).
    pub busy: bool,
    /// The sessions table cursor (selectable rows — the terminate
    /// target). Copied for `render_stateful_widget`; update owns the
    /// mutations.
    pub sessions_table: TableState,
    /// What a Confirm-modal accept executes (cleared on cancel).
    pub pending: Option<PendingAction>,
    /// What an Input-modal accept feeds (cleared on cancel).
    pub pending_input: Option<PendingInput>,
    /// The running one-shot action's label ("wait gateway") — the busy
    /// guard per action; the status line renders it while in flight.
    pub in_flight: Option<&'static str>,
}

/// The display-level filter over the gateway's log levels. `All`
/// disables filtering; the rest act as a MINIMUM threshold (filter =
/// Warn renders Warn and above) applied AT RENDER over the retained
/// ring — filtering the query alone would hide already-received
/// entries (research anti-pattern). The same threshold rides the tail
/// worker's restart as `min_level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevelFilter {
    /// No filtering — every retained entry renders.
    #[default]
    All,
    /// TRACE and above.
    Trace,
    /// DEBUG and above.
    Debug,
    /// INFO and above.
    Info,
    /// WARN and above.
    Warn,
    /// ERROR and above.
    Error,
}

/// The LOCKED ring capacity — a weekend-long tail cannot OOM the
/// process (must-have truth #2): the display buffer is a ring capped
/// at 10,000 entries, evicting from the front.
pub const LOG_RING_CAP: usize = 10_000;

impl LogLevelFilter {
    /// The `l`-key cycle: All → Trace → … → Error → All.
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Trace,
            Self::Trace => Self::Debug,
            Self::Debug => Self::Info,
            Self::Info => Self::Warn,
            Self::Warn => Self::Error,
            Self::Error => Self::All,
        }
    }

    /// The uppercase wire token for the tail's `min_level` (None when
    /// unfiltered — the param stays absent, exactly like the CLI).
    pub fn wire(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Trace => Some("TRACE"),
            Self::Debug => Some("DEBUG"),
            Self::Info => Some("INFO"),
            Self::Warn => Some("WARN"),
            Self::Error => Some("ERROR"),
        }
    }

    /// The status-row label.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Whether an entry's wire level passes this threshold (render-side
    /// filter). Unknown level strings rank with INFO — never hidden by
    /// Trace..Info filters, hidden by Warn/Error (quality-is-data: the
    /// level string is never parsed beyond ranking).
    pub fn matches(self, level: &str) -> bool {
        self.wire()
            .is_none_or(|min| level_rank(level) >= level_rank(min))
    }
}

/// Rank a wire level string for threshold comparisons (TRACE lowest).
fn level_rank(level: &str) -> u8 {
    match level {
        "TRACE" => 1,
        "DEBUG" => 2,
        "INFO" => 3,
        "WARN" => 4,
        "ERROR" => 5,
        "FATAL" => 6,
        _ => 3, // unknown levels rank with INFO
    }
}

/// The Logs screen's data (06-03): the ring-backed stream, the
/// render-side level filter, and the follow/scroll state machine.
#[derive(Debug)]
pub struct LogsData {
    /// The retained stream — a ring capped at [`LOG_RING_CAP`] entries
    /// (evict front). Memory-bounded by construction.
    pub ring: VecDeque<LogEntry>,
    /// The render-side level filter (also the tail's `min_level`).
    pub filter: LogLevelFilter,
    /// How many FILTERED lines the view sits above the bottom (0 = at
    /// the newest line). Scrolling up disables follow; `G`/End (or
    /// scrolling back down to the bottom) re-enables it.
    pub scroll_offset: usize,
    /// Auto-scroll to bottom on new entries, unless the user scrolled
    /// up (`f` toggles).
    pub follow: bool,
    /// Total entries evicted from the ring's front (ring turnover
    /// accounting — the status row's honesty counter).
    pub dropped: usize,
    /// The tail worker's shutdown switch — `send(true)` on leaving the
    /// screen, filter restart, and profile switch (a dropped sender
    /// also stops the worker).
    pub tail_shutdown: Option<watch::Sender<bool>>,
}

impl Default for LogsData {
    fn default() -> Self {
        Self {
            ring: VecDeque::new(),
            filter: LogLevelFilter::default(),
            scroll_offset: 0,
            follow: true,
            dropped: 0,
            tail_shutdown: None,
        }
    }
}

impl LogsData {
    /// Append one streamed entry, evicting from the front at the cap —
    /// THE ring discipline (a weekend-long tail stays memory-bounded).
    pub fn push_line(&mut self, entry: LogEntry) {
        while self.ring.len() >= LOG_RING_CAP {
            self.ring.pop_front();
            self.dropped += 1;
        }
        self.ring.push_back(entry);
    }

    /// How many retained entries pass the current filter — the scroll
    /// bounds and the render window both key off this length.
    pub fn filtered_len(&self) -> usize {
        self.ring
            .iter()
            .filter(|e| self.filter.matches(&e.level))
            .count()
    }

    /// Scroll `step` filtered lines towards the top (disables follow).
    /// Clamped so the FIRST filtered line can reach the top of the
    /// view, never further.
    pub fn scroll_up(&mut self, step: usize) {
        let len = self.filtered_len();
        if len <= 1 {
            return;
        }
        self.scroll_offset = (self.scroll_offset + step).min(len - 1);
        self.follow = false;
    }

    /// Scroll `step` filtered lines back towards the bottom; reaching
    /// the bottom re-enables follow.
    pub fn scroll_down(&mut self, step: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(step);
        if self.scroll_offset == 0 {
            self.follow = true;
        }
    }

    /// `G`/End: jump to the newest line and re-enable follow.
    pub fn jump_to_end(&mut self) {
        self.scroll_offset = 0;
        self.follow = true;
    }

    /// `f`: toggle follow. Re-enabling snaps to the bottom (following
    /// from mid-history is a lie about the newest line).
    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow;
        if self.follow {
            self.scroll_offset = 0;
        }
    }
}

/// The Alarms screen's data (06-03): the polled active-alarm table
/// (the action's own row type — re-used, never re-mapped) with the
/// per-poll degrade convention, and the poll worker's rail.
#[derive(Debug, Default)]
pub struct AlarmsData {
    /// Latest accepted rows (None until the first poll — or after an
    /// errored poll: the honest error state replaces stale rows).
    pub active: Option<Vec<AlarmRow>>,
    /// Why the last poll errored, when it did.
    pub error: Option<String>,
    /// When the last accepted poll landed — the header's poll age.
    pub last_poll: Option<Instant>,
    /// A one-shot poll is in flight — the ack-refresh trigger's busy
    /// guard.
    pub busy: bool,
    /// The table cursor (the ack target's selector).
    pub table: TableState,
    /// The poll worker's shutdown switch — screen-scoped like the
    /// tail's.
    pub shutdown: Option<watch::Sender<bool>>,
}

/// The whole cockpit, in plain data. The era counter is the stale-worker
/// guard (research Pitfall 9): workers stamp their spawn-era onto
/// results; update drops events whose era no longer matches.
#[derive(Debug, Default)]
pub struct AppState {
    /// Set by update; the select loop breaks out when it flips.
    pub should_quit: bool,
    /// The active screen.
    pub screen: Screen,
    /// Keystroke arbitration.
    pub focus: Focus,
    /// The open modal, if any (modal keys route here first).
    pub modal: Option<Modal>,
    /// Worker generation counter — bumped when the world under the
    /// workers changes (screen exit, profile switch).
    pub era: u64,
    /// The client workers target (None before resolve / in unit tests).
    pub client: Option<ClientHandle>,
    /// The active profile's URL string (doctor's `profile_url`).
    pub profile_url: Option<String>,
    /// The AppEvent rail — a clone of the loop's sender, so update can
    /// arm workers (spawn helpers take their copy from here).
    pub events_tx: Option<mpsc::UnboundedSender<AppEvent>>,
    /// The dashboard's refresh worker shutdown switch — `send(true)` on
    /// profile switch (re-target) and on quit.
    pub refresh_shutdown: Option<watch::Sender<bool>>,
    /// Dashboard screen data (06-02).
    pub dashboard: DashboardData,
    /// Logs screen data (06-03) — the ring, filter, follow/scroll.
    pub logs: LogsData,
    /// Alarms screen data (06-03) — the polled table + ack target.
    pub alarms: AlarmsData,
    /// The active profile's name (workers' target — what `p` switches).
    pub profile: Option<String>,
    /// The status-line banner — set by a landed profile switch
    /// (`profile: NAME`), cleared by the first refresh of the new
    /// world (the confirmation fulfilled its purpose).
    pub banner: Option<String>,
}

impl AppState {
    /// Fresh cockpit: Dashboard screen, normal focus, no modal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a modal and move focus to it.
    pub fn open_modal(&mut self, modal: Modal) {
        self.modal = Some(modal);
        self.focus = Focus::Modal;
    }

    /// Close the modal (if any) and return focus to normal.
    pub fn close_modal(&mut self) {
        self.modal = None;
        self.focus = Focus::Normal;
    }
}
