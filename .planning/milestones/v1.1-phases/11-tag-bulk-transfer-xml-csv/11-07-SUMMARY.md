---
phase: 11-tag-bulk-transfer-xml-csv
plan: 07
subsystem: cli
tags: [error-handling, hints, contract-tests, sentinel-pattern, rust, uat-gap-closure]

# Dependency graph
requires:
  - phase: 11-tag-bulk-transfer-xml-csv
    provides: TAGS-12 loss gate (main.rs loss_gate / render_loss_prose), CoreError::hint() central attachment + InvalidInput arm, 06-07 TUI_TTY_REFUSAL_REASON sentinel precedent, 11-05 contract_tags loss-gate refusal tests
provides:
  - LOSS_GATE_REFUSAL_REASON_PREFIX sentinel const (content-addressed hint override, public core API)
  - Loss-gate InvalidInput hint = --yes re-run guidance on human stderr AND JSON envelope error.hint
  - Drift-guard contract pins failing if the prose header and the sentinel ever diverge
affects: [tags-import, error-hints, envelope-consumers, any-future InvalidInput contextual-hint work]

# Tech tracking
tech-stack:
  added: []
  patterns: [content-addressed hint override via reason-prefix sentinel (dynamic-prose variant of the 06-07 exact-match sentinel)]

key-files:
  created: []
  modified:
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/tests/contract_tags.rs

key-decisions:
  - "Prefix sentinel over dedicated constructor (debug-session sanction): LOSS_GATE_REFUSAL_REASON_PREFIX = the render_loss_prose header literal, content-addressed in hint()'s InvalidInput arm — extends the 06-07 TTY pattern at one removal with zero new error variants or API churn"
  - "Frozen contract untouched: same slug invalid_input, same exit 2, same envelope shape, message prose byte-identical — only the hint VALUE differs on the loss-gate path (hints are not slugs; not a Three-Place event)"
  - "Task-1 unit test builds its loss-gate reason via format! from the CONST — no sentinel string literal in the test module, keeping the 2-hit production-literal uniqueness gate exact"
  - "No touch to the ~66 other InvalidInput sites sharing the generic hint — their hint drift is the debug session's deferred design question, out of this gap's scope"

patterns-established:
  - "Dynamic-prose sentinel: when a refusal reason is generated prose, content-address the hint off a stable PREFIX const instead of exact-match; pair with contract pins at the render site as the drift guard"

# Metrics
duration: 12 min
completed: 2026-09-14
---

# Phase 11 Plan 07: Loss-Gate Hint Mismatch Gap Closure Summary

**Loss-gate refusal now carries the --yes re-run hint on stderr and in the JSON envelope via a prefix sentinel in `CoreError::hint()`, drift-guarded by contract pins — generic file-read and TTY hints byte-identical everywhere else**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-14T14:27:43Z
- **Completed:** 2026-09-14T14:40:41Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- UAT test 3's defect closed: the loss-gate refusal's trailing hint matches the failure ("the loss report above names what this import would drop or coerce — re-run with --yes to import anyway") on both the human stderr line and the agent-facing `error.hint` envelope field
- `LOSS_GATE_REFUSAL_REASON_PREFIX` const + one `starts_with` branch in `hint()`'s InvalidInput arm — the 06-07 TTY-refusal override pattern adapted for dynamic prose, zero new variants/slugs
- Drift-guard coupling: main.rs throw-site comment + contract pins that fail if `render_loss_prose`'s header ever drifts off the sentinel (the hint would silently regress to the generic default)

## Task Commits

Each task was committed atomically:

1. **Task 1: LOSS_GATE_REFUSAL_REASON_PREFIX const + hint branch in core** - `459307d` (feat)
2. **Task 2: Throw-site coupling comment + drift-guard contract pins** - `cbfc022` (test)

**Plan metadata:** (docs commit follows this summary)

## Files Created/Modified
- `crates/ignition-core/src/error.rs` - Sentinel const next to TUI_TTY_REFUSAL_REASON; prefix branch in hint()'s InvalidInput arm between the TTY check and the generic default; 3 unit pins (loss-gate override + generic regression + TTY precedent intact)
- `crates/ignition-cli/src/main.rs` - loss_gate throw-site comment citing the sentinel coupling (prose header = the sentinel; 4 lines, construction untouched)
- `crates/ignition-cli/tests/contract_tags.rs` - Human-mode pin (`hint: the loss report above names` present, `fix the input source` absent on stderr) + JSON-mode pin (`error.hint` carries the corrected guidance); comments cite the debug session doc

## Decisions Made
- Implemented the debug session's sanctioned prefix-sentinel variant instead of UAT test 3's suggested dedicated constructor — equivalent outcome (correct hint, human + JSON), no new API surface, extends the existing TTY precedent (traceability note in plan context honored)
- Unit tests assert the hint WITHOUT literal-matching the generic default string, mirroring the existing TTY-pin style (`!hint.contains(...)` negative assertions + positive content checks)
- Task-1 test reason built via `format!("{}xml): …", LOSS_GATE_REFUSAL_REASON_PREFIX)` from the const — a literal in the test module would be a third sentinel-text hit and break the production-literal uniqueness gate

## Deviations from Plan

None - plan executed exactly as written. (One verification-count clarification, not a code deviation: the plan's verify step expects 7 hits of `fix the input source` in `crates/`, but Task 1's own action spec REQUIRES a negative `!hint.contains("fix the input source")` assertion in error.rs's test module — that mandated 8th hit exists at error.rs:1745. Actual ledger: 5 pre-existing hits intact + 2 Task-2 pins + 1 Task-1 pin = 8, all new hits are required `!contains` negative assertions; nothing removed.)

## Issues Encountered
- None. (Note: sibling plan 11-08's uncommitted doc/README changes were present in the working tree during execution; both task commits staged only this plan's three files, keeping 11-07 atomic. compile/test runs pick up only src, so no interference.)

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- UAT test 3 closed on evidence: 54 workspace test suites green (incl. the extended `tags_import_loss_gate_refusals` and the new core unit pin), `cargo clippy --workspace -- -D warnings` clean
- Frozen contract held: no golden edits, no README exit-table change, slug/exit/envelope byte-identical — only the hint value differs on the loss-gate path
- Remaining Phase-11 gap work: 11-08 (UDT-type fact scope correction) — already in progress in this tree per its own plan

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-14*

## Self-Check: PASSED
- Files verified on disk: error.rs, main.rs, contract_tags.rs, SUMMARY.md — all FOUND
- Commits verified in history: 459307d (Task 1), cbfc022 (Task 2) — both FOUND
- Workspace suite green (54 suites), clippy -D warnings clean at cbfc022
