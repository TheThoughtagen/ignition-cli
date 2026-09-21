---
phase: 06-tui-cockpit
plan: 07
subsystem: core
tags: [serde, f64, error-taxonomy, exit-codes, classify, wiremock, snapbox, tty]

# Dependency graph
requires:
  - phase: 06-tui-cockpit (UAT)
    provides: "06-UAT.md gap diagnosis with wire-verified root causes (exponent-form gauges, prune 409, TTY hint)"
provides:
  - "CurrentGauges decodes 8.3.3 exponent-form Java doubles (f64 wire typing, integer-shaped agent JSON)"
  - "Additive exit-6 slug session_not_prunable via classify()'s route-scoped 409 arm"
  - "Contextual interactive-terminal hint for the ign tui TTY refusal (content-addressed InvalidInput hint)"
affects: [06-tui-cockpit verification, any 8.3.3-gateway user of ign metrics / TUI dashboard / sessions terminate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "serialize_bytes_f64: whole f64 byte counts serialize as JSON integers (2^53 guard) so a wire-side f64 typing never changes agent-visible JSON shape"
    - "Route-scoped classify arm: match guard on the URL (singular /designer/{id} vs plural /designers) scopes a status mapping to one route without touching others"
    - "Content-addressed variant hint: pub REASON const + constructor pair pins one raise site's contextual hint without a taxonomy variant or instance field"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/metrics.rs
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/sessions_contract.rs
    - crates/ignition-core/tests/status_contract.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/cli_chassis.rs
    - crates/ignition-tui/src/ui/dashboard.rs
    - README.md

key-decisions:
  - "CurrentGauges heap/max are f64 on the wire (8.3.3 Java doubles in exponent form); serialize_bytes_f64 keeps whole byte counts as JSON integers so agent-visible --json output keeps the pre-f64 integer shape"
  - "session_not_prunable is an additive exit-6 slug wired through classify()'s ROUTE-SCOPED 409 arm — the singular prune path matches, the plural list path cannot; every other route's 409 keeps Internal"
  - "Perspective-terminate 404 ('No valid sessions found to close') stays generic not_found — the body shape is unverified on the wire (only the openapi declares the message); skip rationale commented at the classify arm"
  - "TTY hint is content-addressed: pub TUI_TTY_REFUSAL_REASON const + CoreError::tui_tty_refusal() constructor pair the reason with its terminal hint — no new variant, no hint field, slug/exit 2 frozen"

patterns-established:
  - "Sibling wire audit comment: when a gauge typing changes, audit sibling wire structs and document the conclusion in-model (threads stay longs, chart values already f64)"
  - "Off-route negative pin: every route-scoped classify arm ships a wiremock test proving OTHER routes keep the fallback behavior"

# Metrics
duration: 52min
completed: 2026-08-28
---

# Phase 6 Plan 07: Gap Closure Summary

**Three 06-UAT gaps closed with tests: f64 gauge decode for 8.3.3 exponent-form Java doubles, a route-scoped 409→exit-6 `session_not_prunable` slug with a close-the-Designer hint, and a contextual interactive-terminal hint for the `ign tui` pipe refusal.**

## Performance

- **Duration:** 52 min
- **Started:** 2026-08-28T10:29:39Z
- **Completed:** 2026-08-28T11:21:46Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments
- `ign metrics` and the TUI dashboard metrics panel now work against 8.3.3 (b2026012009) gateways — the heap gauge arrives as `2.85746728E8` and decodes; agent JSON output keeps the integer shape (`285746728`, never `285746728.0`)
- Designer prune of a live session exits 6 (`session_not_prunable`) with "close the Designer first — prune removes stale entries only" instead of exit-1 internal-error; route-scoped so no other route's 409 changed behavior
- `ign tui | cat` refusal hint now says "run `ign tui` in an interactive terminal…" instead of the meaningless `--file`/stdin resource-put hint — slug, exit code, and all resource-put goldens unchanged
- All 713 workspace tests green (was 698), fmt clean, clippy `-D warnings` clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix CurrentGauges decode for exponent-form Java doubles** - `baf653a` (feat) + `5b5ba34` (style: rustfmt the fixture test)
2. **Task 2: Map designer-prune 409 to an exit-6 target-state slug** - `b18c396` (feat)
3. **Task 3: Contextual hint for the ign tui TTY refusal** - `83ae8b4` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/metrics.rs` - heap/max gauges as f64 + `serialize_bytes_f64` (whole→integer JSON) + exponent/integer/decimal fixture via `from_str` + sibling wire audit comment
- `crates/ignition-core/src/client/classify.rs` - route-scoped 409 arm (`is_designer_prune_url`/`designer_prune_id` helpers) + perspective-404 skip rationale
- `crates/ignition-core/src/error.rs` - `SessionNotPrunable` variant (slug/exit/hint/endpoint) + `TUI_TTY_REFUSAL_REASON` const + `tui_tty_refusal()` constructor + content-addressed InvalidInput hint + enumerated/hint test extensions
- `crates/ignition-core/tests/sessions_contract.rs` - 409-empty-body prune wiremock proof + off-route 409 stays Internal
- `crates/ignition-core/tests/status_contract.rs` - gauges assertions updated to f64
- `crates/ignition-cli/src/main.rs` - TTY raise site switched to `CoreError::tui_tty_refusal()`
- `crates/ignition-cli/src/render.rs` - `human_bytes` call sites cast f64→i64 (output byte-identical)
- `crates/ignition-cli/tests/cli_chassis.rs` - snapbox golden: `ign tui --compact` under a pipe (exit 2, envelope, new hint, zero stdout)
- `crates/ignition-tui/src/ui/dashboard.rs` - `fmt_mib` takes f64 (callers cast; rendered output unchanged)
- `README.md` - exit table row 6 + sessions terminate row document `session_not_prunable`

## Decisions Made
- **f64 with integer serialization over a custom deserializer**: byte counts ≤ ~9e15 are exact in f64, so plain f64 typing + `serialize_bytes_f64` (whole → `serialize_i64`, 2^53 guard) fixes decode without changing one byte of agent-visible JSON
- **Route-scoped classify match guard over a blanket 409 arm**: `S::CONFLICT if is_designer_prune_url(url)` — only the singular prune path maps; the plural `/designers` list (one character away) provably keeps Internal via its own wiremock test
- **Content-addressed hint over a hint field**: adding `hint: Option<String>` to InvalidInput would touch every construction site; a pub const + constructor pair achieves the same with zero shape change and drift-proof pairing
- **Skip the perspective-terminate 404 distinction**: the empty/unverified 404 body gives classify nothing to distinguish on — generic `not_found` is the honest mapping until a wire capture proves a marker

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Continuation] Adopted interrupted prior session's Task 1 implementation**
- **Found during:** Task 1 start
- **Issue:** The working tree held a complete but uncommitted f64-gauges implementation (metrics.rs/render.rs/dashboard.rs/status_contract.rs) from an interrupted 06-07 executor session — file content matched Task 1's spec exactly (f64 wire+domain, `from_str` fixture, sibling audit, display consumers)
- **Fix:** Verified ownership (no overlap with 06-08/06-10/06-11 file sets), ran the full Task 1 verification green, and committed it as Task 1 rather than rewriting identical code
- **Files modified:** (all Task 1 files, as planned)
- **Verification:** `cargo test -p ignition-core metrics` + full ignition-cli + ignition-tui suites green
- **Committed in:** `baf653a`

**2. [Rule 3 - Blocking] snapbox trailing-newline normalization in the TTY golden**
- **Found during:** Task 3 (golden test)
- **Issue:** raw stderr slice kept eprintln's trailing newline; the `str!` golden is newline-normalized, failing equality on the newline alone
- **Fix:** `trim_end()` at the golden's call site (documented inline)
- **Files modified:** crates/ignition-cli/tests/cli_chassis.rs
- **Verification:** golden green; full CLI suite green
- **Committed in:** `83ae8b4`

---

**Total deviations:** 2 auto-fixed (2 blocking/continuation)
**Impact on plan:** No scope creep — first was adopting verified interrupted work matching the plan exactly, second a test-harness normalization detail.

## Issues Encountered
- **Concurrent sibling-plan execution in the shared tree:** during Task 3 verification, 06-08's uncommitted files (resources.rs, resources_contract.rs) appeared in the working tree mid-edit (including momentary fmt drift). Handled by staging only this plan's files per commit, deferring workspace-wide fmt until the drift cleared, and re-running the full gate (713 green / fmt clean / clippy clean) once stable. No cross-plan files were committed.
- **Human-mode error rendering is text, not JSON:** the first TTY golden attempt expected an envelope without `--json`/`--compact`; the refusal renders `error:`/`hint:` text in human mode (correct behavior) — the golden pins the `--compact` envelope, matching the sessions-golden precedent.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Plans 06-08 through 06-11 remain (resources root-member surgery, tags refresh keys — already partially committed by a sibling session, modal geometry, rig panel readability)
- This plan's gaps are closed and pinned: the four must_have truths each carry a test (exponent fixture, dashboard f64 consumers, 409 wiremock, TTY golden)
- All existing slugs, exit codes, envelope shapes, and goldens unchanged (additive only) — 713 tests green

## Self-Check: PASSED

- Commit `baf653a` (Task 1): FOUND
- Commit `5b5ba34` (style): FOUND
- Commit `b18c396` (Task 2): FOUND
- Commit `83ae8b4` (Task 3): FOUND
- All 10 modified files: FOUND on disk

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-28*
