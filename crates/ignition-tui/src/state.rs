//! The cockpit's Elm-style model (Phase 6 research, Pattern 4).
//!
//! PURE DATA — no I/O, no async, no terminals. Every mutation flows
//! through [`crate::update`]; workers never touch this struct directly.
//! Per-screen data structs are added by their plans (06-02..06-06); the
//! shell owns the navigation chrome only.

use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use ignition_core::actions::projects::ProjectSummary;
use ignition_core::actions::resources::ResourceGetResult;
use ignition_core::actions::sessions::{SessionType, SessionsResult};
use ignition_core::actions::tags::{AlarmRow, BrowseRow, TagProviderRow, TagReadRow};
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::client::logs::LogEntry;
use ignition_core::client::projects::ProjectRecord;
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
    /// `hint` is the optional one-line rule reminder (06-04's write
    /// form states the JSON-scalar rule; the loggers forms carry
    /// none).
    Input {
        title: String,
        hint: Option<String>,
        buffer: String,
    },
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
    /// The Tags screen's actions menu (06-04): the remaining tags
    /// family verbs with a moving selection — the same shape as
    /// [`Modal::Actions`], its own list.
    TagsActions { selected: usize },
    /// The Projects screen's actions menu (06-05): the project,
    /// resource, and webdev family verbs with a moving selection —
    /// the same shape as [`Modal::Actions`], its own list.
    ProjectsActions { selected: usize },
    /// The Rig screen's actions menu (06-06): the rig family verbs
    /// (up/down/reset/status/logs/trial status/trial reset/snapshot/
    /// restore) with a moving selection — the same shape as
    /// [`Modal::Actions`], its own list.
    RigActions { selected: usize },
}

/// What a confirmed modal executes — the TUI-side `--yes`. Modal accept
/// (`y`) looks here; cancel (Esc) clears it. The LOGGED verbs join the
/// set (the loggers family's two `--yes`-guarded mutations, 06-03) and
/// the tags family's guarded three (provider delete, config delete,
/// import overwrite — 06-04) — the TUI owns their confirmation, the
/// action fns stay unguarded.
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
    /// `ign tags provider delete NAME` (Confirm ≡ `--yes` — the 6th
    /// guarded verb in the CLI; the TUI mirrors the guard).
    TagsProviderDelete { name: String },
    /// `ign tags config delete PATH` (Confirm ≡ `--yes` — the 7th).
    TagsConfigDelete { path: String },
    /// `ign tags import --collision-policy overwrite` (Confirm ≡
    /// `--yes`; the ABORT policy is unguarded — its collisions refuse
    /// at the action's own zero-write pre-check).
    TagsImportOverwrite { file: String, provider: String },
    /// `ign project delete NAME` (Confirm ≡ `--yes` — the guarded
    /// project verb, main.rs's own set).
    ProjectDelete { name: String },
    /// `ign project import <NAME> --file <FILE>
    /// --collision-policy overwrite` (Confirm ≡ `--yes`; the ABORT
    /// policy needs no confirm — its collisions refuse at the
    /// action's own zero-write pre-check).
    ProjectImportOverwrite { name: String, file: String },
    /// `ign resource put <PROJECT> <PATH> --file <FILE>` (Confirm ≡
    /// `--yes` — guarded since 05-02: the member surgery implicitly
    /// overwrite-imports the whole project).
    ResourcePut {
        project: String,
        path: String,
        file: String,
    },
    /// `ign resource delete <PROJECT> <PATH>` (Confirm ≡ `--yes` —
    /// guarded since 05-02, the put twin).
    ResourceDelete { project: String, path: String },
    /// `ign rig reset` (Confirm ≡ `--yes` — the guarded teardown
    /// cycle; the TUI mirrors main.rs's `require_confirmation` set
    /// EXACTLY: reset, restore, and trial reset are the ONLY gated
    /// rig verbs).
    RigReset,
    /// `ign rig restore --file <FILE>` (Confirm ≡ `--yes`).
    RigRestore { file: String },
    /// `ign rig trial reset` (Confirm ≡ `--yes`; credentials ride
    /// the env ladder — IGNITION_TOKEN / IGNITION_USER +
    /// IGNITION_PASSWORD — the CLI's `--user` flag has no cockpit
    /// form, the `?` hatch names the env vars).
    RigTrialReset,
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
            Modal::TagsActions { .. } => "actions",
            Modal::ProjectsActions { .. } => "actions",
            Modal::RigActions { .. } => "actions",
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
/// renderer both key off this order). The wait labels are display
/// prose (06-10: "wait restart" scanned like a restart variant); the
/// route rows in [`crate::routes`] and the worker labels carry the
/// clap-exact spellings ("wait for gateway up" runs the `wait gateway`
/// worker).
pub const ACTIONS: [&str; 7] = [
    "version",
    "connections",
    "wait for gateway up",
    "wait for restart complete",
    "wait for module ready",
    "doctor",
    "restart",
];

/// The Logs screen's actions menu entries (06-03) — the loggers
/// family. Labels are display-side; the route rows in
/// [`crate::routes`] carry the clap-exact spellings.
pub const LOG_ACTIONS: [&str; 3] = ["loggers list", "loggers set", "loggers reset"];

/// The Tags screen's actions menu entries (06-04) — the remaining
/// tags family verbs (browse/read live on the navigation itself;
/// alarms lives on the Alarms screen, 06-03). Labels are display
/// side; the route rows in [`crate::routes`] carry the clap-exact
/// spellings.
pub const TAG_ACTIONS: [&str; 13] = [
    "write",
    "providers list",
    "providers create",
    "providers delete",
    "config get",
    "config create",
    "config edit",
    "config delete",
    "export",
    "import",
    "udt types",
    "udt def",
    "history query",
];

/// One Projects-screen actions-menu entry (06-05, regrouped 06-10):
/// the noun group it renders under (a section header), the executor
/// dispatch verb (the arm key — identical on both sides, the
/// clap-exact spelling), a human display label, and a one-line
/// consequence description (rendered dimmed after the label).
pub struct ProjectAction {
    /// Section header this entry renders under ("project" /
    /// "resource" / "webdev").
    pub group: &'static str,
    /// The dispatch key — must match update's executor arm exactly.
    pub verb: &'static str,
    /// Human display label.
    pub label: &'static str,
    /// One-line consequence description.
    pub description: &'static str,
}

/// The Projects screen's actions menu (06-05, regrouped 06-10) —
/// noun-grouped (project / resource / webdev) with human labels and
/// consequence descriptions, answering the UAT's "delete vs resource
/// delete" confusion: the section carries the scope, the description
/// carries the consequence. ONE flat index space in group order (the
/// `group` field drives the rendered section headers — contiguity is
/// test-pinned); the resource family's guarded verbs (put/delete —
/// list/get live on the navigation itself), webdev deploy deliberately
/// UNGUARDED (the ign-cli project is CLI-owned, the 05-03 decision).
/// The verb keys are the clap-exact spellings; the route rows in
/// [`crate::routes`] stay untouched.
pub const PROJECT_ACTIONS: [ProjectAction; 11] = [
    ProjectAction {
        group: "project",
        verb: "new",
        label: "new",
        description: "create an empty project",
    },
    ProjectAction {
        group: "project",
        verb: "copy",
        label: "copy",
        description: "duplicate under a new name",
    },
    ProjectAction {
        group: "project",
        verb: "rename",
        label: "rename",
        description: "change the project's name",
    },
    ProjectAction {
        group: "project",
        verb: "set",
        label: "set",
        description: "change one field (title, …)",
    },
    ProjectAction {
        group: "project",
        verb: "delete",
        label: "delete",
        description: "remove the whole project",
    },
    ProjectAction {
        group: "project",
        verb: "import",
        label: "import",
        description: "load a project from a zip",
    },
    ProjectAction {
        group: "project",
        verb: "export",
        label: "export",
        description: "stream the project to a zip",
    },
    ProjectAction {
        group: "resource",
        verb: "resource put",
        label: "put",
        description: "create or replace one file",
    },
    ProjectAction {
        group: "resource",
        verb: "resource delete",
        label: "delete",
        description: "remove one file from it",
    },
    ProjectAction {
        group: "webdev",
        verb: "webdev deploy",
        label: "deploy",
        description: "publish to the gateway",
    },
    ProjectAction {
        group: "webdev",
        verb: "webdev status",
        label: "status",
        description: "report the routes",
    },
];

/// The Rig screen's actions menu entries (06-06) — the FULL
/// RigCommand verb set (up/down/reset/status/logs + the trial pair +
/// snapshot/restore). Labels are display side; the route rows in
/// [`crate::routes`] carry the clap-exact spellings.
pub const RIG_ACTIONS: [&str; 9] = [
    "up",
    "down",
    "reset",
    "status",
    "logs",
    "trial status",
    "trial reset",
    "snapshot",
    "restore",
];

/// What an accepted Input modal's buffer is for on the Tags screen
/// (06-04) — the tags family's own small-form router, carrying its
/// own payloads (multi-step flows chain through these). Cleared by
/// the shared cancel path so a stale form can never arm a later
/// Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagsForm {
    /// `write` — the VALUE for the carried path (the JSON-scalar rule
    /// applies at accept; the modal's hint states it).
    WriteValue { path: String },
    /// `providers create` — the new provider's NAME.
    ProviderCreateName,
    /// `providers delete` — the target provider's NAME (a Confirm
    /// gate arms before the fire).
    ProviderDeleteName,
    /// `config get` — the tag PATH.
    ConfigGetPath,
    /// `config create` step 1 — the tag PATH (the definition-file
    /// prompt chains next).
    ConfigCreatePath,
    /// `config create` step 2 — the JSON definition FILE path.
    ConfigCreateFile { path: String },
    /// `config edit` step 1 — the tag PATH.
    ConfigEditPath,
    /// `config edit` step 2 — the JSON definition FILE path.
    ConfigEditFile { path: String },
    /// `config delete` — the tag PATH (a Confirm gate arms before the
    /// fire).
    ConfigDeletePath,
    /// `export` — the output FILE (prefilled with the 05-05 default
    /// naming for the carried path).
    ExportFile { path: String },
    /// `import` step 1 — the export FILE path.
    ImportFile,
    /// `import` step 2 — the target PROVIDER.
    ImportProvider { file: String },
    /// `import` step 3 — the collision POLICY (`abort`/`overwrite`;
    /// the overwrite arm is Confirm-gated, abort fires unguarded).
    ImportPolicy { file: String, provider: String },
    /// `udt types` — the PROVIDER whose `_types_` folder to list.
    UdtTypesProvider,
    /// `udt def` step 1 — the UDT type NAME.
    UdtDefName,
    /// `udt def` step 2 — the PROVIDER.
    UdtDefProvider { name: String },
    /// `history query` — the tag PATH (the trailing-24h window rides
    /// like the alarms history browse).
    HistoryQueryPath,
}

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

/// One level of the tag-tree browse stack (06-04): the browse path
/// the level lists, its loaded entries, and the cursor saved when
/// descending (restored on ascend — navigation honesty).
#[derive(Debug, Default)]
pub struct BrowseLevel {
    /// The browse path this level lists (`[default]P5` — a provider's
    /// root level is `[name]`).
    pub path: String,
    /// The loaded entries (None while loading; an error replaces them
    /// with the honest error state).
    pub entries: Option<Vec<BrowseRow>>,
    /// Why the load errored, when it did.
    pub error: Option<String>,
    /// The cursor position saved on descend (restored on ascend).
    pub selected: Option<usize>,
}

/// The detail pane's on-demand read state — 06-02's Loading/Error
/// pattern applied to a single row (quality strings are DATA, never
/// parsed into errors — the 05-04 convention).
#[derive(Debug)]
pub enum DetailRead {
    /// The one-shot read is in flight.
    Loading,
    /// The landed row (value raw JSON, quality/timestamp verbatim).
    Loaded(TagReadRow),
    /// The read's error — require_routes denials surface here with
    /// the action's own hint text (route preconditions live INSIDE
    /// the actions layer; the TUI inherits them for free).
    Error(String),
}

/// The open tag detail pane (06-04): node info from the browse row
/// plus the on-demand current value.
#[derive(Debug)]
pub struct TagsDetail {
    /// The tag's full bracket-qualified path.
    pub path: String,
    /// The leaf name.
    pub name: String,
    /// The wire tagType token verbatim.
    pub tag_type: String,
    /// The row's dataType, when it carried one.
    pub data_type: Option<String>,
    /// The on-demand read (fired when the pane opened; Enter refires).
    pub read: DetailRead,
}

/// The Tags screen's data (06-04): the k9s-style object browser —
/// the provider list, the descend/ascend browse stack, the open
/// detail pane with its on-demand read, and the live-watch set.
#[derive(Debug, Default)]
pub struct TagsData {
    /// The provider list level: rows (None until loaded / after an
    /// error), the honest error, the busy guard, the cursor.
    pub providers: Option<Vec<TagProviderRow>>,
    /// Why the provider load errored, when it did.
    pub providers_error: Option<String>,
    /// A provider load is in flight (the entry/reload busy guard).
    pub providers_busy: bool,
    /// The provider table cursor.
    pub providers_table: TableState,
    /// The browse stack (empty = the provider level is the surface;
    /// the TOP of the stack is the current level).
    pub stack: Vec<BrowseLevel>,
    /// The current (top) level's cursor.
    pub tree_table: TableState,
    /// The open detail pane, if any.
    pub detail: Option<TagsDetail>,
    /// Detail-open sequence: every open bumps it and stamps its read
    /// worker — a read for a left/replaced pane drops (the request-id
    /// stale gate; the global era stays world-scoped per 06-03's
    /// lock, so this counter is the detail pane's private era).
    pub detail_seq: u64,
    /// The live-watch set (tag paths), BTree order — the worker's
    /// whole request and the marker's source of truth.
    pub watched: BTreeSet<String>,
    /// The watch table's latest rows (request order — the set's).
    pub watch_rows: Vec<TagReadRow>,
    /// Paths whose value or quality CHANGED on the last poll (the
    /// table's updated-marker; timestamps excluded — a clock bump
    /// with an unchanged value is not a change).
    pub watch_changed: BTreeSet<String>,
    /// Why the last watch poll errored, when it did (rows degrade to
    /// the honest error state, the alarms convention).
    pub watch_error: Option<String>,
    /// Watch-worker generation: every (re)spawn bumps it — a
    /// set-change respawn's in-flight poll from the PRIOR worker
    /// drops (the local stale gate; the global era stays world-scoped
    /// per 06-03's lock).
    pub watch_gen: u64,
    /// The watch worker's shutdown switch — screen exit, empty set,
    /// set-change respawn, profile switch.
    pub watch_shutdown: Option<watch::Sender<bool>>,
    /// The armed tags form (the Input modal's routing slot for the
    /// tags family — [`TagsForm`]); cleared by the shared cancel path.
    pub pending_form: Option<TagsForm>,
    /// The path of the most recently FIRED `tags write` (06-09): the
    /// ActionDone refresh trigger's comparison target — ActionDone
    /// carries only the label, so the form's accept site arms this
    /// and the landing consumes it once (a FAILED write leaves the
    /// displayed value correct, so only success refires).
    pub last_write_path: Option<String>,
}

/// The open project detail's find tri-state (06-05) — 06-02's
/// Loading/Error pattern applied to the record pane.
#[derive(Debug)]
pub enum ProjectRecordState {
    /// The one-shot find is in flight.
    Loading,
    /// The read-back record (the six summary fields PLUS the
    /// defaultDb/tagProvider/userSource passthrough the detail pane
    /// shows).
    Loaded(ProjectRecord),
    /// The find's error.
    Error(String),
}

/// The open project detail pane (06-05): the record (one-shot find)
/// plus the project's resource paths (one-shot export-surgery list —
/// the surgery itself is invisible here, the actions layer owns it).
#[derive(Debug)]
pub struct ProjectDetail {
    /// The project's name (the fetch target AND the pane's identity
    /// for stale-result gating).
    pub name: String,
    /// The find's tri-state.
    pub record: ProjectRecordState,
    /// The project's resource paths (None while loading; an error
    /// replaces them with the honest error state).
    pub resources: Option<Vec<String>>,
    /// Why the resources load errored, when it did.
    pub resources_error: Option<String>,
    /// The resources table cursor (the drill-down selector).
    pub resources_table: TableState,
}

/// The open resource detail's get tri-state (06-05). Binary fencing
/// rides the action's own exit-6 `resource_binary` refusal — surfaced
/// as the Error state verbatim, never a blank pane.
#[derive(Debug)]
pub enum ResourceGetState {
    /// The one-shot get is in flight.
    Loading,
    /// The flat `{project, path, content_kind, content}` shape.
    Loaded(ResourceGetResult),
    /// The get's error.
    Error(String),
}

/// The open resource detail pane (06-05): the get result with a
/// SCROLLABLE content preview (text content renders; the action
/// fences binary out with its exit-6).
#[derive(Debug)]
pub struct ResourceDetail {
    /// The owning project.
    pub project: String,
    /// The resource path (slashes kept).
    pub path: String,
    /// The get's tri-state.
    pub state: ResourceGetState,
    /// The content preview's line offset from the top (Up/Down
    /// scroll at this depth — clamped at render).
    pub scroll: u16,
}

impl ResourceDetail {
    /// The content preview lines: JSON pretty-printed, text raw —
    /// derived at render (state stays small; the pure derivation is
    /// unit-pinned).
    pub fn content_lines(result: &ResourceGetResult) -> Vec<String> {
        if result.content_kind == "text" {
            result
                .content
                .as_str()
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        } else {
            serde_json::to_string_pretty(&result.content)
                .unwrap_or_else(|_| result.content.to_string())
                .lines()
                .map(str::to_string)
                .collect()
        }
    }
}

/// What an accepted Input modal's buffer is for on the Projects
/// screen (06-05) — the project/resource/webdev families' own
/// small-form router, carrying its own payloads (multi-step flows
/// chain through these). Cleared by the shared cancel path so a
/// stale form can never arm a later Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectsForm {
    /// `project new` step 1 — the new project's NAME.
    NewName,
    /// `project new` step 2 — the optional display TITLE (empty
    /// skips the field).
    NewTitle { name: String },
    /// `project copy` step 1 — the SOURCE project.
    CopySrc,
    /// `project copy` step 2 — the DESTINATION name.
    CopyDst { src: String },
    /// `project rename` step 1 — the CURRENT name (prefilled from
    /// the selection).
    RenameOld,
    /// `project rename` step 2 — the NEW name.
    RenameNew { old: String },
    /// `project set` step 1 — the target project NAME (prefilled
    /// from the selection).
    SetName,
    /// `project set` step 2 — the `FIELD=VALUE` line (one pair per
    /// prompt; the `?` CLI form shows every flag).
    SetLine { name: String },
    /// `project delete` — the target NAME (a Confirm gate arms
    /// next).
    DeleteName,
    /// `project import` step 1 — the export ZIP file path.
    ImportFile,
    /// `project import` step 2 — the name to import AS.
    ImportName { file: String },
    /// `project import` step 3 — the collision POLICY
    /// (`abort`/`overwrite`; the overwrite arm is Confirm-gated,
    /// abort fires unguarded).
    ImportPolicy { file: String, name: String },
    /// `project export` — the output FILE (prefilled with the
    /// `<name>.zip` default).
    ExportFile { name: String },
    /// `resource put` step 1 — the target PROJECT.
    ResourcePutProject,
    /// `resource put` step 2 — the resource PATH.
    ResourcePutPath { project: String },
    /// `resource put` step 3 — the content FILE path (a Confirm gate
    /// arms next).
    ResourcePutFile { project: String, path: String },
    /// `resource delete` step 1 — the target PROJECT.
    ResourceDeleteProject,
    /// `resource delete` step 2 — the resource PATH (a Confirm gate
    /// arms next).
    ResourceDeletePath { project: String },
}

/// The Projects screen's data (06-05): the object-list → detail
/// navigation stack — the project list, the open project detail with
/// its resources list, and the open resource detail (the get with
/// content preview).
#[derive(Debug, Default)]
pub struct ProjectsData {
    /// The project list: rows (None until loaded / after an error),
    /// the honest error, the busy guard, the cursor.
    pub list: Option<Vec<ProjectSummary>>,
    /// Why the list load errored, when it did.
    pub list_error: Option<String>,
    /// A list load is in flight (the entry/reload busy guard).
    pub list_busy: bool,
    /// The project table cursor.
    pub list_table: TableState,
    /// The open project detail pane, if any (navigation level 1).
    pub detail: Option<ProjectDetail>,
    /// The open resource detail pane, if any (navigation level 2).
    pub resource: Option<ResourceDetail>,
    /// Resource-open sequence: every open bumps it and stamps its get
    /// worker — a get for a left/replaced pane drops (the request-id
    /// stale gate, the tags detail's shape).
    pub resource_seq: u64,
    /// The armed projects form (the Input modal's routing slot for
    /// the project/resource/webdev families — [`ProjectsForm`]);
    /// cleared by the shared cancel path.
    pub pending_form: Option<ProjectsForm>,
}

/// What an accepted Input modal's buffer is for on the Rig screen
/// (06-06) — the rig family's small-form router. The restore verb is
/// the family's ONLY form (every other verb is a menu fire);
/// cleared by the shared cancel path so a stale form can never arm a
/// later Enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigForm {
    /// `rig restore` — the gwbk FILE to restore (a Confirm gate arms
    /// next; the action's own pre-checks fence missing/empty files).
    RestoreFile,
}

/// The raw-line ring cap for the rig logs pane — the 06-03
/// [`LOG_RING_CAP`] discipline applied to compose passthrough lines
/// (a weekend-long `rig logs -f` pane cannot OOM the process).
pub const RIG_LOG_RING_CAP: usize = 10_000;

/// The Rig screen's data (06-06): the one-shot status summary (the
/// allowlist [`RigStatusResult`] rendered as containers + state) and
/// the raw compose-logs stream pane (a second ring, reusing the
/// 06-03 pattern).
#[derive(Debug, Default)]
pub struct RigData {
    /// The latest accepted status (None until the first load / after
    /// an error — the pane renders its Loading state).
    pub status: Option<ignition_core::actions::rig::RigStatusResult>,
    /// Why the status load errored, when it did.
    pub status_error: Option<String>,
    /// A status load is in flight (the entry/refresh busy guard).
    pub status_busy: bool,
    /// Whether the logs pane is ON (the `l` toggle + the menu's
    /// `logs` verb) — the pane's identity survives screen exits so
    /// re-entry resumes the stream (the tail/alarms re-arm shape).
    pub logs_on: bool,
    /// The retained raw compose lines — a ring capped at
    /// [`RIG_LOG_RING_CAP`] (evict front). Cleared on every stream
    /// (re)spawn — compose `logs --tail` has no `since` resume, and
    /// overlapping the tail would double-render.
    pub logs: VecDeque<String>,
    /// Total lines evicted from the ring's front (turnover
    /// accounting — the status row's honesty counter).
    pub logs_dropped: usize,
    /// The stream worker's shutdown switch — `send(true)` on leaving
    /// the screen and toggling the pane off (a dropped sender also
    /// stops the worker).
    pub logs_shutdown: Option<watch::Sender<bool>>,
    /// The armed rig form (the Input modal's routing slot —
    /// [`RigForm`]); cleared by the shared cancel path.
    pub pending_form: Option<RigForm>,
}

impl RigData {
    /// Append one streamed compose line, evicting from the front at
    /// the cap — THE ring discipline (the 06-03 twin).
    pub fn push_log_line(&mut self, line: String) {
        while self.logs.len() >= RIG_LOG_RING_CAP {
            self.logs.pop_front();
            self.logs_dropped += 1;
        }
        self.logs.push_back(line);
    }
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
    /// Tags screen data (06-04) — provider list, browse stack, detail.
    pub tags: TagsData,
    /// Projects screen data (06-05) — list, project detail,
    /// resource detail.
    pub projects: ProjectsData,
    /// Rig screen data (06-06) — the status summary + the raw logs
    /// pane.
    pub rig: RigData,
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

#[cfg(test)]
mod tests {
    use super::{ACTIONS, PROJECT_ACTIONS};

    /// The dashboard menu's wait labels are display PROSE (06-10) —
    /// the executor arms in update.rs match these exact strings, and
    /// routes.rs carries the clap-exact spellings (never these).
    #[test]
    fn dashboard_actions_use_display_prose_wait_labels() {
        assert_eq!(
            ACTIONS,
            [
                "version",
                "connections",
                "wait for gateway up",
                "wait for restart complete",
                "wait for module ready",
                "doctor",
                "restart",
            ]
        );
    }

    /// The Projects menu's structure contract (06-10): the 11 verbs
    /// in their LOCKED flat order (the selection index space — the
    /// update.rs executor arms key off these exact spellings), every
    /// verb unique, groups contiguous (the render's header detection
    /// depends on it), and exactly the three noun groups.
    #[test]
    fn project_actions_are_grouped_and_ordered() {
        assert_eq!(
            PROJECT_ACTIONS.iter().map(|a| a.verb).collect::<Vec<_>>(),
            [
                "new",
                "copy",
                "rename",
                "set",
                "delete",
                "import",
                "export",
                "resource put",
                "resource delete",
                "webdev deploy",
                "webdev status",
            ]
        );
        let mut verbs = PROJECT_ACTIONS.iter().map(|a| a.verb).collect::<Vec<_>>();
        verbs.sort_unstable();
        verbs.dedup();
        assert_eq!(verbs.len(), PROJECT_ACTIONS.len(), "every verb unique");

        let groups: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for action in &PROJECT_ACTIONS {
                if seen.last() != Some(&action.group) {
                    seen.push(action.group);
                }
            }
            seen
        };
        assert_eq!(groups, ["project", "resource", "webdev"]);
        // Contiguity: the collapsed group walk visits exactly 3
        // groups — a mid-list group change-back would lengthen it.
        let changes = PROJECT_ACTIONS
            .windows(2)
            .filter(|w| w[0].group != w[1].group)
            .count();
        assert_eq!(changes, 2, "exactly two group boundaries");
    }
}
