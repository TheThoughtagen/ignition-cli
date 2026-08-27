//! The cockpit's Elm-style model (Phase 6 research, Pattern 4).
//!
//! PURE DATA — no I/O, no async, no terminals. Every mutation flows
//! through [`crate::update`]; workers never touch this struct directly.
//! Per-screen data structs are added by their plans (06-02..06-06); the
//! shell owns the navigation chrome only.

use std::sync::Arc;
use std::time::Instant;

use ignition_core::actions::sessions::{SessionType, SessionsResult};
use ignition_core::client::ReqwestGatewayApi;
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
}

/// What a confirmed modal executes — the TUI-side `--yes`. Modal accept
/// (`y`) looks here; cancel (Esc) clears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    /// `ign restart` — the guarded global verb.
    Restart,
    /// `ign sessions terminate` on the selected row.
    TerminateSession { kind: SessionType, id: String },
}

/// What an accepted Input modal's buffer is for — the small-form router
/// (06-02 ships `wait module`'s id prompt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingInput {
    /// The module id for `wait module`.
    WaitModule,
}

impl Modal {
    /// The modal's title line.
    pub fn title(&self) -> &str {
        match self {
            Modal::Confirm { title, .. }
            | Modal::Input { title, .. }
            | Modal::Result_ { title, .. } => title,
            Modal::Actions { .. } => "actions",
            Modal::Profiles { .. } => "profiles",
            Modal::ProfileAdd { .. } => "profile add",
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
