---
phase: 09-agent-surface-api-diagnostics
plan: 07
subsystem: api
tags: [diagnostics, bundle-wait, error-taxonomy, exit-codes, three-place-rule, uat-gap-closure, wiremock]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics (09-01, 09-05)
    provides: GatewayClientError Three-Place pattern (09-01); BUNDLE_GENERATING_STATES/BUNDLE_CAPTURED_STATES vocabulary + wait/poll machinery + contract_diagnostics.rs shared home (09-05)
provides:
  - Invalid encoded as a captured TERMINAL steady state (BUNDLE_CAPTURED_STATES + BUNDLE_UNAVAILABLE_STATES + is_bundle_unavailable) with 2026-09-07 UAT provenance
  - CoreError::BundleNotAvailable exit-6 slug bundle_not_available landed Three-Place atomic (exit enum + literal (exit,slug) table + README exit-6 row)
  - Honest deadline: CoreError::Network.observation field — answered gateways never get called "unreachable"; no-observation wording preserved
  - bundle wait immediate-exit semantics on Invalid (2-probe WaitRig pin + 1-hit binary pin)
affects: [10-and-later-phases, mcp-transport, verify-work-uat-retest]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "thiserror named-arg lead-word switch (Display lead chosen by field state — observation Some ⇒ 'no terminal state', None ⇒ 'gateway unreachable')"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/diagnostics.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/poll.rs
    - crates/ignition-core/src/actions/diagnostics.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/idp.rs
    - crates/ignition-core/src/actions/version.rs
    - crates/ignition-cli/tests/contract_diagnostics.rs
    - README.md

key-decisions:
  - "Invalid is a captured TERMINAL steady state (not a transient failure) — UAT TTL-probe truth: Valid decays to Invalid within ~2 min unprompted and stays; only a fresh generate changes it"
  - "Network deadline observation rides a dedicated Option<String> field, not the url string — the Display lead switches on it, so an answered gateway is never called unreachable while transport-error and no-observation wordings stay byte-compatible"
  - "bundle_not_available landed Three-Place ATOMIC in the Task-2 commit (exit enum + both pinned tables + README exit-6 row) — the readme_exit_table_agreement pin compiles against the README, so the row could not wait for Task 3"

patterns-established:
  - "Probe-arm ordering contract: is_generating → is_bundle_unavailable → captured-Done → unknown-pending (unavailable MUST precede captured-Done since Invalid is in both sets)"
  - "Deadline-with-observation message shape: 'no terminal state at {subject} — timed out after {waited:?}; last observation: {obs}'"

# Metrics
duration: 167min
completed: 2026-09-08
---

# Phase 9 Plan 07: Bundle-Wait Invalid-State Honesty Summary

**`bundle wait` now exits immediately exit-6 `bundle_not_available` on the UAT-discovered `Invalid` terminal steady state, and the poll deadline carries its last observation on a dedicated `Network.observation` field so an answered gateway is never mislabeled "unreachable" — closed UAT test 8 (Gap 3) with Three-Place atomic slug landing.**

## Performance

- **Duration:** 167 min (bulk = two full-suite background waits on a heavily loaded box; hands-on edit time ~35 min)
- **Started:** 2026-09-08T02:44:10Z
- **Completed:** 2026-09-08T05:31:52Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments
- `Invalid` capture-encoded with live provenance (rig ign-p9-836, 2026-09-07): added to `BUNDLE_CAPTURED_STATES`, new `BUNDLE_UNAVAILABLE_STATES` + `is_bundle_unavailable()`; the falsified "exactly two states" module docs rewritten, fileSize non-static quirk recorded as observation-not-behavior
- `CoreError::BundleNotAvailable { state }` — exit 6, slug `bundle_not_available`, hint naming `ign diagnostics bundle generate`; landed ATOMICALLY across exit enum + literal (exit,slug) table + README exit-6 row (the `readme_exit_table_agreement` pin proved the atomicity requirement live — it went red exactly as designed until the README row landed)
- Honest deadline: `CoreError::Network` gained `observation: Option<String>`; `deadline_error()` populates it; Display leads "no terminal state" when an observation exists and preserves "gateway unreachable" only when the gateway said nothing; exit 4 / `network_error` unchanged
- `bundle wait` probe order: generating → **unavailable-refuses-immediately** → captured-Done → unknown-keeps-polling; WaitRig pins exactly-2-probes exit-6; binary contract pins exactly-1-hit exit-6; happy path (Generating→Valid exit 0) untouched and green

## Task Commits

Each task was committed atomically:

1. **Task 1: Encode Invalid as a captured TERMINAL steady state** - `f12de36` (feat)
2. **Task 2: bundle_not_available slug + immediate-exit wait + honest deadline** - `4df872d` (feat)
3. **Task 3: Binary contract test + README rows** - `1d9cc58` (test)

**Plan metadata:** _pending final docs commit_

_Note: Task 2's commit includes the README exit-6 row (Three-Place atomicity — see Deviations)._

## Files Created/Modified
- `crates/ignition-core/src/client/diagnostics.rs` — Invalid in BUNDLE_CAPTURED_STATES + BUNDLE_UNAVAILABLE_STATES + is_bundle_unavailable; module docs rewritten; 3 new/updated unit tests
- `crates/ignition-core/src/error.rs` — BundleNotAvailable variant (slug/exit-6/hint); Network.observation field + lead-word Display; both pinned tables + enumerated case; observation-aware Network hint
- `crates/ignition-core/src/poll.rs` — deadline_error populates observation; new no-observation deadline test; deadline test asserts "unreachable" ABSENT when observed
- `crates/ignition-core/src/actions/diagnostics.rs` — probe-arm order with immediate BundleNotAvailable abort; new WaitRig Invalid test; unknown-deadline test hardened
- `crates/ignition-core/src/client/mod.rs`, `client/idp.rs`, `actions/version.rs` — mechanical `observation: None` at 12 literal Network constructions
- `crates/ignition-cli/tests/contract_diagnostics.rs` — new `bundle_wait_invalid_exits_immediately`; deadline contract upgraded for the honest wording
- `README.md` — exit-6 row + bundle wait row rewritten + generate-row vocabulary claim corrected

## Decisions Made
- **Invalid = terminal steady state, not transient failure:** UAT TTL-probe evidence is unambiguous (Valid at min 4-5, Invalid at min 6, persists) — encode the observed truth, not the plan-era guess that failure states "remain unenumerated"
- **Observation as a dedicated field:** keeps transport-error and no-observation-deadline message bytes IDENTICAL to today (all pinned contract tests untouched), while the answered-deadline wording changes exactly where honesty demands
- **README exit-6 row pulled into Task 2:** the pinned `readme_exit_table_agreement` test `include_str!`s the README — Three-Place cannot be two commits; the command-reference rows stayed in Task 3 as planned

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] README exit-6 row landed in the Task 2 commit, not Task 3**
- **Found during:** Task 2 verification
- **Issue:** The plan places the README exit-table row in Task 3, but `readme_exit_table_agreement` (Phase 08 pin) compiles the README via `include_str!` — `cargo test -p ignition-core` could not be green with the slug in only two of three places, and Task 2's verify demands a green suite
- **Fix:** Added `bundle_not_available` to the README exit-6 row in the Task 2 commit (true Three-Place atomic landing); Task 3 kept the command-reference rows + contract tests
- **Files modified:** README.md
- **Verification:** full ignition-core suite green (361 lib + all integration), Three-Place test passes both directions
- **Committed in:** 4df872d (Task 2 commit)

**2. [Rule 1 - Bug] Corrected the falsified "EXACTLY two states" claim in the README generate row**
- **Found during:** Task 3 (README row rewrite)
- **Issue:** Row `ign diagnostics bundle generate` claimed the observed vocabulary is "EXACTLY `Generating` and `Valid`" — falsified by the 09-07 UAT the same way the wait row was; leaving it would make the README self-contradictory
- **Fix:** Minimal wording update naming `Invalid` as captured (terminal "no current bundle", see `wait`); envelope/keys/timeout claims untouched
- **Files modified:** README.md
- **Verification:** rg confirms both rows agree; no test pins the row text
- **Committed in:** 1d9cc58 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 missing-critical atomicity, 1 stale-doc bug)
**Impact on plan:** Both fixes serve the plan's own must-have ("Three-Place consistent, all agree") and README self-consistency. No scope creep.

## Issues Encountered
- Full-suite runs on this box are heavily contended (competing cargo builds + the parallel 09-08 wave): `cargo test -p ignition-core --tests` needed a background nohup + poll pattern (~25 min wall), and the workspace suite took ~65 min. All 54 workspace result blocks finished `ok` with zero failures — no code issue, purely load.
- The plan's construction-site estimate of "~15 literal Network constructions" was exact in spirit: 12 production/test literals + the two deadline paths (mod.rs ×7, idp.rs ×4, poll.rs, version.rs, error.rs helper).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- UAT test 8's three diagnosed misses each have a landed counterpart: vocabulary+provenance (`BUNDLE_UNAVAILABLE_STATES`), immediate exit (exit 6 `bundle_not_available`, ≤2 probes), deadline distinction (observation field + lead-word switch) — re-testable at `/gsd-verify-work` against either rig
- Happy path re-proven green (existing tests): Generating → Valid still exits 0 with the wire envelope; defaults 2s/300s and download rows untouched
- Live-rig re-test note: on a decaying rig, `ign diagnostics bundle wait` against `Invalid` now exits in seconds (one probe) instead of hanging 300 s — the exact UAT-reported failure is structurally impossible
- 09-08 (TUI menu parity) landed in parallel; both remaining Phase 9 gap plans complete → phase ready for re-verification

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-08*

## Self-Check: PASSED

- All 6 modified files + SUMMARY.md exist on disk ✓
- All 3 task commits found in history: f12de36, 4df872d, 1d9cc58 ✓
- Full workspace suite: 54/54 result blocks ok, zero FAILED ✓
- Three-Place rg checks: bundle_not_available in exit enum (error.rs:579), enumerated case (error.rs:1226), literal table (error.rs:1288), README exit-6 row, README wait row ✓
