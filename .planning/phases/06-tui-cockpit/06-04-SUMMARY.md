---
phase: 06-tui-cockpit
plan: 04
subsystem: ui
tags: [ratatui, tokio, tag-browser, live-watch, confirmation, route-registry]

# Dependency graph
requires:
  - phase: 06-tui-cockpit plan 02
    provides: one-shot workers, result/confirm modals, era-stamped events, and async panel states
  - phase: 06-tui-cockpit plan 03
    provides: screen-scoped polling worker lifecycle reused by tag watch
  - phase: 05-tags-webdev
    provides: provider REST actions plus WebDev-backed browse/read/write/config/export/import/UDT/history actions
provides:
  - Provider-first tag tree navigation with detail reads and honest Loading/Loaded/Error states
  - Two-second whole-set live watch table driven by actions::tags::tags_read
  - Tags action menu covering write, providers, config, export/import, UDT, and history
  - Confirm gates for provider delete, config delete, and import overwrite
  - Exact route-registry rows for every non-alarm tags leaf
  - Rich-form CLI synopsis escape hatch through the ? key
affects: [06-05-projects, 06-06-route-coverage, phase-06-verification]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Provider → tree → detail navigation: Enter descends and Esc ascends exactly one level"
    - "Whole watched-set polling: set changes stop and respawn an era/generation-gated 2s tags_read worker"
    - "Caller-owned safety: destructive TUI actions arm Confirm before invoking unguarded core actions"
    - "Rich modal escape hatch: ? replaces a compact Input form with its exact CLI synopsis"

key-files:
  created: []
  modified:
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/ui/tags.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/workers/watch.rs

key-decisions:
  - "The tag watch worker polls the complete watched path set in one tags_read call every two seconds and restarts under a new generation whenever membership changes."
  - "Provider delete, config delete, and import overwrite are the only Tags-screen Confirm-gated operations, matching main.rs require_confirmation call sites."
  - "Rich tags operations use chained common-field Input modals; ? opens the exact CLI synopsis instead of introducing a general form framework."
  - "TUI JSON-file inputs reject stdin (-) because crossterm owns raw terminal input; files are read asynchronously inside spawned workers."
  - "tags export -o - remains a CLI-only flag-value exception, so tags export maps to Screen(Tags) and receives no separate OutOfBand route row."

patterns-established:
  - "Tag navigation honesty: detail → tree → provider list, one Esc press per level."
  - "Prefilled Input values are accepted directly or explicitly edited; tests model replacement rather than appending to defaults."

# Metrics
duration: 12min
completed: 2026-08-27
---

# Phase 6 Plan 4: Tags Cockpit Summary

**Provider-first tag browsing with on-demand detail reads, generation-gated two-second live watch, and a complete confirm-safe Tags action surface**

## Performance

- **Duration:** 12 min continuation (prior executor time unavailable)
- **Started:** 2026-08-27T21:11:10Z
- **Completed:** 2026-08-27T21:22:54Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments
- Built provider → tree → detail navigation with full-path rows, on-demand value/quality/timestamp reads, and exact one-level Esc ascent.
- Added a two-second live-watch table that polls the entire watched set through the existing `tags_read` action, generation-gates stale events, and stops on screen exit.
- Exposed all remaining tags verbs through the actions menu and shared one-shot result modal, including JSON-scalar write guidance and compact rich-input flows.
- Mirrored the CLI safety set with Confirm gates for provider delete, config delete, and overwrite import; abort import remains unguarded.
- Registered every non-alarm tags leaf using exact clap spellings and documented why `tags export -o -` is not a separate registry leaf.
- Verified 645 workspace tests passing, clippy with `-D warnings`, rustfmt, and diff checks.

## Task Commits

Each task was committed atomically:

1. **Task 1: Tag browser — providers, tree browse, detail + read** - `250b8cb` (feat)
2. **Task 2: Live watch table** - `f6643b8` (feat)
3. **Task 3: Tags actions menu — write, providers, config, export/import, UDT** - `1542f10` (feat)
4. **Test stabilization (verification-gate fixes)** - `f1ff0b6` (fix)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-tui/src/ui/tags.rs` - provider table, indented tag tree, detail pane, and watched-value table.
- `crates/ignition-tui/src/workers/watch.rs` - provider/browse/read one-shots, two-second whole-set watch worker, and async JSON-file loading.
- `crates/ignition-tui/src/event.rs` - provider, browse, detail, and watch result events with stale-result metadata.
- `crates/ignition-tui/src/state.rs` - tag navigation/watch/form state, Tags actions modal, and guarded pending actions.
- `crates/ignition-tui/src/update.rs` - tag state machine, worker lifecycle, action routing, chained forms, CLI help pane, and confirmation handling.
- `crates/ignition-tui/src/ui/mod.rs` - Tags actions rendering and multiline Input hints that remain visible at the locked half-width geometry.
- `crates/ignition-tui/src/routes.rs` - exact non-alarm tags leaf mappings and stdout flag-value exception documentation.
- `crates/ignition-tui/src/lib.rs` - run-loop cleanup for the tag watch rail.

## Decisions Made
- Watch membership changes restart one whole-set `tags_read` worker rather than spawning one worker per tag; generation stamps reject late results from the retired set.
- Compact TUI forms cover common arguments while `?` shows the exact CLI form for advanced usage; this preserves route reachability without adding a form framework.
- File-backed config/import inputs use `tokio::fs` in the spawned worker and reject `-`, preventing raw-mode stdin from being stolen from crossterm.
- The route registry uses clap's singular `tags provider ...` spelling even though menu labels use the user-facing plural “providers.”

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Repaired inherited uncommitted Task 3 form and rendering failures**
- **Found during:** Task 3 continuation verification
- **Issue:** Test helpers double-borrowed mutable state and would not compile; prefilled form tests appended replacement values to defaults; the JSON-scalar hint was clipped by the locked half-width modal; JSON file reads synchronously blocked the async worker.
- **Fix:** Corrected mutable reborrows, modeled direct acceptance/replacement of prefilled fields, rendered deliberate multiline hints with dynamic modal height, and switched file loading to `tokio::fs::read`.
- **Files modified:** `crates/ignition-tui/src/update.rs`, `crates/ignition-tui/src/ui/mod.rs`, `crates/ignition-tui/src/workers/watch.rs`
- **Verification:** `cargo test -p ignition-tui`, workspace tests, clippy, rustfmt, and `git diff --check` all pass.
- **Committed in:** `1542f10`

  Deviation 1's impact: the repairs made the inherited Task 3 implementation compile, model prefilled fields honestly, keep critical guidance visible, and avoid blocking the Tokio runtime — no feature scope added beyond the planned CLI-form escape hatch.

**2. [Rule 1 - Bug] Stabilized the flaky logs-tail deadline test**
- **Found during:** Plan-level verification (continuation executor's full workspace run)
- **Issue:** `tail_streams_pages_in_order_and_ends_cleanly_on_deadline` gave the tail a 40ms total deadline; under full-workspace parallel load, scheduler jitter starved the second in-memory page out of the budget (only 2 of 4 entries). Passed 3/3 in isolation — load-induced flake in a Phase 02-04 test, in a crate no 06-04 commit touches.
- **Fix:** Widened the test's deadline budget 40ms → 400ms (~10× headroom over the observed ~35ms starvation; isolated runtime still < 0.5s; poll-count assertions are `>=` so semantics unchanged).
- **Files modified:** `crates/ignition-core/src/actions/logs.rs`
- **Verification:** Test green in isolation; full workspace suite 645 passed / 0 failed.
- **Committed in:** `f1ff0b6`

**3. [Rule 1 - Bug] Guarded the compose streaming test against a stopped Docker daemon**
- **Found during:** Plan-level verification (second full workspace run)
- **Issue:** `run_streaming_forwards_lines_via_piped_stdout` skipped on `docker compose version` (a CLIENT-side check), but its body's `compose logs` needs a live daemon for the absent-project→exit-0 premise. OrbStack auto-stopped between runs → guard passed → spurious exit-1 failure.
- **Fix:** Added a daemon-reachability probe (plain `docker version` via the existing `run_docker` seam, nonzero when unreachable) so a daemon-less machine skips as quietly as a docker-less one.
- **Files modified:** `crates/ignition-core/src/rig/compose.rs`
- **Verification:** Test green via the quiet-skip path with the daemon down; full workspace suite green.
- **Committed in:** `f1ff0b6`

---

**Total deviations:** 3 auto-fixed (3 bugs)
**Impact on plan:** Deviations 2–3 were pre-existing test-infrastructure flakes in ignition-core that blocked the plan's workspace-green verification gate; both fixes are test-only with zero production-code changes. No scope creep.

## Issues Encountered
- The first workspace verification exceeded the 120-second shell timeout while reaching ignored E2E suites. It was rerun with a 300-second timeout and completed successfully; no test hung or failed.
- Continuation executor's verification surfaced two environment-sensitive ignition-core test failures (one load-induced deadline flake, one stopped-Docker-daemon guard gap) — both auto-fixed as Deviations 2–3; the third full workspace run finished 645 passed / 0 failed with clippy and rustfmt clean.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 06-05 can reuse the action/result/Confirm and rich-form `?` patterns for Projects.
- 06-06 can enable the bidirectional clap-tree coverage test against the now-complete tags rows.
- Live interaction against a route-deployed gateway remains part of the phase-end human verification; all navigation, polling, stale-result, rendering, and guard state machines are headlessly covered.

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-27*

## Self-Check: PASSED

All 8 key files exist; all 3 task commits (`250b8cb`, `f6643b8`, `1542f10`) plus the verification fix commit (`f1ff0b6`) are present; `tag_watch_worker` calls `tags_read` over the watched set; exact one-level Esc navigation and complete non-alarm tags route coverage remain test-pinned.

Continuation executor re-verified independently: `cargo test --workspace` 645 passed / 0 failed; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean; must_have artifacts (ui/tags.rs 635 lines ≥ 80; watch.rs contains `tags_read`) and all three key_link grep patterns confirmed; exact clap leaf spellings cross-checked against cli.rs (singular `tags provider`, `tags udt types/def`, `tags history`, `tags config`, `tags export/import`); the `tags export -o -` stdout exception is documented in a routes.rs comment per 06-06's OutOfBand rule.
