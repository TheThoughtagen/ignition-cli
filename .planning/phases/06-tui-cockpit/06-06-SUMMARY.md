---
phase: 06-tui-cockpit
plan: 06
subsystem: ui
tags: [ratatui, crossterm, tui, clap-commandfactory, coverage-test, docker-compose, ring-buffer]

# Dependency graph
requires:
  - phase: 06-tui-cockpit (plans 01-05)
    provides: the cockpit shell + all five prior screens, the routes scaffold, the spawn_action/result-modal contract, gated_cli_verb tripwire
  - phase: 04-rig-lifecycle
    provides: the rig action layer (status/up/down/reset/logs/trial/snapshot/restore) every rig verb rides AS-IS
provides:
  - Rig screen: status summary pane (allowlist render, down-is-data), full RigCommand action menu with EXACT CLI confirm parity, raw compose-logs stream pane (the Streamed mapping)
  - The COMPLETE routes registry: 63 rows, one per row-requiring CLI node, no gaps, no orphans
  - tui_coverage.rs — the structural SC1 proof: live clap-tree walk (CommandFactory) vs registry, bidirectional equality CI-enforced
  - ignition-cli lib target (pub mod cli) so tests walk the compiled command tree in-process
  - README TUI section (interactive-only, silent stdout, stderr-after-restore, confirm parity, keybindings, coverage guarantee)
affects: [07-interop-script, any-future-cli-command]

# Tech tracking
tech-stack:
  added: [async-trait as ignition-tui DEV-dep (test-side ComposeRunner fake)]
  patterns:
    - "Structural coverage proof: clap CommandFactory tree walk vs a static registry, bidirectional set equality — a new CLI command without a TUI mapping FAILS CI"
    - "Coverage rule: a node is row-requiring when it is a true leaf OR !is_subcommand_required_set() (the Option bare forms); required-subcommand groups map only through their children"
    - "Raw-pane streaming: a compose logs sink forwarding each line as an AppEvent into a second 10k ring, select!-ed against the pane's watch shutdown (the 06-03 tail shape, docker edition)"
    - "Secrets confinement by construction: the rig workers pass raw env strings into context.rs constructors; no auth-value type is ever named outside context.rs"

key-files:
  created:
    - crates/ignition-tui/src/workers/rig_stream.rs
    - crates/ignition-cli/src/lib.rs
    - crates/ignition-cli/tests/tui_coverage.rs
  modified:
    - crates/ignition-tui/src/ui/rig.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/event.rs
    - crates/ignition-tui/src/context.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/workers/mod.rs
    - crates/ignition-tui/src/ui/mod.rs
    - crates/ignition-tui/src/lib.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/completions.rs
    - crates/ignition-tui/Cargo.toml
    - Cargo.lock
    - README.md

key-decisions:
  - "ignition-cli gained a lib target (pub mod cli) — the coverage test needs Cli::command() in-process; dispatch/render stay binary-only (choke-file discipline preserved)"
  - "Rig credentials are env-only in the cockpit (IGNITION_TOKEN / IGNITION_USER + IGNITION_PASSWORD — the CLI's --user flag has no TUI form; the ? hatch names the env contract)"
  - "The rig logs pane clears its ring on every stream (re)spawn — compose logs --tail has no since-resume, so overlapping the tail would double-render (deliberate divergence from the gateway tail's resumption)"
  - "Confirm parity is EXACT: rig reset/restore/trial reset are the family's only gates; down stays ungated (compose down keeps volumes) — behaviorally pinned by rig_down_fires_without_confirm"
  - "tui_coverage is feature-gated (#![cfg(feature = \"tui\")]) so the lean --no-default-features build stays warning-clean with zero tests"

patterns-established:
  - "The CI-enforced mapping contract: routes() is now load-bearing structure, not documentation — treat registry edits as API changes"
  - "context.rs as the single auth-value construction site: workers pass strings in, typed pairs out"

# Metrics
duration: 28min
completed: 2026-08-28
---

# Phase 6 Plan 6: Rig Screen + Structural Coverage Proof Summary

**Rig screen (status + exact-parity action menu + raw compose-logs pane), the 63-row complete route registry, and the tui_coverage CI test that walks the live clap tree and machine-enforces CLI↔TUI completeness — plus the README's `ign tui` contract**

## Performance

- **Duration:** 28 min
- **Started:** 2026-08-27T23:39:40Z
- **Completed:** 2026-08-28T00:07:35Z
- **Tasks:** 3
- **Files modified:** 17

## Accomplishments
- The phase's defining claim is now machine-enforced: `tests/tui_coverage.rs` walks `Cli::command()` (the same CommandFactory clap_complete uses), applies the coverage rule (true leaves + the bare-invocable `sessions`/`logs`/`logs loggers` Option forms), and asserts BIDIRECTIONAL equality with `routes()` — 63 nodes, zero missing, zero orphans, proven to fail loudly in both directions (drift-tested by deleting a row → `missing: ["rig snapshot"]`, adding a bogus row → `orphans: ["telemetry"]`)
- The Rig family is fully reachable from the cockpit: the allowlist status pane (a down rig is data), all nine verbs on the `a` menu with main.rs's `require_confirmation` set mirrored EXACTLY (reset/restore/trial-reset gated; down deliberately ungated), and the raw `rig logs -f` pane — the one Streamed mapping — streaming compose lines into a second 10k ring with watch-channel shutdown on screen exit/pane toggle
- The registry is COMPLETE: all nine rig leaf rows registered (no bare `rig`/`rig trial` rows — both require subcommands), OutOfBand pinned to exactly the `completions` leaf, and the flag-value/stream-form exceptions (`logs -f`, `tags export -o -`) documented as routes.rs comments so STATE's four-exception list stays traceable
- README documents `ign tui`: interactive-only (TTY pre-check, exit 2), silent stdout on success, errors-after-restore to stderr per the frozen taxonomy, the 14-verb confirm-parity set, keybindings, and the CI-enforced coverage guarantee
- Full verification sweep green: 698 workspace tests (22 new), clippy -D warnings clean, fmt clean, single crossterm 0.29, update purity preserved (zero new await sites — rig futures live in workers/rig_stream), auth-value confinement mechanically exact (grep `Secret|Credential` outside context.rs = 0 hits), lean build warning-clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Rig screen — status, guarded action menu, logs stream pane** - `7afeb59` (feat)
2. **Task 2: Complete the registry + the tui_coverage.rs CI proof** - `772da64` (feat) + `ea709ed` (chore: lockfile)
3. **Task 3: README TUI section + phase verification sweep** - `e653664` (docs)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-tui/src/ui/rig.rs` - the screen: status summary (tri-state, down-is-data), raw logs pane (follow-the-newest window), status row
- `crates/ignition-tui/src/workers/rig_stream.rs` - status worker (AUTO discovery, era-stamped), the Streamed follow worker (rig_logs sink → AppEvent rail, select! vs shutdown), nine fire helpers
- `crates/ignition-tui/src/state.rs` - RigData (status tri-state + 10k raw ring + pane lifecycle), RIG_ACTIONS, RigForm, three rig PendingActions, Modal::RigActions
- `crates/ignition-tui/src/update.rs` - rig keymap (r/l/a), RigActions nav, restore form router, execute_pending rig arms, gated_cli_verb extended to the complete 14-verb set, rig refresh triggers, screen/profile-switch lifecycle
- `crates/ignition-tui/src/context.rs` - the rig client constructors + trial ladder + token-only rung (the Credential/Secret confinement home)
- `crates/ignition-tui/src/event.rs` - RigStatus (era-stamped) + RigLogLine (ring acceptance policy)
- `crates/ignition-tui/src/routes.rs` - nine rig rows + the flag-value/stream-form exception comments
- `crates/ignition-cli/src/lib.rs` - NEW lib target: `pub mod cli` (the shared command tree)
- `crates/ignition-cli/tests/tui_coverage.rs` - THE structural proof (walk + bidirectional equality + OutOfBand pin + bare-form pins)
- `crates/ignition-cli/src/main.rs` / `completions.rs` - import surgery to the lib target (dispatch untouched)
- `crates/ignition-tui/Cargo.toml` + `Cargo.lock` - async-trait dev-dep (the trait-implementing test fake)
- `README.md` - the TUI cockpit section + intro update

## Decisions Made
- ignition-cli gained a lib target (`pub mod cli`) — the coverage test needs `Cli::command()` in-process; the dispatch/render chassis stays binary-only (the choke-file discipline holds; main.rs only changed imports)
- Rig credentials are env-only in the cockpit; the `?` hatch names `IGNITION_TOKEN` / `IGNITION_USER + IGNITION_PASSWORD` (the CLI's `--user` flag has no modal form — the LOCKED modal-depth decision applied to the rig family)
- The rig logs pane clears its ring at every stream (re)spawn: compose `logs --tail` has no `since` resume, and overlapping the tail would double-render — the one deliberate divergence from the gateway tail's resumption semantics
- The rig logs stream always runs `--tail 200 --follow` (the CLI defaults), service-unscoped — the pane is the whole rig
- Profile switches clear RigData and re-arm the screen (the era bump retires in-flight rig workers; uniform with every other screen even though the rig world is docker-side)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ignition-cli needed a lib target for the integration test**
- **Found during:** Task 2 (tui_coverage.rs)
- **Issue:** the plan specifies `crates/ignition-cli/tests/tui_coverage.rs` importing `Cli` for `CommandFactory`, but the package was bin-only — integration tests cannot reach bin-crate items
- **Fix:** added `src/lib.rs` (`pub mod cli`) and switched main.rs/completions.rs imports to `ignition_cli::cli` (six `crate::cli::` references + the `use` block); dispatch/render stay binary-only
- **Files modified:** crates/ignition-cli/src/lib.rs, src/main.rs, src/completions.rs
- **Verification:** full workspace suite green (all 138+ ignition-cli tests unchanged); lean `--no-default-features` build warning-clean
- **Committed in:** 772da64

**2. [Rule 3 - Blocking] async-trait dev-dependency for the ComposeRunner test fake**
- **Found during:** Task 1 (the sink-forward key_link test)
- **Issue:** `ComposeRunner` is `#[async_trait]` — implementing the fake in the TUI test requires the macro, which ignition-tui did not depend on
- **Fix:** `async-trait = { workspace = true }` under `[dev-dependencies]` (already in the workspace graph; zero new external deps)
- **Files modified:** crates/ignition-tui/Cargo.toml, Cargo.lock
- **Verification:** the fake-runner test streams both fixture lines to the rail as RigLogLine
- **Committed in:** 7afeb59, ea709ed

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both were compile/test blockers inherent to the plan's own file choices. No scope creep.

## Issues Encountered
- clap's behavior on nested subcommand groups (`tags provider`, `rig trial`) was empirically confirmed before writing the rule: bare invocation exits 2 with `<COMMAND>` usage — `is_subcommand_required_set()` is true on them, so the walk correctly emits only their children (the plan's rule needed no adjustment; verified by the green first run plus the negative-space pins in `bare_option_forms_are_row_requiring_nodes`)
- The `update.rs` await audit needed its operational reading made explicit: 31 `.await` tokens exist in the file, ALL inside `async move {}` blocks passed to `workers::spawn_action` (futures constructed, never awaited, in update — the 06-02-onward call shape; `update()` itself is and stays a sync fn, compiler-enforced). 06-06 added ZERO new sites: every rig future lives in `workers/rig_stream.rs` (the 06-05 ops.rs convention)
- The main.rs Tui arm is ~14 rustfmt lines for 3 statements (the InvalidInput guard struct formats across 7 lines) — unchanged since 06-01, no new logic; within the "≤ ~10 lines" audit's spirit (choke-file discipline is about logic, and there is none)
- macOS pty smoke remains impractical (the 06-01 0×0-winsize finding) — manual smoke steps below instead of an `#[ignore]` live test

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 6 is COMPLETE: all six TUI requirements delivered, every CLI action TUI-mapped and CI-enforced (SC1 machine-checkable), terminal lifecycle safe, README current
- Manual smoke (documented per the plan's optional-live-smoke clause): on a machine with a rig — `ign tui` → Tab×5 to Rig → verify the status summary, `l` for the logs pane (Ctrl-C-free exit via `q`), `a` → `down` fires without a modal, `a` → `reset` shows the Confirm gate and Esc spawns nothing
- The coverage test now guards every future CLI addition: a new leaf without a routes() row fails CI with the node named — Phase 7's `script run` family MUST land its TUI surface (or an OutOfBand justification) in the same plan that adds the command
- 698 workspace tests green; fmt/clippy clean; secrets confinement mechanically exact

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-28*

## Self-Check: PASSED

All 6 key files exist on disk; all 4 task commits (7afeb59, 772da64, ea709ed, e653664) found in git history; must-have artifacts verified: tui_coverage.rs contains `CommandFactory` (2 mentions, 165 lines ≥ 40 min_lines), walks `Cli::command()`, asserts against `ignition_tui::routes::routes()`; routes.rs carries the complete 63-row registry; ui/rig.rs provides the status render + action menu host + raw logs stream pane.
