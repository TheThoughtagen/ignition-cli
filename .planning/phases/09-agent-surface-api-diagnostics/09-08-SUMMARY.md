---
phase: 09-agent-surface-api-diagnostics
plan: 08
subsystem: tui
tags: [tui, ratatui, menu-parity, ci-contract, diagnostics, gap-closure]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics
    provides: "09-04/09-05 routes.rs Dashboard rows for the seven Phase 9 verbs + the ignition-core action fns (license/redundancy/gan/diagnostics)"
  - phase: 06-tui-cockpit
    provides: "ACTIONS locked list + Modal::Actions dynamic sizing + tui_coverage clap-walk harness"
provides:
  - "Seven Phase 9 verbs runnable from the TUI Dashboard actions menu via the SAME ignition-core actions the CLI uses (closes UAT test 10)"
  - "menu_label(path) — the single registry-path → menu-prose alias seam in routes.rs"
  - "dashboard_actions_menu_matches_registry — bidirectional routes↔menu parity CI test with pinned 31-row Dashboard-route count"
affects: [12-tuix, 13-mcp, future Dashboard command families]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pinned-count parity test: a Screen-mapped route row count is asserted, forcing conscious menu-entry updates in the same change (the 08-06 OutOfBand pre-declaration pattern, applied to the menu surface)"
    - "Single alias seam: menu_label() is the only place display-prose labels exist; parity CI resolves labels through it in both directions"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/tui_coverage.rs

key-decisions:
  - "The seven new menu labels are clap-exact (no prose) — the menu_label seam exists ONLY for the 06-10 wait trio; parity CI resolves labels through the seam in both directions"
  - "bundle wait mirrors clap defaults (DEFAULT_INTERVAL 2s + 300s literal deadline) — deliberately NOT BUNDLE_DOWNLOAD_TIMEOUT, which is the per-request download override, a different semantic"
  - "bundle download rides None = the .part-rename timestamped fallback naming (differs from backup download, which passes the profile stem)"
  - "Menu-parity pinned count = 31 Dashboard rows; justified exclusions commented: tui (the cockpit), status/modules/metrics/sessions (panels), sessions terminate (panel-row modal), profile use/list/add (switcher modal)"
  - "The Actions modal fit assertion moved to an 80x30 frame — 22 entries + 4 chrome rows = 26 exceeds the v1.0-sized 80x24; at 24 rows the modal clamps by design"

patterns-established:
  - "Route-row pinned count as pre-declaration: adding a Dashboard-mapped route without a menu entry fails CI in the same change (the 09-04/09-05 blind spot is structurally unreachable)"

# Metrics
duration: 138min
completed: 2026-09-08
---

# Phase 9 Plan 08: TUI Menu Surface + Routes↔Menu Parity Contract Summary

**The seven Phase 9 morning-check verbs now run from the Dashboard actions menu through the same ignition-core actions the CLI uses, with a bidirectional pinned-count parity test making the routes-without-surface blind spot structurally unrepeatable.**

## Performance

- **Duration:** 138 min (a large share was cargo lock contention with the parallel 09-07 agent rebuilding ignition-core)
- **Started:** 2026-09-08T02:44:14Z
- **Completed:** 2026-09-08T05:02:41Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- ACTIONS grew 15→22 with clap-exact labels and 09 provenance comments; seven ungated direct-spawn executor arms dispatch through `ignition_core::actions::{license,redundancy,gan,diagnostics}` — no second construction, results render via the generic ActionDone pretty-JSON path
- `menu_label(path)` installed in routes.rs as the single path→label alias seam (wait trio only; everything else clap-exact by default)
- `dashboard_actions_menu_matches_registry` CI test: MENU_HOSTED[22] ⊆ Dashboard rows, both-direction label resolution through the seam with exactly-one cardinality, ACTIONS↔MENU_HOSTED cardinality equality, and the pinned 31-row Dashboard-route count that forces conscious same-change updates
- Both negative proofs demonstrated live then reverted: dropping "lint" from ACTIONS fails assertion (c); a fake Dashboard route fails the pinned count (32 ≠ 31)
- UAT test 10's gap closed: the seven verbs are reachable, runnable, and parity-enforced; the tab-indicator visual issue stays deferred to Phase 12 per the UAT diagnosis

## Task Commits

Each task was committed atomically:

1. **Task 1: ACTIONS 15→22 + seven executor dispatch arms** - `87d8672` (feat)
2. **Task 2: routes↔menu parity contract (menu_label seam + CI test with negative proof)** - `f98812b` (test)

## Files Created/Modified
- `crates/ignition-tui/src/state.rs` - ACTIONS [&str; 22] with 09 provenance comments; locked-list pinned test extended
- `crates/ignition-tui/src/update.rs` - seven dispatch arms + the lint dead-verb fix; G-test bottom index 14→21
- `crates/ignition-tui/src/ui/mod.rs` - Actions modal fit assertion re-framed to 80x30 (test only; render code untouched)
- `crates/ignition-tui/src/routes.rs` - menu_label alias seam (7-line function + doc comment)
- `crates/ignition-cli/tests/tui_coverage.rs` - the parity test with pinned count, exclusions, and seam-keyed assertions

## Decisions Made
- The seven new labels are clap-exact — no new prose aliases; `menu_label` stays minimal to the 06-10 wait trio so the seam cannot become a dumping ground
- bundle wait's 300 s deadline is a site literal with a comment forbidding `BUNDLE_DOWNLOAD_TIMEOUT` reuse (per-request download override ≠ poll deadline)
- Exclusion dispositions recorded in the test: `sessions terminate` is panel-row modal-driven; the profile trio rides the global `p` switcher modal — neither is a menu verb
- The 80x24 Actions fit pin was re-framed to 80x30 rather than introducing modal scrolling — clamping at small frames is the existing 06-10 mechanism and stays the design

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The "lint" menu verb was dead since 07-04 — Enter did nothing**
- **Found during:** Task 1 (locating the plan's "after the lint arm" placement — no lint arm existed)
- **Issue:** `execute_menu_action` had no `Some("lint")` arm and `PendingInput::LintPaths` was never constructed anywhere — the consumer arm at update.rs:3219 was unreachable; pressing Enter on the menu's lint entry fell through to `_ => {}` (verified via `git log -S` back to the original b2c88ef commit)
- **Fix:** Added the `Some("lint")` arm opening the LintPaths Input modal (the 07-04 design intent; empty input falls to clear_pending, mirroring clap's required PATH)
- **Files modified:** crates/ignition-tui/src/update.rs
- **Verification:** cargo test -p ignition-tui green; the input-router consumption arm now reachable
- **Committed in:** 87d8672 (Task 1 commit)

**2. [Rule 3 - Blocking] The Actions modal fit test pinned 80x24, which 22 entries physically cannot fit**
- **Found during:** Task 1 verification (`menu_modals_fit_content_and_clamp_to_small_frames` failed: footer clipped at 80x24 — 22+4=26 rows needed)
- **Issue:** The plan's context claimed "NO ui/mod.rs change is needed" — true for the render code (dynamic .len() sizing confirmed) but false for the pinned test frame
- **Fix:** The fit assertion now renders at 80x30 via the existing `rendered_rows_sized` helper; the LogsActions fit + the 12-row clamp assertions stay as they were
- **Files modified:** crates/ignition-tui/src/ui/mod.rs (test module only)
- **Verification:** full ignition-tui suite green (199 passed)
- **Committed in:** 87d8672 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both fixes were required for correctness of the surface this plan ships: a dead menu verb inside the very menu being extended, and a pinned test that contradicted the mandated growth. No scope creep. Note: the plan's Task 1 placement instruction ("after the lint arm") referenced an arm that does not exist — the seven arms were placed after the last literal arm ("script run"), preserving menu order.

## Issues Encountered
- The parallel 09-07 agent's in-flight ignition-core edits (CoreError::Network gaining an `observation` field) transiently broke compilation of everything downstream; waited for their tree to become consistent before running verifications. No file-level conflicts — the scopes were disjoint as designed, and no 09-07 files were touched by this plan.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- UAT test 10's diagnosed gap is re-testable at verify-work (press `a` on the Dashboard → 22 verbs → Enter runs the shared action → pretty-JSON result modal)
- The parity contract extends naturally: Phase 13/14's `mcp`/`lsp`/`edit` rows are OutOfBand (not Dashboard) so the pinned count is unaffected; any future Dashboard family must extend ACTIONS + MENU_HOSTED + executor arms in one change or CI refuses
- The tab-indicator visual ambiguity (UAT Gap 2) remains assigned to Phase 12 (TUIX-03/04) — explicitly out of this plan's scope

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-08*

## Self-Check: PASSED

All 5 modified files exist on disk; both task commits (87d8672, f98812b) verified in git log; full `cargo test -p ignition-tui -p ignition-cli` EXIT 0 with zero failures.
