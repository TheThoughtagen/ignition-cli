---
phase: 06-tui-cockpit
plan: 01
subsystem: ui
tags: [ratatui, crossterm, tui, elm-architecture, tokio-select, event-stream]

# Dependency graph
requires:
  - phase: 05-tags-webdev
    provides: finished action surface (actions layer free fns on &dyn GatewayApi) the cockpit consumes
provides:
  - Event-driven async cockpit loop (AppEvent mpsc + crossterm EventStream + 250ms tick, tokio::select!)
  - Elm-style AppState/update with ALL six Screen variants, Focus arbitration, Modal infra (Confirm/Input/Result_)
  - profile→client context resolution (context::resolve/rebuild over public config fns)
  - logs::tail sink + Send (spawnable tail futures — the 06-03 seam)
  - Worker lifecycle conventions (era stamping, watch shutdown channel)
  - routes.rs coverage-registry scaffold (Mapping kinds + seed rows)
  - ign tui dispatch arm with TTY guard (exit 2 usage-class on piped stdout)
affects: [06-02-dashboard, 06-03-logs, 06-04-tags, 06-05-alarms, 06-06-projects-rig]

# Tech tracking
tech-stack:
  added: [ratatui 0.30.2 (MSRV-floor pin), crossterm 0.29 (event-stream)]
  patterns:
    - "Elm architecture in-crate: AppEvent enum → pure update() → render(); update.rs banned from await"
    - "Workers are the ONLY actions callers; results stamp spawn-era, update drops stale-era events"
    - "Exhaustive EventStream match in select! (Ok/Err/None) — a Some(Ok())= arm permanently freezes input"
    - "ratatui::init/restore own the terminal lifecycle (panic hook included); resolve BEFORE init so config errors never flash the alt screen"

key-files:
  created:
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/workers/mod.rs
    - crates/ignition-tui/src/context.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/ui/{dashboard,logs,tags,alarms,projects,rig}.rs
  modified:
    - Cargo.toml
    - crates/ignition-tui/Cargo.toml
    - crates/ignition-core/src/actions/logs.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs

key-decisions:
  - "TuiExited renders NOTHING in every mode — the cockpit owns the alt screen; no envelope, no summary line (the plan's LOCKED stdout decision, intercepted in render_ok before mode dispatch)"
  - "context::resolve mirrors main.rs's private resolve_profile_context via public config fns only (choke files untouched); no-profile = NoActiveProfile exit 3; REQUIRED credential (authed surface, no degradation)"
  - "Ctrl-C quit checked BEFORE modal input routing — raw mode disables ISIG so Ctrl-C must quit even mid-input-modal; 'q' typing inserts into Input modal buffers, never quits behind a modal"
  - "The AppEvent select arm holds a live (unused) sender so recv() pends — zero workers in the shell but the rail stays armed for 06-02"
  - "Usage-class TTY refusal rides CoreError::InvalidInput (the generic exit-2 sibling; no dedicated Usage variant exists in the frozen taxonomy)"
  - "Screen::ALL + next()/prev() make Tab cycling data-driven; ALL six variants exist day one so screen plans never edit the enum"

patterns-established:
  - "Draw after EVERY processed select arm; the 250ms tick is only the staleness floor"
  - "Per-screen ui modules as whole-file placeholders — later plans OWN files, never edit ui/mod.rs dispatch beyond what exists"
  - "Local ENV_LOCK replication for env-mutating tests in crates that cannot import core's pub(crate) lock"

# Metrics
duration: 222min
completed: 2026-08-27
---

# Phase 6 Plan 1: TUI Cockpit Foundation Summary

**Async cockpit shell: ratatui 0.30 select-loop over EventStream/tick/AppEvent with pure Elm update, modal infra, and profile→client context — every 06-02..06-06 screen plan now purely adds modules**

## Performance

- **Duration:** 222 min (3h 42m)
- **Started:** 2026-08-27T14:27:26Z
- **Completed:** 2026-08-27T18:09:47Z
- **Tasks:** 3
- **Files modified:** 31 (19 plan-scoped + 12 fmt-drift sweep)

## Accomplishments
- `ign tui` runs: resolves profile context BEFORE terminal init, opens the tab-bar cockpit, quits on q/Esc/Ctrl-C, and restores the terminal on every path (pty-proven: alt-screen enter/leave + cursor hide/show paired, exit 0)
- The two seam fixes are in: `logs::tail` sink is `+ Send` (06-03's tail worker can spawn) and `context::resolve/rebuild` gives the TUI profile→client resolution without touching the CLI choke files
- 19 headless tests: TestBackend chrome buffer asserts (exact tab-bar row, placeholder borders, cell-level BOLD on the active tab, modal centering/Clear/structure) + keymap units (quit/modal/tab/Press-filter/input-buffer)
- Piped stdout refuses cleanly: exit 2, usage-class envelope, no panic

## Task Commits

Each task was committed atomically:

1. **Task 1: Deps, tail-sink Send fix, profile→client context** - `773f8e4` (feat)
2. **Task 2: AppEvent loop, AppState/update, run() entry, minimal Tui arm** - `4c80b35` (feat)
3. **Task 3: UI chrome, modal infra, routes scaffold, headless tests** - `8da8fa7` (feat)
4. **Style sweep (Rule 3 deviation)** - `1d57a5a` (style)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-tui/src/lib.rs` - run() entry: resolve → ratatui::init → select loop → restore on every path
- `crates/ignition-tui/src/event.rs` - AppEvent (Input/Tick/Error) + Shutdown watch type
- `crates/ignition-tui/src/state.rs` - AppState, Screen (all 6), Focus, Modal shapes, era counter
- `crates/ignition-tui/src/update.rs` - pure sync key routing (quit/nav/modal/input-buffer)
- `crates/ignition-tui/src/workers/mod.rs` - era stamping, shutdown channel, stale-era gate
- `crates/ignition-tui/src/context.rs` - resolve/rebuild over public config fns + isolated-env unit tests
- `crates/ignition-tui/src/routes.rs` - coverage-registry scaffold (tui + completions seed rows)
- `crates/ignition-tui/src/ui/mod.rs` - tab bar + dispatch + modal overlay (Rect::centered + Clear)
- `crates/ignition-tui/src/ui/{dashboard,logs,tags,alarms,projects,rig}.rs` - per-screen placeholders
- `crates/ignition-core/src/actions/logs.rs` - tail sink `+ Send` (signature, TailState, callers, test locals)
- `crates/ignition-cli/src/main.rs` - Tui arm (TTY guard + delegate), ActionOutput::TuiExited
- `crates/ignition-cli/src/render.rs` - TuiExited intercepts (renders nothing in every mode)
- `Cargo.toml` / `crates/ignition-tui/Cargo.toml` - ratatui 0.30.2 + crossterm 0.29 (event-stream), the only new deps

## Decisions Made
- TuiExited prints NOTHING on success in every mode (plan-locked); errors after restore flow the frozen envelope/taxonomy
- No-profile cockpit open = `CoreError::NoActiveProfile` (exit 3, hint names `profile add`) — the cockpit is a gateway surface and cannot open without a target
- The shell holds a live AppEvent sender (no workers yet) so the third select rail stays armed — 06-02 clones it
- Ctrl-C checked before modal routing; 'q' inserts into Input modal buffers (typed text can never quit)
- Feature-gated `TuiExited`/`IsTerminal`/render arms so the lean build (`--no-default-features`) stays warning-clean

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] render.rs + a minimal ui/mod.rs were required beyond the plan's file lists**
- **Found during:** Task 2 (run() entry)
- **Issue:** Task 2's file list omits render.rs, but the exhaustive `render_json`/`render_human` matches require TuiExited arms there; lib.rs's draw needs a ui module that Task 3 only later fleshes out
- **Fix:** Added the TuiExited render arms (intercept-in-render_ok + empty fallbacks) and a minimal chrome ui/mod.rs in Task 2, replaced by the full chrome in Task 3
- **Files modified:** crates/ignition-cli/src/render.rs, crates/ignition-tui/src/ui/mod.rs
- **Verification:** cargo build --workspace; all 138 ignition-cli tests green (no golden moved)
- **Committed in:** 4c80b35 / 8da8fa7

**2. [Rule 3 - Blocking] Accumulated rustfmt drift across Phase 5 files (CI gate would fail)**
- **Found during:** Task 3 verification (cargo fmt sweep)
- **Issue:** CI last ran 2026-08-22; all Phase 5 plans (05-02..05-08) landed without CI validation and carry rustfmt drift that fails `cargo fmt --all --check` on the current stable toolchain — this plan's push would have gone red
- **Fix:** One formatting-only sweep commit normalizing the 12 drifted files + the 06-01 files committed pre-sweep; zero behavior changes
- **Files modified:** 15 files (see style commit)
- **Verification:** cargo fmt --all --check clean; 542 workspace tests green; clippy -D warnings clean
- **Committed in:** 1d57a5a

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both were compilation/CI blockers inherent to the plan's own requirements. No scope creep.

## Issues Encountered
- macOS `script` pty smoke tests render into a 0x0 winsize (no TIOCSWINSZ), so visible content cannot be proven on a real TTY this way — lifecycle (alt-screen/cursor pairing, quit, exit 0) IS proven; content proof lives in the TestBackend buffer asserts, and visible-TTY confirmation belongs to the phase-end human verification per config
- ignition-core's `config::ENV_LOCK` is `#[cfg(test)] pub(crate)` — replicated the pattern locally in ignition-tui's tests exactly as the plan prescribed

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 06-02 (Dashboard) plugs directly in: spawn the refresh worker from run()'s resolved client, add the Refresh AppEvent variant, replace ui/dashboard.rs, append routes rows
- The select loop, modal infra, era/shutdown conventions, and TTY guard are all live — screens are purely additive
- Note: CI has not run since 2026-08-22; this plan's push will be the first validation of the fmt sweep + Phase 5 tail

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-27*

## Self-Check: PASSED

All 14 created files exist on disk; all 4 task commits (773f8e4, 4c80b35, 8da8fa7, 1d57a5a) found in git history; `ratatui::init` present in lib.rs (must-have artifact check).
