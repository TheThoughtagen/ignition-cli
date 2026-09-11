---
phase: 10-eam-write-operations
plan: 06
subsystem: testing
tags: [live-gate, eam, wiremock, tokio, spawn_blocking, uat-gap-closure, docker-rig]

# Dependency graph
requires:
  - phase: 10-eam-write-operations
    provides: "10-05 SC-5 live gate (create→flip→suspend→§2→resume→delete→not_found), 10-LIVE-CAPTURES wire truth, 10-RIG-NOTES rig recipe, the UAT-recorded disposable-rig substitution decision (10-UAT.md test 12)"
provides:
  - "§2 poll-until-vanish (10s interval, ~90s deadline, grace-row tolerant) replacing the false-failing single-shot absence assert"
  - "Runtime-safe ScratchTaskGuard::drop (Handle::try_current → spawn_blocking + fresh current_thread runtime) live-proven on the failure path"
  - "Non-ignored wiremock unwind-mechanism test proving cleanup requests land after a mid-runtime unwind drop"
  - "Grace-row wire truth firmly captured + vanish-latency record corrected (fresh-rig <48s upper bound vs long-lived-rig >90s ×2)"
  - "10-LIVE-GATE.md §5 carries two verbatim run records; §4 D1 drift + §6 verdict name the follow-up capture work"
affects: [sc-5-follow-up, eam-capture-sessions, phase-10-verification]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "spawn_blocking cleanup-on-unwind: Drop guards inside #[tokio::test] route async cleanup through the blocking pool (joined at runtime drop), never tokio::spawn (unpolled after unwind) and never block_on (nested-runtime panic)"
    - "deadline-bounded poll with diagnostic-carrying panic (grace-row seen + last observed state) replaces single-shot assertions on eventual-consistency wire reads"

key-files:
  created:
    - ".planning/phases/10-eam-write-operations/10-06-SUMMARY.md"
  modified:
    - "crates/ignition-core/tests/live_gateway.rs"
    - ".planning/phases/10-eam-write-operations/10-LIVE-CAPTURES.md"
    - ".planning/phases/10-eam-write-operations/10-LIVE-GATE.md"

key-decisions:
  - "10-06: gate code stays EXACTLY as-run after the failed runs — the §5 verbatim records must match the committed binary; the transient-wording correction lives in the docs (captures §2 + gate §4), and the follow-up gap plan amends wording with its own re-run — Evidence-provenance over cosmetic retrofits"
  - "10-06: the ONE honest retry was spent on the §2 failure (transient-shaped: no HTTP/wire error, timing-variance signature); NO deadline bump — re-sizing without understanding the scheduled/false reconcile mechanics is guesswork, forbidden by the plan's failing-step contract — Follow-up: dedicated vanish-behavior capture across fresh + long-lived rigs (gate §6)"
  - "10-06: reused the UAT rig ign-uat-836 + its surviving /tmp/ign-uat/tokens.env (chmod 600, outside repo) rather than re-provisioning; token shredded at teardown — RIG-NOTES hygiene held"

patterns-established:
  - "spawn_blocking Drop-guard cleanup for async resources owned by tokio tests"
  - "deadline-bounded polls with grace-row-tolerant diagnostics on eventual-consistency reads"

# Metrics
duration: 27 min
completed: 2026-09-11
---

# Phase 10 Plan 06: UAT Gap 2 Closure — Live Gate Lifecycle Summary

**§2 poll-until-vanish + runtime-safe Drop guard shipped and live-run twice on the disposable controller rig — through-suspend proven end-to-end, both fixes live-proven on the failure path, but the §2 vanish never landed within 90s (×2) so SC-5 stays open with the drift recorded and the follow-up capture work named.**

## Performance

- **Duration:** 27 min
- **Started:** 2026-09-11T11:22:47Z
- **Completed:** 2026-09-11T11:50:37Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments
- §2 absence check is now a deadline-bounded poll (10s interval, ~90s deadline) that tolerates the captured grace-period row (`taskState=Suspended` in `scheduled/false`) instead of false-failing on first read — the exact UAT-race gap closed in code.
- `ScratchTaskGuard::drop` works when dropped during unwind inside the test runtime (the "Cannot start a runtime from within a runtime" root cause): cleanup routes through `Handle::spawn_blocking` + a fresh current_thread runtime; a non-ignored wiremock test proves the find+delete requests land after a mid-runtime drop.
- The live gate ran TWICE on a real disposable controller rig (`ign-uat-836`, 8.3.6): create → Scheduled+cron flip → suspend 204 + `isSuspended=true` live-proven through the action layer both times, first-attempt suspends; the Drop guard deleted BOTH failed runs' scratch tasks (zero leftovers; the UAT run's stranded-task failure mode is gone).
- Wire truth extended honestly: the grace-row shape is firmly captured, and the vanish-latency record now states fresh-rig <48s (upper bound) vs long-lived-rig >90s ×2 — mechanics unresolved, follow-up named in 10-LIVE-GATE.md §4 D1/§6.
- Rig torn down clean: `docker rm -f -v`, token shredded, zero containers/volumes/scratch tasks/token material left.

## Task Commits

Each task was committed atomically:

1. **Task 1: §2 poll-until-vanish + grace-row tolerance, vocabulary record extended** - `cf33221` (fix)
2. **Task 2: ScratchTaskGuard::drop works inside the test runtime + wiremock unwind proof** - `9f99ed5` (fix)
3. **Task 3: Live gate re-run on a disposable controller rig + record the run** - `8f67ca7` (docs)

## Files Created/Modified
- `crates/ignition-core/tests/live_gateway.rs` — §2 poll loop, shared `scratch_cleanup_future`, spawn_blocking Drop path, non-ignored `guard_drop_during_unwind_inside_runtime_still_cleans_up` wiremock test
- `.planning/phases/10-eam-write-operations/10-LIVE-CAPTURES.md` — §2 grace-row record + 2026-09-11 vanish-latency update (fresh vs long-lived rigs)
- `.planning/phases/10-eam-write-operations/10-LIVE-GATE.md` — status `failed-with-findings`; §3 step 6 truthful; §4 D1 drift; §5 two verbatim run records + pre-flight/teardown; §6 verdict + follow-up

## Decisions Made
- **Gate code stays exactly as-run** after the failed runs — §5's verbatim records must match the committed binary; wording corrections live in the docs, and the follow-up plan amends wording with its own re-run.
- **No deadline bump, no third attempt** — the retry budget (one, transient-only) is spent; re-sizing the 90s deadline without capturing the reconcile mechanics would be guesswork, which the plan's failing-step contract forbids.
- **UAT rig + surviving token reused** (pre-flight verified: StatusPing RUNNING, token 200, controller mode active, zero pre-existing tasks); token material stayed outside the repo and was shredded at teardown.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused `UNWIND_SCRATCH_PATH` const (dead_code warning)**
- **Found during:** Task 3 (the warning surfaced in the live run's compile output)
- **Issue:** const defined but both path matchers embed the encoded path directly — `dead_code` warning on every compile
- **Fix:** removed the const (pure cleanup, zero behavioral impact; both live runs' recorded line numbers unaffected)
- **Files modified:** crates/ignition-core/tests/live_gateway.rs
- **Verification:** `cargo test` + `cargo clippy -p ignition-core` clean, no warnings
- **Committed in:** 8f67ca7 (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (1 bug/cleanliness)
**Impact on plan:** Cosmetic only — no scope creep, no behavioral change.

## Issues Encountered

**The live gate failed at its own §2 vanish poll in BOTH runs — SC-5 remains open.** This is the honest-stop branch the plan itself defines for a failing live step (Task 3 step 4: capture verbatim, fix nothing by guesswork, record drift in §4, one transient-only retry allowed). Facts: the grace row (`taskState=Suspended`) persisted through the full ~90s poll deadline twice on the long-lived rig (~22h uptime), contradicting the fresh-rig <48s upper bound the deadline was sized from; suspend/flip/create live-proven both times; resume/delete not reached by the gate (they remain live-proven via the UAT CLI tests on the same rig, per 10-UAT.md). Drift recorded as 10-LIVE-GATE.md §4 D1; follow-up gap work named in §6: a dedicated capture of `scheduled/false` post-suspend vanish behavior (distribution, or whether the row ever leaves long-lived rigs) before the deadline is re-sized or the check re-shaped, then a gate re-run for the SC-5 close. Rig access is no longer a blocker — it is a documented recipe.

## User Setup Required

None - no external service configuration required (rig recipe is documented in 10-RIG-NOTES.md + 10-LIVE-GATE.md §5).

## Next Phase Readiness
- The two code fixes are landed, unit-proven (wiremock unwind test), and live-proven on the failure path — the gate is structurally ready for the SC-5 close.
- The named follow-up (vanish-behavior capture → deadline re-size or check re-shape → gate re-run) is a small, well-scoped gap plan; 10-07 proceeds independently.
- STATE.md blockers/concerns updated: SC-5 blocker is no longer "user env" — it is the pending vanish-behavior capture work.

---
*Phase: 10-eam-write-operations*
*Completed: 2026-09-11*

## Self-Check: PASSED

- All 4 key files exist on disk (verified `[ -f ]`).
- All 3 task commits present in git log: cf33221, 9f99ed5, 8f67ca7.
- Test suite: 1 passed (unwind mechanism test), 13 ignored (gate compiles, still env-gated); clippy clean.
- Rig teardown verified: zero ign-uat containers/volumes, token env shredded, zero scratch tasks at delete time.
