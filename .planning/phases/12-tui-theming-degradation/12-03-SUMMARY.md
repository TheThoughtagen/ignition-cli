---
phase: 12-tui-theming-degradation
plan: 03
subsystem: ui
tags: [ratatui, tui, theming, style-tokens, ci-enforcement, mono-degradation, tabs, cursor]

# Dependency graph
requires:
  - phase: 12-tui-theming-degradation/12-01
    provides: theme module — Palette slots, Tier, 4 themes × 4 authored tiers, theme::error/selection/warning style helpers with mono adaptation
  - phase: 12-tui-theming-degradation/12-02
    provides: ResolvedContext.palette + AppState.palette wired at construction and both adoption sites
provides:
  - Literal-free screens: zero Color::/style::Color/Color-as outside ui/theme.rs (grep-proven)
  - Dashboard error labels + table row selection rendered from palette slots (theme::error / theme::selection)
  - logs level_style themed through the palette (level_style(level, palette))
  - Tab bar visibly highlights the active screen via Tabs::select + emphasis token (09-UAT Gap 2 UI half)
  - Terminal cursor hidden for the cockpit's lifetime (09-UAT Gap 2 cursor half)
  - CI style-tokens grep step — tokenization is machine-enforced from this commit forward
affects: [12-04, uat-verification, future-ui-screens]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Slot-equality-only test doctrine: buffer cells compared against palette slots, never Color literals"
    - "Whole-palette equality against the mono-AUTHORED palette for all-Reset pins (auto-covers future fields)"
    - "Palette threaded as minimal pure dependency (&theme::Palette) into private render fns — not &AppState"
    - "Modifier composition inline at call sites; helpers carry color only (research doctrine)"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/ui/dashboard.rs
    - crates/ignition-tui/src/ui/logs.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/context.rs
    - .github/workflows/ci.yml

key-decisions:
  - "12-03: CI tokenization grep landed ATOMICALLY with the migration (single commit with the last literal escape) — the grep goes red the moment it's added, so state.rs/context.rs test-code literals migrated in the same commit as the CI step"
  - "12-03: all-Reset pins compare WHOLE-PALETTE equality against the mono-authored palette — strictly stronger than a slot walk (auto-covers any future Palette field) and tokenization-clean"
  - "12-03: palette threaded into dashboard render fns as &theme::Palette (the minimal pure dependency), NOT &AppState — render_sessions keeps &AppState since it already held it"
  - "12-03: error-label buffer test identifies the label by 'inner row content == error' — dead-gateway message prose can start with the same word and rides Span::raw, so it must not vote"
  - "12-03: terminal.hide_cursor() is best-effort (let _ =) — a failed hide on an unsupported terminal must not kill the cockpit; restore path re-shows either way"

patterns-established:
  - "Slot-equality testing: assert cell.fg/bg/modifier against palette slots; the theme's hues are the theme's business"
  - "Both-tier render proofs: color tier (bg fill / fg hue) AND mono tier (REVERSED / BOLD) asserted at the buffer level"

# Metrics
duration: 18min
completed: 2026-09-14
---

# Phase 12 Plan 03: Literal Migration + Tab-Bar Fix + CI Enforcement Summary

**Zero Color literals outside ui/theme.rs (machine-enforced by a new CI grep), dashboard/log colors on palette slots at both tiers, and the 09-UAT Gap 2 tab-indicator fix (Tabs::select + emphasis token + hidden cursor)**

## Performance

- **Duration:** 18 min
- **Started:** 2026-09-14T16:21:49Z
- **Completed:** 2026-09-14T16:40:44Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments
- All 10 planned Color literals migrated (dashboard ×5, logs ×2 production + ×3 test-side) plus 5 unenumerated test-code occurrences — the exact CI grep pipeline over crates/ returns EMPTY, with theme.rs the sole site (152 Color:: references intact)
- 09-UAT Gap 2 closed: active tab selected via `Tabs::select` + `highlight_style` (emphasis fg + inline BOLD — BOLD survives the mono tier) and the terminal cursor hidden for the cockpit's lifetime
- Tokenization made permanent: CI check job now fails any future `Color::` / `style::Color` / `Color as` escape from theme.rs
- Both-tier mono contracts proven at the render site: selection = bg fill at color tier / REVERSED at mono; errors = error-slot fg / BOLD at mono

## Task Commits

Each task was committed atomically:

1. **Task 1: dashboard literals to tokens, palette threaded into private render fns** - `91cd9a7` (feat)
2. **Task 2: logs level_style themed, 3 test literals migrated to palette comparisons** - `0e3fd05` (feat)
3. **Task 3: tab-bar highlight + cursor hide (09-UAT Gap 2) + CI tokenization grep** - `5fed72c` (feat)

**Plan metadata:** pending (docs: complete plan)

## Files Created/Modified
- `crates/ignition-tui/src/ui/dashboard.rs` — 5 literals → theme::error/selection; &theme::Palette threaded into render_status/modules/metrics; buffer tests for error slot + both-tier selection
- `crates/ignition-tui/src/ui/logs.rs` — level_style(level, palette) via theme::error/warning; span scoping preserved; slot-based color tests + mono BOLD proof
- `crates/ignition-tui/src/ui/mod.rs` — render_tab_bar rewritten with Tabs::select + token highlight_style; token-driven tab-bar test (BOLD + emphasis slot + inactive empty modifier)
- `crates/ignition-tui/src/lib.rs` — terminal.hide_cursor() right after ratatui::init() (best-effort)
- `crates/ignition-tui/src/state.rs` — default-palette test migrated to whole-palette equality against the mono-authored palette (deviation)
- `crates/ignition-tui/src/context.rs` — assert_all_reset helper migrated to whole-palette equality (deviation)
- `.github/workflows/ci.yml` — style-tokens grep step in the check job (after rust-cache, before fmt)

## Decisions Made
- CI grep landed ATOMICALLY with the final literal migration — the grep goes red the moment it's added, so it, the tab-bar fix, and the state.rs/context.rs test migrations share one commit
- All-Reset pins use whole-palette equality against `Theme::by_name("mono").mono` — strictly stronger than the slot walk (auto-covers any future `Palette` field) and tokenization-clean
- Palette threaded as `&theme::Palette` (minimal pure dependency) into private dashboard render fns, not `&AppState` — render_sessions already held `&AppState` and uses `&state.palette`
- Error-label buffer test matches only rows whose inner content is exactly "error" — dead-gateway message prose can start with the same word and rides Span::raw
- `hide_cursor()` result discarded deliberately (best-effort chrome; restore path re-shows the cursor on every exit path)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Migrated 5 unenumerated test-code Color literals in state.rs and context.rs**
- **Found during:** Task 3 (CI grep insertion)
- **Issue:** The plan enumerated 10 literals, but the tree also carried test-code occurrences that its own verify gate (grep pipeline → EMPTY) would fail: `state.rs` (doc comment + `use ratatui::style::Color` + `Color::Reset` assertion in `app_state_default_palette_is_mono_reset`) and `context.rs` (import + `Color::Reset` assertion in `assert_all_reset`) — 12-02 test code
- **Fix:** Whole-palette equality against the mono-authored palette (`Theme::by_name("mono").mono`) replaces both slot walks; imports removed; doc comments reworded. The comparison is strictly stronger (PartialEq covers every field, including any future one) and stays honest to the slot-equality-only doctrine
- **Files modified:** crates/ignition-tui/src/state.rs, crates/ignition-tui/src/context.rs
- **Verification:** grep pipeline over crates/ returns empty; all 54 workspace suites green
- **Committed in:** 5fed72c (Task 3 commit — same commit as the CI step per the grep-atomicity mandate)

**2. [Rule 1 - Bug] hide_cursor returns an unused Result — clippy -D warnings red**
- **Found during:** Task 3 (cursor hide in lib.rs)
- **Issue:** ratatui 0.30's `Terminal::hide_cursor()` returns `io::Result`; the plan's bare call fails `clippy -- -D warnings`
- **Fix:** `let _ = terminal.hide_cursor();` with a comment: best-effort chrome — a failed hide must not kill the cockpit; the restore path re-shows the cursor either way
- **Files modified:** crates/ignition-tui/src/lib.rs
- **Verification:** clippy --workspace --all-targets -- -D warnings clean
- **Committed in:** 5fed72c (Task 3 commit)

**3. [Rule 1 - Bug] Error-label buffer test false-matched error message prose**
- **Found during:** Task 1 (test authoring)
- **Issue:** First detector matched any row containing "error" with a clean prefix — dead-gateway messages can START with "error" (Span::raw, fg Reset), so the assertion failed on the wrong cell; also byte-offset vs char-column mismatch with 3-byte border glyphs
- **Fix:** Match only rows whose inner content (trimmed of spaces/borders) is exactly "error", and convert the byte offset to a char column before indexing the buffer
- **Files modified:** crates/ignition-tui/src/ui/dashboard.rs (test only)
- **Verification:** dashboard suite 8/8 green
- **Committed in:** 91cd9a7 (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs)
**Impact on plan:** All fixes were required by the plan's own verify gates (grep-empty contract, clippy -D warnings). No scope creep; no API surface changes.

## Issues Encountered
- Verified upfront that ratatui 0.30.2's `Tabs` has NO `highlight_symbol` field (registry source inspected) — `Tabs::select + highlight_style` changes styles only, so the existing tab-bar layout tests (row 0 = " Dashboard │ Logs │ …") hold unchanged

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Success criterion 3 of the phase (tokens + CI enforcement) is now landed and machine-enforced
- 12-04 (remaining plan) proceeds against a literal-free tree: any new theming work must add slots to theme.rs; the CI grep enforces it from this commit forward
- 09-UAT Gap 2 todo in STATE.md can be closed at phase verification (visual confirmation remains a checkpoint concern)

---
*Phase: 12-tui-theming-degradation*
*Completed: 2026-09-14*

## Self-Check: PASSED

All 8 claimed files exist on disk; all 3 task commits (91cd9a7, 0e3fd05, 5fed72c) present in git log.
