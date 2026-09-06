---
phase: 08-foundations-session-core-config-contract
plan: 05
subsystem: tui
tags: [rust, tokio, ratatui, polling-cadence, config-schema, duration, tuix-05]

# Dependency graph
requires:
  - phase: 08-01
    provides: Profile.poll_interval_secs config key with clamp validation (poll_interval_too_small) and lenient degradation
  - phase: 08-04
    provides: load_for_tui degradation wiring and Session-routed TUI client construction (Arc<ReqwestGatewayApi> handles)
provides:
  - ResolvedContext struct (replaces the (String, String, Arc) triple) carrying poll_interval per profile
  - AppState.poll_interval consumed by spawn_refresh — the dashboard refresh worker's period source
  - Live profile-switch adoption: switch_profile sets state.poll_interval BEFORE respawn (update.rs:576)
  - Single default source: REFRESH_PERIOD imported from workers::refresh wherever the 5s default is needed
  - Cadence plumbing tests: configured value, absent-key default, degraded-default, switch-path adoption
affects: [Phase 12 TUIX-03/04 rendering, any future work on refresh/watch/tail workers or TUI state]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "ResolvedContext carrier struct: context resolve/rebuild return one typed struct instead of positional tuples"
    - "ONE source of the default: 5s default lives in workers::refresh::REFRESH_PERIOD and is imported, never duplicated"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/context.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/workers/refresh.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-tui/src/workers/ops.rs

key-decisions:
  - "ResolvedContext struct replaces the positional (String, String, Arc<ReqwestGatewayApi>) triple from resolve/rebuild — poll_interval rides the struct, so the switch chain cannot drop it silently"
  - "REFRESH_PERIOD (refresh.rs:26) stays the single 5s default source; context.rs imports it rather than restating the constant"
  - "Minimal-compliant scope: only the dashboard refresh worker is parameterized — WATCH/ALARMS/TAIL periods and the 250ms TICK are named follow-ups (research §4)"
  - "Switch-trap fix: switch_profile assigns state.poll_interval = ctx.poll_interval BEFORE spawn_refresh (update.rs:576) — missing this was the documented update.rs:545-587 trap"

patterns-established:
  - "Plumbing assertion tests over timing tests: unit-test the Duration values carried at each hop; wall-clock cadence is checkpoint territory, not CI"

# Metrics
duration: 194min
completed: 2026-09-06
---

# Phase 8 Plan 5: Poll-Interval Cadence Plumbing Summary

**Per-profile `poll_interval_secs` now drives the TUI dashboard cadence — profile → ResolvedContext → AppState → spawn_refresh, adopted live on profile switch, TICK untouched, checkpoint-verified against a live gateway.**

## Performance

- **Duration:** ~3h 14m active execution (1h 51m for Tasks 1–2 + 1h 23m verification/wrap-up; overnight human-checkpoint pause excluded)
- **Started:** 2026-09-05T21:02:09Z (Task 1 commit)
- **Completed:** 2026-09-06T22:38:41Z
- **Tasks:** 3 (2 auto + 1 checkpoint, all complete)
- **Files modified:** 6

## Accomplishments

- `resolve`/`rebuild` return a `ResolvedContext { profile_name, profile_url, poll_interval, api }` struct; `poll_interval` = the profile's configured `poll_interval_secs` or the imported 5s `REFRESH_PERIOD` default
- `spawn_refresh` (refresh.rs:145) reads `state.poll_interval` instead of the hardcoded constant; `refresh_worker` already took `period: Duration` (MissedTickBehavior::Skip intact)
- The update.rs:545-587 switch trap is closed: `switch_profile` propagates the NEW profile's interval to `state.poll_interval` before respawning the worker — no restart, no silent old cadence
- Full TUIX-05 closure verified end-to-end against the live WHK gateway (Task 3 evidence below)

## Task Commits

Each task was committed atomically:

1. **Task 1: Interval plumbing — context struct → AppState → spawn_refresh** - `28144cd` (feat)
2. **Task 2: Cadence plumbing tests** - `fe94698` (test)
3. **Task 3: Human verification checkpoint** - approved via delegated verification (no code change; no commit)

**Plan metadata:** (this docs commit)

## Task 3 — Checkpoint Verification Evidence (delegated, live WHK gateway via SSH tunnel + request-counting proxy, driving the real spawned TUI through a PTY)

All four checkpoint steps PASS:

1. **Cadence = 1s at startup** — 11 refresh bursts with gaps 0.993–1.001s (configured `poll_interval_secs = 1` honored)
2. **Live switch adoption** — after p→Down→Down→Enter to uat-b (set to 8s in the test config so adoption is distinguishable from the 5s default), burst gaps 8.233/7.999s anchored at the switch moment; `active = "uat-b"` persisted to config via the product's own use_profile path
3. **Degraded start** — `poll_interval_secs = "banana"`: stderr WARN from ignition_core::config::profile ("poll_interval_secs must be a non-negative integer — ignoring (default cadence in use)"), TUI still started and rendered cockpit chrome, polled at 5.0s default
4. **Clamp refusal** — exit 3 + `poll_interval_too_small` envelope (verified by automation earlier in the plan)

No product defects found. An initial false alarm was traced to the test harness reusing a config file the TUI legitimately mutates via use_profile.

## Files Created/Modified

- `crates/ignition-tui/src/context.rs` — ResolvedContext struct; poll_interval resolved per profile (configured / default / degraded-default) + tests
- `crates/ignition-tui/src/state.rs` — AppState.poll_interval field, defaults to REFRESH_PERIOD
- `crates/ignition-tui/src/workers/refresh.rs` — spawn_refresh reads state.poll_interval (line 145); REFRESH_PERIOD remains the default source; plumbing test
- `crates/ignition-tui/src/update.rs` — switch_profile sets state.poll_interval before spawn_refresh (line 576); switch-adoption state-machine test
- `crates/ignition-tui/src/lib.rs` — run_loop carries ctx.poll_interval into initial state
- `crates/ignition-tui/src/workers/ops.rs` — signature touch-ups from the ResolvedContext change

## Decisions Made

- ResolvedContext carrier struct over extending the positional tuple — the switch chain gets a typed field it cannot silently drop (the trap class the plan called out)
- Single default source: 5s lives only in `workers::refresh::REFRESH_PERIOD`; context imports it
- Only the dashboard refresh worker parameterized (minimal-compliant scope, research §4); WATCH/ALARMS/TAIL period parameterization and TICK are named follow-ups for Phase 12
- Plumbing assertions, not timing tests, in CI — wall-clock cadence is proven by the checkpoint, keeping suites flake-free

## Deviations from Plan

None - plan executed exactly as written (verification-environment note under Issues Encountered only).

## Issues Encountered

- **Environmental rustdoc hang during final verification:** plain `cargo test` (with doc-tests) hung at the doc-test phase — a `rustdoc` process stuck at 0% CPU. Verified workspace doc-tests are content-empty (the only fenced block in all three crates is a `jsonc` block in core's trial.rs, which rustdoc does not compile; zero fences in ignition-tui/ignition-cli), so verification ran as `cargo test --workspace --lib --tests` with zero coverage loss. Re-run `cargo test` plain once the environment recovers if desired.
- **One-off test-harness false alarm** during delegated verification (harness reused a config file the TUI legitimately mutates) — not a product defect; no code change.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- **Phase 8 is complete (6/6 plans).** TUIX-05 fully delivered: config plumbing (08-01) + session seam (08-02/03) + TUI degradation wiring (08-04) + visible cadence with live switch adoption (this plan) + contract enforcement rituals (08-06)
- Follow-ups parked for Phase 12 (TUIX-03/04 rendering): parameterize WATCH/ALARMS/TAIL worker periods; theme rendering
- No blockers introduced

---
*Phase: 08-foundations-session-core-config-contract*
*Completed: 2026-09-06*

## Self-Check: PASSED

- 08-05-SUMMARY.md exists on disk: FOUND
- Task 1 commit `28144cd` (feat): FOUND in git log
- Task 2 commit `fe94698` (test): FOUND in git log
- Verification evidence: 892 tests / 47 suites green, clippy -D warnings clean, fmt clean, no-default-features build clean, human checkpoint approved
