---
phase: 10-eam-write-operations
plan: 07
subsystem: ui
tags: [ratatui, tui, modal, word-wrap, testbackend, regression-test, gap-closure, blast-radius]

# Dependency graph
requires:
  - phase: 10-eam-write-operations (10-04)
    provides: the Confirm modal as the TUI blast-radius gate (eam_preview_body body shape, EamPreview armed modals)
  - phase: 09-uat
    provides: 10-UAT.md gap 1 / test 10 — the mid-word clip diagnosis (single unwrapped Line in a Ratio(1,2) box)
provides:
  - Confirm modal renders the full blast-radius body: preview line word-wrapped, agents/pending tails readable, footer hint inside the box
  - wrapped_row_count greedy estimator (wrapped-row-aware content-driven height for the Confirm arm)
  - buffer-level regression test confirm_modal_wraps_the_blast_radius_body pinning non-clipping at 80x24
affects: [12-tuix-rendering, verify-work, uat]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Confirm-arm content-driven height via greedy token-packing wrapped-row estimate at the Ratio(1,2) inner width (frame/2 − 2); over-estimate is the safe side"
    - "ratatui fact: embedded \\n inside a single Line is whitespace-glyph whitespace, NOT a row break — multi-line bodies must be split into Lines before wrapping"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/ui/mod.rs

key-decisions:
  - "Confirm body split into real Lines before wrapping — ratatui renders embedded \\n inside a Span as literal whitespace glyphs (reflow.rs is_whitespace), so .wrap() alone would fold all three body lines into one logical flow with visible newline glyphs and row-split tokens; the height calc already counted body.lines(), proving the multi-line intent"
  - "wrapped_row_count targets the Ratio(1,2) inner width (frame.area().width / 2 − 2); estimating at a narrower-than-actual width is an over-estimate = safe side; the 80x24 buffer test is the arbiter if ratatui's WordWrapper ever disagrees"
  - "Greedy token-packing matches ratatui's WordWrapper exactly for this body (whole words move rows; mid-word split only for overlong tokens) — verified by hand-tracing reflow.rs and by the green test"

patterns-established:
  - "Modal-interior test scoping: anchor the modal box via its title row (┌<title>), match the bottom border by char column, slice interior columns — and convert str::find byte offsets to char offsets before chars().nth() scans (box-drawing chars are 3 bytes in UTF-8)"

# Metrics
duration: 24min
completed: 2026-09-11
---

# Phase 10 Plan 07: Confirm-Modal Blast-Radius Clip Gap Closure Summary

**TUI Confirm modal now word-wraps the blast-radius preview (`.wrap(Wrap { trim: false })` + split multi-line body) with wrapped-row-aware height, pinning UAT test 10 shut via a 80x24 buffer-level regression test**

## Performance

- **Duration:** 24 min
- **Started:** 2026-09-11T11:22:48Z
- **Completed:** 2026-09-11T11:47:24Z
- **Tasks:** 2 (TDD-shaped: RED test → GREEN fix)
- **Files modified:** 1

## Accomplishments
- The Confirm modal renders the complete blast-radius body at the UAT's 80x24 frame: the ~140-char preview line folds within the 38-column inner width, `agents:` / `pending executions:` tails are readable, and the `y to confirm · Esc to cancel` footer stays inside the bordered box
- The clip is permanently regression-pinned: `confirm_modal_wraps_the_blast_radius_body` asserts every distinctive blast-radius token is present in the modal interior buffer
- Modal height is wrapped-row-aware (greedy `wrapped_row_count` estimator at the Ratio(1,2) inner width), preserving the +4 chrome contract and the content-driven-height doctrine

## Task Commits

Each task was committed atomically:

1. **Task 1: Regression test — the clipped blast-radius preview, pinned in RED** - `25f9a65` (test)
2. **Task 2: Wrap the Confirm body + wrapped-row-aware height (GREEN)** - `7df4780` (fix)

## Files Created/Modified
- `crates/ignition-tui/src/ui/mod.rs` — Confirm render arm (split body into Lines + `Wrap { trim: false }`), Confirm height arm (`wrapped_row_count` at inner width), new `wrapped_row_count` helper, `Wrap` import, and the RED regression test

## Decisions Made
- **Split the Confirm body into real `Line`s in addition to `.wrap()`** — tracing ratatui 0.30's `reflow.rs` showed embedded `\n` inside a single `Line` is treated as whitespace (a literal glyph cell), not a row break; `.wrap()` alone would render the three body lines as one logical flow with visible newline glyphs and could row-split the `agents:` token pair. The pre-existing height calc already counted `body.lines()`, confirming the body was always meant to be multi-line. Splitting lives entirely inside the Confirm arm (scope guard held).
- **Estimator targets the Ratio(1,2) inner width with over-estimate-as-safe** — `frame.area().width / 2 − 2`; the plan's directive that over-estimation is safe and the buffer test is the arbiter was encoded in the doc comment.
- **Estimator verified against ratatui's WordWrapper by source trace** — words move whole to the next row (no mid-word split for tokens ≤ width), so greedy token packing matched exactly for this body at width 38 (estimator 8 body rows = ratatui's render).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Byte-index vs char-index mismatch in the test's modal-interior extraction**
- **Found during:** Task 1 (harness debugging)
- **Issue:** `str::find("┌suspend eam task")` returns a BYTE offset (box-drawing chars are 3 bytes in UTF-8 → byte 22 = char 20), so `chars().nth(at)` bottom-border scan looked at the wrong column and the harness panicked before reaching the intended assertion
- **Fix:** convert the byte offset to a char offset (`row[..byte_idx].chars().count()`) before the buffer-column scans
- **Files modified:** crates/ignition-tui/src/ui/mod.rs (test only)
- **Verification:** RED test then fails on the intended token assertion (`blast-radius token "dispatches" must be readable`), with the UAT clip `│suspends task ign-uat-scratch (eam_bac│` visible in the failure dump
- **Committed in:** 25f9a65 (Task 1 commit)

**2. [Rule 1 - Bug] Confirm body needed line-splitting, not only `.wrap()`**
- **Found during:** Task 2 (GREEN implementation)
- **Issue:** `Line::from(body)` carries embedded `\n` as whitespace graphemes (ratatui renders literal newline glyphs; the whole body becomes one logical wrapping flow whose rows can split the `agents:` / `_controller` token pair and show stray `\n` cells)
- **Fix:** Confirm arm maps `body.lines()` to real `Line`s before the `Wrap { trim: false }` paragraph — the multi-line rendering the height calc always assumed
- **Files modified:** crates/ignition-tui/src/ui/mod.rs (Confirm render arm only)
- **Verification:** regression test green at 80x24; estimator row count (8 body rows) matches the actual render; full suite + clippy clean
- **Committed in:** 7df4780 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 × Rule 1 bug)
**Impact on plan:** Both fixes were required to make the pinned assertion meaningful and the render genuinely readable; no scope creep — all changes inside the Confirm arm + its test.

## Issues Encountered
None beyond the two documented deviations (both diagnosed via buffer dumps and ratatui source inspection).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- UAT test 10 gap 1 is closed at the deterministic buffer level; visual re-confirmation rides the next verify-work/UAT pass per the plan (the UAT rig was torn down)
- Only remaining Phase 10 item: SC-5 live gate still blocked-on-env (user-provisioned WHK controller env — unchanged by this plan, see 10-USER-SETUP.md)
- Phase 12 TUIX-03/04 can build on the `wrapped_row_count` estimator if the tab-indicator/theming pass touches modal geometry

---
*Phase: 10-eam-write-operations*
*Completed: 2026-09-11*

## Self-Check: PASSED

- SUMMARY.md exists at `.planning/phases/10-eam-write-operations/10-07-SUMMARY.md`
- `crates/ignition-tui/src/ui/mod.rs` modified and committed
- Commit `25f9a65` (test RED) verified in git log
- Commit `7df4780` (fix GREEN) verified in git log
- Working tree clean for `crates/ignition-tui/` (remaining modified files belong to the parallel 10-06 executor — untouched here)
