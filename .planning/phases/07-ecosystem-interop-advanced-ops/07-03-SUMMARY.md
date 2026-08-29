---
phase: 07-ecosystem-interop-advanced-ops
plan: "03"
subsystem: api
tags: [webdev, scriptexec, jython, wiremock, snapbox, clap, ratatui]

# Dependency graph
requires:
  - phase: 05-webdev-backend-tag-operations
    provides: the scriptExec route (05-01 template + secret lifecycle), webdev_route_call seam + 200-BODY envelope oracle (05-03), traceback surfacing pattern (05-08)
  - phase: 07-ecosystem-interop-advanced-ops (07-02)
    provides: the BackupType-param'd trait surface the test doubles must mirror; the 07-02 goldens' harness conventions
provides:
  - "ign script run (SCRPT-01): three input forms (--code/--file/--file -), probe+exec sequence over scriptExec, {stdout, result, elapsedMs} envelope"
  - "additive exit-6 slug script_exec_not_configured (two-place exit-table rule held)"
  - "actions::script::{script_run, read_script_input} + ScriptRunResult"
  - "TUI script run row + ACTIONS entry + code-only Input modal (ungated, CLI parity)"
affects: [07-04, verify-work, README exit-table consumers]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "structural opt-in verb: no --yes guard — the deploy flag IS the consent (secret presence gates, zero HTTP on refusal)"
    - "pub(crate) route constants (SCRIPT_EXEC_ROUTE/SECRET_HEADER) shared across action modules — one spelling, no drift"
    - "config loaded inside the TUI worker for secret-gated actions (the fire_webdev_status precedent extended)"

key-files:
  created:
    - crates/ignition-core/src/actions/script.rs
    - crates/ignition-core/tests/script_contract.rs
    - crates/ignition-cli/tests/contract_script.rs
  modified:
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/actions/webdev.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - crates/ignition-tui/src/workers/ops.rs
    - README.md

key-decisions:
  - "No clap conflicts_with on --code/--file: read_script_input owns the both-given refusal so the CLI surfaces the InvalidInput envelope (exit 2, profile null), not a clap usage render — the plan's both-inputs golden demanded the CoreError shape"
  - "Precondition = the version action WITH the secret header riding webdev_route_call's existing denial mapping (secret_mismatch → webdev_route_error family, no new slug); no version-compare magic — the tags precondition owns that discrimination"
  - "TUI script run is CODE-ONLY (single Input modal — no textarea shape exists in the cockpit; stdin/--file refused per the crossterm raw-input rule), firing UNGATED at accept (CLI parity)"
  - "Missing answer fields default (empty/null/0) with ALL keys still riding — the family convention"
  - "snapbox backslash-normalization gotcha recorded inline at the JSON golden (hello\\n goldens as hello/n — the 03-02 discipline applied to a new family)"

patterns-established:
  - "Structural-gate verb shape: config-store precondition → additive slug + zero-HTTP, before any client work"
  - "Worker-side config load for secret-dependent TUI actions (fire_script_run mirrors fire_webdev_status)"

# Metrics
duration: 54min
completed: 2026-08-29
---

# Phase 7 Plan 3: script run Summary

**`ign script run` over the already-secured scriptExec route — three input forms, secret-gated probe+exec sequence, {stdout, result, elapsedMs} envelope, one additive exit-6 slug, fresh TUI row, zero new dependencies**

## Performance

- **Duration:** 54 min
- **Started:** 2026-08-29T02:14:39Z
- **Completed:** 2026-08-29T03:09:32Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments
- `actions::script::script_run`: resolves `webdev_secret` FIRST (None → additive `script_exec_not_configured` exit 6 whose hint names `ign webdev deploy --with-script-exec` verbatim, ZERO HTTP), then the version-probe handshake, then the exec POST — both carrying `X-Ignition-CLI-Secret`; the answer maps under unit-explicit `{stdout, result, elapsedMs}` with ALL keys always; route error bodies surface their traceback (the 05-08 marker)
- `read_script_input`: the PURE three-form reader (`--code STR` / `--file PATH` / `--file -` stdin) — both/neither/unreadable → InvalidInput exit 2 (usage errors lead)
- CLI surface + goldens: `script run` leaf (no `--yes` by design), success JSON golden (`[..]` elision on elapsedMs), success human golden (stdout verbatim + result/elapsed lines), missing-secret refusal binary-pinned (exit 6, zero HTTP), both-inputs golden (exit 2, profile null), redaction canary on both streams
- TUI: the `script run` row lands FRESH (grep-verified none existed; tui_coverage clap-walk green in the SAME plan), ACTIONS 13→14, code-only Input modal firing unguarded through `fire_script_run` (worker-side config load), result in the shared pretty-JSON modal
- README: command-table row + the scriptExec posture section extended with the verb contract (structural opt-in, no-timeout honesty, redaction, shared-secret cross-ref)

## Task Commits

Each task was committed atomically:

1. **Task 1: script_run action — secret resolution + precondition + exec + contract** - `cc189f7` (feat)
2. **Task 2: CLI surface + goldens + TUI input/result modal + README** - `ddf8723` (feat)

**Plan metadata:** (recorded below)

## Files Created/Modified
- `crates/ignition-core/src/actions/script.rs` - NEW: script_run action + read_script_input + unit tests
- `crates/ignition-core/tests/script_contract.rs` - NEW: wiremock contract (probe/exec sequence, secret header on both POSTs, denials, traceback, redaction)
- `crates/ignition-cli/tests/contract_script.rs` - NEW: binary goldens (success JSON/human, refusals, canary)
- `crates/ignition-core/src/error.rs` - ScriptExecNotConfigured additive exit-6 slug + enumerated test row
- `crates/ignition-core/src/actions/webdev.rs` - SCRIPT_EXEC_ROUTE/SECRET_HEADER → pub(crate)
- `crates/ignition-core/src/actions/mod.rs` - script module registration
- `crates/ignition-cli/src/cli.rs` - Script(ScriptArgs) + ScriptCommand::Run
- `crates/ignition-cli/src/main.rs` - dispatch arm (input-first), ActionOutput::ScriptRun, render_json arm
- `crates/ignition-cli/src/render.rs` - human render (stdout block, result, elapsed)
- `crates/ignition-tui/src/routes.rs` - "script run" → Dashboard row
- `crates/ignition-tui/src/state.rs` - PendingInput::ScriptCode, ACTIONS 14
- `crates/ignition-tui/src/update.rs` - menu arm + accept arm
- `crates/ignition-tui/src/workers/ops.rs` - fire_script_run
- `README.md` - exit-table slug + command row + verb-contract posture section

## Decisions Made
- **No clap conflicts_with on --code/--file** — `read_script_input` owns the both-given refusal so the CLI surfaces the `invalid_input` envelope (exit 2, profile null) rather than clap's own usage render; the plan's "both-inputs InvalidInput golden" demanded the CoreError shape
- **Probe = version action through `webdev_route_call`'s existing denial mapping** — `secret_mismatch` surfaces as the honest `webdev_route_error` (deployed-elsewhere/stale advice already in the hint); no new slug, no version-compare magic
- **TUI is code-only** — the cockpit has no textarea shape, so a single Input modal; `--file`/stdin refused per the crossterm raw-input rule (documented in the modal hint); fires UNGATED (CLI parity)
- **Absent answer fields default** (empty string / null / 0) with all keys still riding — the family convention for agent stability

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Repaired never-run tests + clippy + fmt in adopted partial work**
- **Found during:** Task 1 (interrupted-run review)
- **Issue:** The prior executor's partial files had never been verified: two unit tests failed (serde_json `Value` maps are key-SORTED — the test asserted declaration order; the "neither input" message didn't contain the literal `--file -` the test expected), clippy flagged `redundant_guards` on `if file == "-"` and `type_complexity` on the rig helper, error.rs doc table carried broken indentation (rustfmt), and `script_run`'s result mapping had fmt drift
- **Fix:** pattern `(None, Some("-"))` for the stdin arm; `--file -` named verbatim in the neither-given reason; keys sorted before assert; `type CallLog` alias; doc-comment indentation restored; `cargo fmt --all` normalized
- **Files modified:** crates/ignition-core/src/actions/script.rs, crates/ignition-core/src/error.rs
- **Verification:** cargo test -p ignition-core --lib (288 green) + --test script_contract (5 green) + clippy --all-targets -D warnings + fmt --check
- **Committed in:** cc189f7 (Task 1 commit)

**2. [Rule 1 - Bug] TUI menu-motion test pinned the old ACTIONS length**
- **Found during:** Task 2 (workspace test run)
- **Issue:** `menu_modals_take_vim_motions` asserted G lands on index 12 (the pre-07-03 last entry); ACTIONS grew 13→14 with "script run"
- **Fix:** re-pinned to index 13 with the comment updated (07-02 families + 07-03's script verb)
- **Files modified:** crates/ignition-tui/src/update.rs
- **Verification:** cargo test --workspace — 808 passed, 0 failed
- **Committed in:** ddf8723 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 bug-class repairs to adopted/generated code)
**Impact on plan:** No scope creep — both were correctness repairs surfaced by the plan's own verification commands. The interrupted-run adoption itself followed the continuation protocol (partial work reviewed against the plan, verified via the test suite, then committed atomically per task).

## Issues Encountered
- The interrupted run's partial work was structurally sound (action shape, contract coverage, and slug matched the plan) but had never executed its own tests — treating the test suite as the source of truth caught five concrete defects before any commit (documented as Deviation 1)
- snapbox normalizes backslashes in actual output (the known 03-02 gotcha): the JSON golden's `hello\n` escape had to be written `hello/n`; recorded inline at the golden

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 07-03 (SCRPT-01) complete; the tui_coverage clap-walk gate is green with the script run row registered in the same plan
- 808 workspace tests green, fmt + clippy -D warnings clean, zero new dependencies, route sources untouched
- Ready for 07-04 (the final plan of the phase/milestone)

---
*Phase: 07-ecosystem-interop-advanced-ops*
*Completed: 2026-08-29*

## Self-Check: PASSED

All key-files.created exist on disk; both task commits (cc189f7, ddf8723) present in git log; workspace clean of uncommitted 07-03 changes (only the unrelated untracked pterm_20260828031855.zip remains, which predates this plan and is not plan scope).
