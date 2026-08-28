---
phase: 06-tui-cockpit
plan: 10
subsystem: ui
tags: [ratatui, tui, modal, vim-motions, menu-ux, gap-closure]

# Dependency graph
requires:
  - phase: 06-tui-cockpit (06-01..06-09)
    provides: the cockpit's modal system, actions menus, and the routes/coverage + confirm-parity tripwires this plan had to keep green
provides:
  - Content-driven, frame-clamped modal geometry (footer hints always visible, no clipping at any terminal size)
  - Full vim motion set (j/k/g/G, Ctrl-d/Ctrl-u) in every menu/list/result modal
  - Display-prose dashboard ACTIONS labels ("wait for gateway up" etc.)
  - Noun-grouped Projects actions menu (project/resource/webdev sections with consequence descriptions)
affects: [06-tui-cockpit (06-11 README keymap sync), tui-ux-backlog (color work)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Shared menu_nav helper: one arrows+vim navigation function powering ALL list-bearing modals (selection index space per modal)"
    - "Shared line-builder pattern: render and height formula walk the SAME Vec<Line> builder so modal geometry always fits content exactly"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/state.rs

key-decisions:
  - "Menu labels are display prose; worker labels + routes.rs rows keep clap-exact spellings — executor arms match the prose strings on both sides"
  - "PROJECT_ACTIONS is ONE flat const of ProjectAction{group,verb,label,description} (the plan's simpler flatten-groups option); group contiguity + verb order are test-pinned"
  - "Result-modal Ctrl-d/Ctrl-u step is a fixed 10 lines (update is frame-blind by design; the Logs screen pages by the same step)"
  - "Descriptions kept within the LOCKED half-width modal budget (38 interior cols) — the plan's example texts ('irreversible') would have clipped"

patterns-established:
  - "menu_nav(selected, len, code): drop-in navigation for any future list modal — arrows + vim motions for free"
  - "projects_action_lines(selected): single source of truth for a modal's rendered rows AND its height"

# Metrics
duration: 41min
completed: 2026-08-28
---

# Phase 6 Plan 10: TUI Modal/Menu UX Gap Closure Summary

**Content-driven frame-clamped modal geometry (footer hints never clip), full vim motions (j/k/g/G, Ctrl-d/Ctrl-u) in every modal, prose wait labels, and a noun-grouped Projects menu with consequence descriptions — routes.rs and confirm-gating untouched**

## Performance

- **Duration:** 41 min
- **Started:** 2026-08-28T11:25:33Z
- **Completed:** 2026-08-28T12:07:06Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- Every modal height formula now counts its actual rendered rows (entries + hint + blank + 2 borders) and clamps to the frame — the UAT's clipped "Enter to run · Esc to cancel" footers on Actions and LogsActions are gone, modals never clip on small terminals (12-row frame test-pinned), and long Result content rides the existing scroll inside the clamped box
- The full vim motion set works in every menu/list/result modal via one shared `menu_nav` helper (six modals: Actions, LogsActions, TagsActions, ProjectsActions, RigActions, profiles switcher) plus a Result-modal scroll arm (j/k line, Ctrl-d/Ctrl-u half-page, g/G top/bottom) — arrows and PgUp/PgDn unregressed
- The dashboard menu reads as prose ("wait for gateway up" / "wait for restart complete" / "wait for module ready") while the worker labels and routes.rs rows keep clap-exact spellings — the tui_coverage clap-tree walk stays 3/3 green untouched
- The Projects actions menu is noun-grouped (bold **project / resource / webdev** headers, blank-separated) with `label — consequence` rows ("put — create or replace one file", "delete — remove one file from it") — the UAT's "delete vs resource delete" confusion answered by section scope + description
- Confirm-gating per verb unchanged: the gated_cli_verb parity test walks every PendingAction green

## Task Commits

Each task was committed atomically:

1. **Task 1: Content-driven, frame-clamped modal geometry** - `6a322f8` (fix)
2. **Task 2: Vim motions in every modal** - `063bbee` (feat)
3. **Task 3: Display-prose menu labels + noun-grouped Projects menu** - `71f8512` (feat)

## Files Created/Modified
- `crates/ignition-tui/src/ui/mod.rs` - corrected modal height formulas, frame clamp at the render site, sized render-test helper, geometry tests, the shared `projects_action_lines` builder + grouped render
- `crates/ignition-tui/src/update.rs` - `menu_nav` helper wired into all six list modals, Result-modal vim scroll arm (RESULT_HALF_PAGE=10), prose executor arms, Projects dispatch on `.verb`
- `crates/ignition-tui/src/state.rs` - prose ACTIONS labels, `ProjectAction` struct + regrouped PROJECT_ACTIONS const, structure-pin tests

## Decisions Made
- **Flat-with-group-field over the grouped-nested const** — the plan offered "flatten groups into one index space or track (group, idx) — pick the simpler one"; one const keeps `PROJECT_ACTIONS.len()` indexing, the render derives headers from group changes, and contiguity/order/uniqueness are test-pinned in state.rs
- **Fixed 10-line half-page step** — update() is frame-blind by design (pure sync, grep-enforced); the Logs screen's 10-line page convention is the codebase's reference step
- **Description width budget** — kept every `label — description` row within the LOCKED Ratio(1,2) modal's 38 interior columns (the plan's example "remove the whole project (irreversible)" would have clipped); the Confirm gates still carry the full consequence prose
- **PageUp/PageDown keep modifier-free behavior** — only the new char arms carry plain/ctrl guards, so the pre-existing paging keys are byte-identical in behavior

## Deviations from Plan

None - plan executed exactly as written (the flat-vs-grouped indexing choice was the plan's own "pick the simpler one" latitude, recorded as a decision above).

## Issues Encountered
- First full test run after the Task 2 edit appeared to hang and was killed at the 240s tool timeout; an immediate rerun completed green in seconds (build contention, not a test hang) — all subsequent runs clean
- One transient compile error round (misplaced helper inside the tests module via duplicate-anchor edit, and a usize/u16 match-arm mismatch) — both fixed before any commit; no intermediate broken states were committed

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Plan 06-11 (final gap closure: rig status render + README keymap sync) is unblocked — its README task should document the modal vim motions this plan added (j/k/g/G, Ctrl-d/Ctrl-u) alongside the Tags `r` key
- The remaining UAT UX feedback (monochrome/color, richer editor experience) stays backlog by plan scope

## Self-Check: PASSED
