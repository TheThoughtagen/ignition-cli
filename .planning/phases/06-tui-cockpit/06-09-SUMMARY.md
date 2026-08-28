---
phase: 06-tui-cockpit
plan: 09
subsystem: ui
tags: [ratatui, tui, tags, refresh, invalidation, actiondone-trigger]

# Dependency graph
requires:
  - phase: 06-tui-cockpit plan 04
    provides: Tags screen browse/detail/watch surface + one-shot spawn seams
  - phase: 06-tui-cockpit UAT
    provides: gap diagnosis (tests 10 [major] + 11 [minor], stale-402 + stale post-write value)
provides:
  - "'r' refresh key on the Tags screen re-firing the deepest visible one-shot (detail read > top stack browse > provider list)"
  - "Screen re-entry (Tab) and profile-switch re-entry invalidate in-stack Tags one-shot results via the same helper"
  - "ActionDone 'tags write' success trigger refreshing the open matching detail pane (write→read-back round-trip)"
  - "Recovery hint line (press r to refresh) on every Tags error pane (providers/browse/detail)"
affects: [06-10, 06-11, phase-06-verification]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "deepest-visible one-shot refire: ONE helper (refire_tags_current_level) serves 'r', set_screen re-entry, and profile-switch re-entry — one invalidation convention for all three entry points"
    - "ActionDone label-trigger refresh generalized: alarms-ack re-poll (06-03) now has a twin — tags-write success + path match refires the detail read; state carried via a consumed-on-landing armed field (last_write_path)"
key-files:
  created: []
  modified:
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/ui/tags.rs

key-decisions:
  - "Deepest-first refire order (detail > stack top > providers) with per-level error clearing as the refire arms — a stale 402 visibly reloads instead of lingering"
  - "last_write_path armed at the write-form accept site (ActionDone carries only the label); consumed on ANY landing; only SUCCESS + matching open-detail path refires; the watch table is left to its 2s poll (commented at the trigger)"
  - "workers/watch.rs untouched — the existing spawn seams (spawn_browse/spawn_detail_read/spawn_providers_once) were sufficient; no new worker code needed despite the plan listing the file"

patterns-established:
  - "Refresh-key pattern: a screen's 'r' re-fires its deepest visible one-shot and clears that level's error as it arms"
  - "Write→read-back pattern: mutating forms arm a target-path field at accept; the ActionDone landing compares + consumes it and refires the matching read"

# Metrics
duration: ~15min (6min code session — interrupted before summary — + 9min continuation verify/state)
completed: 2026-08-28
---

# Phase 6 Plan 9: Tags Freshness Gap Closure Summary

**'r' deepest-visible refire + write→detail ActionDone refresh + error-pane recovery hints on the Tags screen — the stale-402 and stale-post-write UAT gaps closed, 178 TUI tests green**

## Performance

- **Duration:** ~15 min (6 min code session interrupted before SUMMARY + 9 min continuation verification/state)
- **Started:** 2026-08-28T03:10:48Z (Task 1 commit)
- **Completed:** 2026-08-28T10:39:42Z (continuation session)
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- **Gap 06-UAT test 10 [major] closed:** `r` on the Tags screen re-fires the CURRENT level's read, deepest-first — open detail pane → detail read under a fresh seq; stacked browse level → that level's browse (entries drop to Loading); root → provider list (busy-guard respected). Every level's error field clears as the refire arms, so the honestly-earned stale 402 visibly reloads instead of persisting until Esc+Enter re-navigation.
- **Screen re-entry invalidates:** `set_screen(Tags)` and the profile-switch re-entry arm now route through the same helper — Tab away and back refires the deepest visible one-shot (the recovery path that needs no key discovery).
- **Gap 06-UAT test 11 [minor] closed:** a landed SUCCESSFUL `tags write` for the open detail's path refires the detail read under a fresh seq (the alarms-ack trigger pattern's twin) — the write→read-back round-trip is visible without manual refire. A different path, or a FAILED write, leaves the pane untouched (the old value is still the truth); the watch table refreshes naturally on its 2s poll.
- **Recovery hints:** every Tags error pane (providers/browse/detail) appends a DIM `press r to refresh` line — an honestly-earned stale error always names its one-key way out.

## Task Commits

Each task was committed atomically:

1. **Task 1: 'r' refresh on the Tags screen (current-level refire)** — `04baf41` (feat)
2. **Task 2: tags-write ActionDone refreshes the open detail** — `4c1dbac` (feat)
3. **Task 3: Recovery hints on Tags error panes** — `4711c43` (feat)

**Plan metadata:** (this commit — docs: complete plan)

## Files Created/Modified

- `crates/ignition-tui/src/update.rs` — `refire_tags_current_level` helper + `'r'` binding in tags_keys; set_screen/profile-switch re-entry routed through it; ActionDone `tags write` success trigger; 5 new tests (4 refresh + 1 write-trigger)
- `crates/ignition-tui/src/state.rs` — `TagsData.last_write_path` (the armed write target, consumed on landing)
- `crates/ignition-tui/src/ui/tags.rs` — `refresh_hint()` (DIM) on all three error renders + 1 new render test + hint assertions on the two existing error tests

## Decisions Made

- **Deepest-first refire with error-clear-on-arm** — refiring without clearing would leave the stale error visible until the fresh result landed; clearing makes the reload visible immediately.
- **`last_write_path` armed at the accept site** — ActionDone carries only the label, so the write form's accept arms the target path for the landing's comparison; consumed on any landing so a stale armed target can never refire a later, unrelated write's pane.
- **Watch table deliberately not nudged** — a watched path refreshes on the existing 2s poll; an extra refire would be redundant (noted in a comment at the trigger).
- **workers/watch.rs left untouched** — the plan listed it under files_modified, but the existing spawn seams were exactly the refire primitives needed; no new worker code required.

## Deviations from Plan

None - plan executed exactly as written. (The tasks were committed by an earlier executor session that was interrupted before SUMMARY creation; this continuation session verified every commit against the plan, re-ran all verification gates, and completed the summary/state/metadata steps. No code changes were needed.)

## Issues Encountered

- **Interrupted prior session:** all three task commits existed without a SUMMARY/STATE update. Resolved by full verification of each commit's diff against the plan's requirements, then completing the summary/state flow. No rework required.
- **Parallel-plan working-tree noise (not this plan's):** uncommitted changes for plan 06-08 (metrics exponent-form fix: `metrics.rs`, `dashboard.rs`, `render.rs`, `status_contract.rs`) sit in the working tree from a parallel executor. They were left untouched and unstaged. Workspace tests/clippy pass WITH those changes in tree; the single `cargo fmt --all --check` offender is their `metrics.rs` — this plan's three files verified fmt-clean directly (`rustfmt --edition 2024 --check` exit 0).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plans 06-10 and 06-11 (remaining gap-closure plans) can proceed independently — this plan touched only Tags-screen files.
- 06-UAT gap rows for tests 10 and 11 are now addressed at the code level; the UAT doc's gap triage should mark them closed at re-verification.
- All 178 ignition-tui tests green; workspace tests + clippy `-D warnings` clean; 06-09 files fmt-clean.

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-28*

## Self-Check: PASSED

All 4 key files exist on disk; all 3 task commits (04baf41, 4c1dbac, 4711c43) verified in git log.
