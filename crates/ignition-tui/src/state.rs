//! The cockpit's Elm-style model (Phase 6 research, Pattern 4).
//!
//! PURE DATA — no I/O, no async, no terminals. Every mutation flows
//! through [`crate::update`]; workers never touch this struct directly.
//! Per-screen data structs are added by their plans (06-02..06-06); the
//! shell owns the navigation chrome only.

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
/// acceptance callbacks (wired through update, never stored futures) —
/// the shell ships the rendering + key routing only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// Yes/no confirmation — the TUI-side answer to the CLI's `--yes`
    /// guards (destructive verbs render this before their action).
    Confirm { title: String, body: String },
    /// A single-line text input, hand-rolled (no tui-input dep):
    /// char-append/backspace editing, Esc cancels.
    Input { title: String, buffer: String },
    /// Read-only result/report lines (errors land here too).
    Result_ { title: String, lines: Vec<String> },
}

impl Modal {
    /// The modal's title line.
    pub fn title(&self) -> &str {
        match self {
            Modal::Confirm { title, .. } | Modal::Input { title, .. } | Modal::Result_ { title, .. } => {
                title
            }
        }
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
