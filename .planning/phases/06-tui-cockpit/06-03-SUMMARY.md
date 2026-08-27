---
phase: 06-tui-cockpit
plan: 03
subsystem: ui
tags: [ratatui, tokio, log-tail, ring-buffer, alarms, ack-flow, elm-architecture, wiremock]

# Dependency graph
requires:
  - phase: 06-tui-cockpit plan 01
    provides: the cockpit shell (AppEvent rail, Elm state/update, modal infra, context resolution, Screen enum with all six variants)
  - phase: 06-tui-cockpit plan 02
    provides: the worker patterns (one-shot spawn_action, interval refresh, era/shutdown conventions), the Confirm≡--yes gate, PanelState tri-state render, screen-owned modal overlays
  - phase: 02-foundation
    provides: actions::logs (tail with the Send sink, list/loggers verbs — 02-04/02-05 contracts)
  - phase: 05-tags-webdev
    provides: actions::tags alarms family (active/history/ack with the WebDev precondition and 05-08 prefix expansion inside the action)
provides:
  - Logs screen: live tail via the channel-sink worker, 10k ring buffer (weekend-proof), render-side level filter + min_level tail restart, follow/scrollback, loggers menu with confirm-gated set/reset
  - Alarms screen: 5s active poll, full-UUID table (copy-paste-verbatim), 24h history browse via the result modal, username-required ack form (unguarded, prefix ids pass through)
  - The two streaming worker patterns proven for 06-04: channel-sink tail (workers/tail.rs) and interval poll (workers/watch.rs)
  - Screen-scoped worker lifecycle: set_screen arms/stops per-screen workers; profile switch stops + clears + re-spawns per active screen; run_loop teardown signals every rail
  - routes.rs rows for the logs family (5 leaves) + tags-alarms family (3 leaves) — 23 rows total
affects: [06-04-tags, 06-05-projects, 06-06-projects-rig]

# Tech tracking
tech-stack:
  added: [] # no new deps — the plan reused the existing rail/watch/ratatui stack
  patterns:
    - "Channel-sink tail worker: the action's sync Send sink closure sends AppEvent::LogLine (unbounded mpsc, sync send legal); the WHOLE tail future select!s against the screen's watch shutdown — leaving stops it between entries"
    - "Screen-scoped workers: per-screen shutdown watches (logs.tail_shutdown, alarms.shutdown), armed by set_screen, NO era bump (the global era stays world-scoped = profile switches only)"
    - "Ring discipline: VecDeque capped at 10_000 with front eviction + dropped counter; filter applies AT RENDER over retained entries AND as min_level on tail restart (resume since = ring's newest timestamp — no duplicate flood on re-entry)"
    - "Poll degrade for tables: Ok replaces rows, Err replaces rows with the honest error state (never stale-rows-with-error), selection clamped into new rows"
    - "Ack-refresh trigger: ActionDone label 'alarms ack' fires spawn_alarms_once (busy-guarded one-shot poll) — the table reflects an ack NOW, not ≤5s later"

key-files:
  created:
    - crates/ignition-tui/src/workers/tail.rs
    - crates/ignition-tui/src/workers/watch.rs
  modified:
    - crates/ignition-tui/src/ui/logs.rs
    - crates/ignition-tui/src/ui/alarms.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/workers/mod.rs
    - crates/ignition-tui/src/lib.rs

key-decisions:
  - "Tail spawn resumes at the ring's newest timestamp (since = ring.back().timestamp) — re-entry and filter restarts never duplicate-flood; plan's since_ms param resolved to ring-state, not a fixed window"
  - "Per-line era stamps deliberately absent (plan-locked): the ring's turnover is the acceptance policy; the tail's scope is its shutdown watch, so the global era stays world-scoped (profile switches) — a screen-entry era bump would have retired the dashboard refresh worker's events"
  - "Screen-scoped workers live and die by their own watches, armed in set_screen — no global era churn on Tab (the dashboard's interval worker keeps running across tabs by design)"
  - "History browse rides the one-shot pattern + the LOCKED scrollable result modal over the trailing 24h (the plan's AlarmsData.view: Active|History pane sketch collapsed into the one-mechanism display; the CLI's --start/--end stay on the command line)"
  - "logs loggers set parses one `LOGGER LEVEL` input line (case-normalized, 7 wire levels validated) BEFORE the Confirm gate arms — bad lines open the error modal and arm nothing (the clap value_enum refusal's twin)"
  - "Ack is NOT confirm-gated and Enter is a NO-OP until the username is non-empty (the 3-arg wire form); the id passes AS SHOWN — the action expands prefixes itself (05-08 inherited)"
  - "The alarms table drops the priority column at 80 cols: the FULL 36-char UUID is a fixed leading column that must never compress (the must-have); priority rides the ack result modal"
  - "Poll errors REPLACE rows with the error state (degrade convention) — stale rows behind an error banner would be a lie about liveness"

patterns-established:
  - "Screen-scoped worker lifecycle: set_screen stops the old screen's workers and arms the new screen's; profile switch + run_loop teardown sweep every rail (the pattern 06-04's tag watch and 06-06's rig screens copy)"
  - "Two-field form modals: Modal::Ack mirrors Modal::ProfileAdd (Tab toggles fields, screen-owned overlay rendering, per-field cursor glyph) — the username-required gate lives in update's Enter handler"
  - "ActionDone label routing: update special-cases a landed label ('alarms ack') to fire a follow-up worker — the refresh-trigger seam for action verbs that change polled state"

# Metrics
duration: 36min
completed: 2026-08-27
---

# Phase 6 Plan 3: Logs + Alarms Summary

**Live Logs screen (channel-sink tail into a 10k-entry ring with render-side level filtering and follow/scrollback) and Alarms screen (5s full-UUID poll, 24h history browse, username-required ack form) — the two streaming worker patterns 06-04 reuses**

## Performance

- **Duration:** 36 min
- **Started:** 2026-08-27T19:36:35Z
- **Completed:** 2026-08-27T20:13:24Z
- **Tasks:** 3
- **Files modified:** 11 (2 created, 9 modified)

## Accomplishments
- The Logs screen streams the gateway tail live into a memory-bounded ring (10,000 entries, front eviction + dropped counter — a weekend-long tail cannot OOM); leaving the screen stops the worker even between entries (select! against the shutdown watch)
- The level filter works retroactively (render-side over retained entries — filtering the query alone would hide already-received lines) AND restarts the tail with the new min_level; scrollback + follow compose over the FILTERED view
- The loggers family is fully reachable from the Logs screen with Confirm ≡ --yes on the CLI's two guarded verbs; the tail keeps streaming independently (separate workers)
- The Alarms screen polls active alarms every 5s showing FULL UUIDs (fixed 36-char column that never compresses at 80 cols), degrades honestly on poll errors, browses journal history, and acks via the username-required form with ids passing as-shown to the expanding action
- 37 new tests (570 → 607 workspace): ring cap eviction, render-side filter, follow/scroll machine, wiremock tail stream/shutdown/error proofs, poll fill/stale-drop/degrade, ack gate + refresh trigger, TestBackend renders

## Task Commits

Each task was committed atomically:

1. **Task 1: Logs screen — tail worker, ring buffer, level filter, scrollback** - `62737bb` (feat)
2. **Task 2: Loggers actions + logs-family route rows** - `2305655` (feat)
3. **Task 3: Alarms screen — active poll, history, ack modal** - `ded0d38` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-tui/src/workers/tail.rs` - the channel-sink tail worker (actions::logs::tail AS-IS + LogLine-forwarding Send sink, shutdown select, resume-past-ring since) + spawn/stop helpers
- `crates/ignition-tui/src/workers/watch.rs` - alarms_worker (5s interval poll of tags_alarms_active, era-stamped), spawn/stop, spawn_alarms_once (the ack-refresh trigger)
- `crates/ignition-tui/src/ui/logs.rs` - level-coded stream pane (ERROR/FATAL red, WARN yellow, DEBUG/TRACE dim), filtered window math, status row (filter/follow/ring/dropped)
- `crates/ignition-tui/src/ui/alarms.rs` - full-UUID table with tri-state, poll-age status row, the screen-owned ack form overlay
- `crates/ignition-tui/src/state.rs` - LogLevelFilter, LogsData (ring/filter/follow/scroll + rail), AlarmsData, Modal::LogsActions/Ack, PendingAction::LoggersSet/LoggersReset, PendingInput::LoggersSearch/LoggersSetLine, LOG_ACTIONS
- `crates/ignition-tui/src/update.rs` - LogLine/Alarms arms, set_screen (screen-scoped worker hooks), logs+alarms keymaps, LogsActions nav/executor, LOGGER LEVEL parse, ack form handling, ack-refresh trigger, history spawn, profile-switch sweep
- `crates/ignition-tui/src/event.rs` - AppEvent::LogLine(LogEntry) (unera'd, plan-locked), AppEvent::Alarms { era, Result }
- `crates/ignition-tui/src/routes.rs` - logs family (bare `logs`, `logs download`, `logs loggers`, set/reset) + tags-alarms family rows
- `crates/ignition-tui/src/lib.rs` - run_loop teardown signals the tail + alarms rails
- `crates/ignition-tui/src/ui/mod.rs` - LogsActions + Ack modal rendering/delegation; placeholder test moved to still-unwired screens

## Decisions Made
- Tail resume: `since_ms` = the ring's newest timestamp on every spawn — re-entry and filter restarts continue exactly past what's retained (no duplicates, no gap)
- No per-line eras (plan-locked) and no screen-entry era bumps: the global era remains world-scoped (profile switches); a Tab-driven bump would have retired the dashboard refresh worker's in-flight events
- The 24h history window is the TUI's fixed browse policy via the one-shot pattern + result modal (the plan's Active|History pane sketch collapsed into the LOCKED one-mechanism display; AlarmRow carries no timestamp field, so the plan's "timestamp" column renders source/name/state instead — the row type re-used verbatim per its own instruction)
- Logs pending modal slots ride the existing dashboard pending fields (cockpit-global in practice — exactly one modal open at a time; Esc-clears-pending invariant unchanged)
- The ack key_link's `tags_alarms_ack` call lives in update.rs (all spawns live there per the 06-01/02 architecture — ui/alarms.rs renders the modal and table); the ui↔update contract is the selected row's id AS SHOWN

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Placeholder-screen tests asserted the not-yet-wired blocks this plan replaces**
- **Found during:** Task 1 (ui/mod.rs) and Task 3
- **Issue:** `chrome_renders_tab_bar_and_placeholder_pane` pinned the Logs placeholder and `screen_dispatch_renders_the_active_screen` pinned Alarms — both screens became real this plan
- **Fix:** The tests moved onto still-unwired screens (Tags for task 1, Projects for task 3) — the placeholder contract stays proven without blocking this plan's screens
- **Files modified:** crates/ignition-tui/src/ui/mod.rs
- **Verification:** cargo test -p ignition-tui green
- **Committed in:** 62737bb (task 1) and ded0d38 (task 3)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Test-fixture retarget only. No scope creep; plan sketch honored everywhere it conflicted with must-have truths (must_haves override sketches per the phase's standing rule).

## Issues Encountered
- ratatui Table compresses fixed `Length` columns when constraint totals exceed the pane width — the alarms table's five columns truncated the 36-char UUID at 80 cols. Resolved by sizing constraints to 72+3 spacing ≤ 76 inner width and dropping the priority column (it rides the ack result modal); the UUID column is now structurally uncroppable at 80 cols
- The ack overlay's prefixed target line (`alarm  <uuid>`) exceeded the half-width modal's 38-char inner width — the UUID now rides bare on its own line (also the better copy-paste shape)
- `workers::watch` (the new module) collided with the `use tokio::sync::watch` import in workers/mod.rs — the import moved to fully-qualified paths

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 06-04 (Tags) plugs straight in: the channel-sink and interval-poll patterns are proven, the screen-scoped lifecycle (set_screen/switch_profile/teardown) is three lines per screen, and the two-field form modal shape is established
- The live-rig smoke (Logs streaming while navigating tabs, alarms table against a route-deployed rig) remains the phase-end human verification item; all state machines and worker loops are headless-proven (wiremock + TestBackend)
- routes.rs now carries 23 rows; 06-06's bidirectional clap-walk test inherits them

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-27*

## Self-Check: PASSED

All 8 key files exist on disk; all 3 task commits (62737bb, 2305655, ded0d38) found in git history; key_link greps verified (send(AppEvent::LogLine in tail.rs, actions::logs::tail ×2 in tail.rs, tags_alarms_active ×4 in watch.rs, tags_alarms_ack in update.rs [the spawn site per the 06-01/02 architecture], VecDeque + 10_000 cap in state.rs); logs.rs 355 lines (min 60 must-have), alarms.rs 314 (min 50), tail.rs 266, watch.rs 191.
