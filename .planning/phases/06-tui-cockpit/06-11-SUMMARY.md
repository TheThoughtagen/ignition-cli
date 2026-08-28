---
phase: 06-tui-cockpit
plan: 11
subsystem: ui
tags: [ratatui, tui, rig, render, readme, keymap, docs]

# Dependency graph
requires:
  - phase: 06-09
    provides: refire_tags_current_level (the Tags 'r' refresh this plan documents)
  - phase: 06-10
    provides: menu_nav vim motions + noun-grouped Projects menu (the keys this plan documents)
provides:
  - Readable grouped rig status summary render (STATE headline + identity/services/volumes sections)
  - README TUI keymap synchronized with every gap-closure key
affects: [07-interop, verification (gsd-verify-work 6)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Sectioned summary render: bold headline + blank-separated bold section headers + indented aligned field rows (width-9 label column + VALUE_COLUMN const)"
    - "fit_tail: width-aware value fitting from the path's discriminating END with leading ellipsis (content-driven layout, the 06-10 lesson)"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/ui/rig.rs
    - README.md

key-decisions:
  - "Rig summary structure = STATE headline (UP/DOWN · PORTS free/held, one bold row) + identity/services/volumes sections — the plan's trial/snapshot sketch sections don't exist on RigStatusResult; actual fields drive the grouping"
  - "summary_lines takes the pane's inner width and tail-fits the compose path (leading …) — long real-world paths stay identifiable instead of clipping at the border"
  - "project row renders ONLY when it differs from rig (COMPOSE_PROJECT_NAME rename is the one case the distinction matters); both are plan.name today"
  - "Result-modal README wording pinned against the code: PgUp/PgDn scroll line-wise, Ctrl-d/Ctrl-u are the 10-line half-page — no conflation"

patterns-established:
  - "Border-stripping inner() test helper for exact-row structural assertions on bordered panes (blank-line separation, section headers)"
  - "README keymap bullet is the single home for per-screen keys + modal motion sets; screen table rows stay semantic"

# Metrics
duration: 20min
completed: 2026-08-28
---

# Phase 6 Plan 11: Gap Closure — Rig Status Render + README Keymap Sync Summary

**Rig screen re-rendered as a grouped, blank-separated summary (STATE UP/DOWN headline, identity/services/volumes sections, width-fitted compose path) and the README TUI keymap now documents every gap-closure key (Tags `r`, modal vim motions, noun-grouped menu) — the final 06-UAT gap closed**

## Performance

- **Duration:** 20 min
- **Started:** 2026-08-28T12:09:46Z
- **Completed:** 2026-08-28T12:30:21Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Diagnosed and fixed 06-UAT test 13 [minor] ("rig status doesn't show well"): the old render was one dense undifferentiated block — state buried mid-list, 1-char label gaps, real compose paths clipping mid-path at the border, comma-packed port cells, volumes run together
- New render: bold `STATE  UP · PORTS  held` headline, blank-separated bold sections (identity / services / volumes), indented aligned field rows, volumes one per row, port mappings joined with `, `, tail-fitted compose path — verified visually via TestBackend dumps for both UP and DOWN shapes at 80×24
- README TUI keymap synced: Tags `r` refresh (deepest-visible refire), vim motions in modals (`j`/`k`/`g`/`G` in list-bearing menus; `j`/`k` + Ctrl-d/Ctrl-u half-page in the Result modal), prose-labeled noun-grouped Projects menu noted on the `a` bullet; exit table cross-checked (`session_not_prunable` already present from 06-07)

## Task Commits

Each task was committed atomically:

1. **Task 1: Diagnose and fix the rig status summary render** - `bef9a51` (feat)
2. **Task 2: Sync the README TUI keymap with gap-closure keys** - `3ee8869` (docs)

## Files Created/Modified
- `crates/ignition-tui/src/ui/rig.rs` - Grouped sectioned summary render (STATE headline, identity/services/volumes, fit_tail width fitting, ports_cell spacing, project-only-when-differs) + extended render tests (section skeleton, both shapes, long-path fitting)
- `README.md` - Keybindings bullet expanded with Tags `r`, modal vim motions, prose menu note

## Decisions Made
- Section names follow the actual `RigStatusResult` fields (state/identity/services/volumes), not the plan's placeholder sketch (trial/snapshot) — those fields don't exist on the allowlist model; must-have truth #1 ("grouped key-value rows with clear section separation") is what governs
- `summary_lines` became width-aware (`inner.width` threaded from `render_summary`) so the compose path tail-fits with a leading ellipsis — content-driven layout per the 06-10 lesson
- The headline merges state + ports on one row (`STATE  UP · PORTS  held`) so the full 11-row section skeleton fits the ~11-inner-row summary pane of the 80×24 logs-on split
- README wording checked against the code before writing: PageUp/PageDown are line-wise in the Result modal; only Ctrl-d/Ctrl-u step 10

## Deviations from Plan

None - plan executed exactly as written. (The plan itself flagged "root cause TBD at diagnosis"; the diagnosis is recorded above and in Decisions Made.)

## Issues Encountered
- A throwaway visual-dump test's automated removal briefly mangled the tests module (unbalanced brace) — caught immediately by compile, repaired, full suite re-run green. No residual impact.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 6 is COMPLETE: all 11 plans (6 original + 5 gap-closure) have summaries; the 13 UAT gaps are addressed or explicitly owned (monochrome/color UX themes are backlog by UAT triage, not gap closure)
- Ready for `/gsd-verify-work 6`, then Phase 7 planning (interop — its CLI additions must land TUI surfaces in the same plan per the CI-enforced coverage tripwire)

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-28*

## Self-Check: PASSED
