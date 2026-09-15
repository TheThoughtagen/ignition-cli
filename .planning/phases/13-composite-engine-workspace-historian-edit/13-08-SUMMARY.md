---
phase: 13-composite-engine-workspace-historian-edit
plan: 08
subsystem: cli-surface
tags: [edit, clap, out-of-band, routes-parity, stdout-purity, scripted-editors, staleness, fail-closed, confirmation-gate, readme-contract]

# Dependency graph
requires:
  - phase: 13-05
    provides: edit_pipeline + StagedEdit/EditStatus + TokioEditor (VISUAL→EDITOR arg-vector seam) + EditTempDir keep() recovery + the stable refusal prefixes (no $EDITOR set / codec-verbatim / changed on gateway since fetch / preserved at <path>)
  - phase: 13-07
    provides: the Session-seam CLI dispatch shape (pre-resolve guards, error_profile envelope threading), the routes-row insertion points, the tui_coverage pin conventions, and the contract_workspace harness patterns (mount-order, scope-verified traffic pins, snapbox goldens)
  - phase: 08-06
    provides: the pre-declared `edit` OutOfBand reservation (row+command-atomic contract)
provides:
  - `ign edit <project> <resource-path> [--yes]` — the full kubectl-edit loop live at the binary level: fetch → decode → $EDITOR → content-decided no-op → fail-closed encode → NOT---yes-able staleness gate → ONE composed guard → overwrite project-import
  - The `edit` OutOfBand registry row landed ATOMICALLY with its clap command — the Phase-8 reservation fulfilled; the pinned set is exactly [api call, completions, edit]
  - Edit's concrete OutOfBand stdout contract: ZERO stdout bytes in every mode (byte-scan pinned under max diagnostics over the real spawned binary); success prose (`edit: no changes — nothing pushed`, `edit: pushed N member(s) to <project>`) and the blast-radius summary on stderr
  - dispatch_edit seam in main.rs (no ActionOutput::Edit variant BY DESIGN — render.rs untouched): editor-env resolves pre-resolve (null profile, zero requests), refusals ride the standard error envelope verbatim
  - tests/contract_edit.rs — 8-test binary suite: purity byte-scan (no-op + push legs), pre-resolve editor-env refusal (expect(0) wire), member-list naming refusal with a marker-file never-opened-editor proof, guard golden (summary IS the refusal), splice proof (re-parsed import body: edited member spliced, untouched members byte-exact), staleness fires with AND without --yes, fail-closed kept-tree proven on disk (redirected TMPDIR), no-op cleanliness (temp removed)
  - README edit contract section (loop, EDITOR/--wait contract, behavior table, zero-stdout), Commands-table row, fifth-exception streaming note, destructive-ops entry, workspace cross-link
affects: [Phase 14 (mcp/lsp join the OutOfBand set next — the pinned test documents the pattern), Phase-14 nvim/LSP slices ride the same editor round-trip conventions]

# Tech tracking
tech-stack:
  added: [] # zero new dependencies
  patterns:
    - "Main-level OutOfBand seam: a verb whose success renders NOTHING bypasses the ActionOutput envelope entirely (main special-case before the chassis, one ExitCode decision point) — the no-variant/no-render.rs-touch alternative to the TuiExited intercept pattern"
    - "One-gate preview composition at dispatch: the staged summary text is composed once and IS the refusal message without --yes, the pre-push blast-radius prose with it (the 10-04 preview_then_confirm shape carried to a non-enveloped verb)"
    - "TMPDIR redirection for binary-level temp-lifecycle proofs: the spawned binary's EditTempDir trees land in a test-owned dir, making kept-tree-on-failure and removed-tree-on-success race-free to assert"
    - "Ambient-EDITOR stripping in test harnesses (a developer shell's EDITOR=nvim otherwise spawns an interactive editor inside the test)"

key-files:
  created:
    - crates/ignition-cli/tests/contract_edit.rs
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/tui_coverage.rs
    - README.md

key-decisions:
  - "Edit dispatches on its own seam in main() BEFORE the normal chassis (a main-level special case, like Completions' pre-config early return) — the plan's own prohibitions (no ActionOutput::Edit variant, render.rs untouched) leave no in-chassis shape that can exit 0 with zero stdout; refusals still ride the SAME render_error envelope and ExitCode still has one decision point in main"
  - "The push is the CLIENT-level project_import call (session.project_import(zip, overwrite=true)) — not the actions::projects wrapper — so ImportDenied 200-denial honesty rides the client seam and no abort-policy find pre-check runs behind an already-gated destructive verb (the resource-put precedent)"
  - "The guard renders the summary only on the --yes path (prose before the push); without --yes the same text appears exactly once — as the refusal message — so 'the staged diff summary IS the refusal' holds without duplication"
  - "The summary mirrors 13-06's render_push_preview genre (count header + '  write:' lines) so ConfirmationRequired's 'is destructive; rerun with --yes to confirm' Display append lands in the established shape; the snapbox golden pins it byte-for-byte"

patterns-established:
  - "AlternatingResponder for multi-run fetch/recheck sequencing at the binary level (even hit = fetch export, odd hit = fresh export) — pairs correctly across any number of sequential runs in one test"
  - "Zero-stdout pins are assert-based byte scans (never snapbox) with the noise precondition asserted (bogus_key seen on stderr), extended to a non-enveloped verb"

# Metrics
duration: 57min
completed: 2026-09-15
---

# Phase 13 Plan 08: Edit CLI Surface Summary

**`ign edit` live end to end at the binary level — the kubectl-edit loop with content-decided no-op, fail-closed kept-tree recovery, NOT---yes-able staleness gate, and ONE composed guard whose refusal IS the staged diff summary — with the `edit` OutOfBand row landing atomically on its clap command (the Phase-8 reservation fulfilled, pinned set exactly [api call, completions, edit]) and the zero-stdout contract byte-scan proven under max diagnostics — SC-5 fully closed.**

## Performance

- **Duration:** 57 min
- **Started:** 2026-09-15T20:18:53Z
- **Completed:** 2026-09-15T21:16:14Z
- **Tasks:** 3
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments
- The `edit` reservation fulfilled ATOMICALLY (Task 1, one commit): `Commands::Edit(EditArgs)` (project + resource-path positionals, `--yes` stays global), the OutOfBand `CliRoute` row with its written justification, and the pinned-set extension to exactly `["api call", "completions", "edit"]` — the bidirectional clap walk green in the same landing; routes.rs module docs updated (`mcp`/`lsp` stay reserved for Phase 14)
- `dispatch_edit` — the OutOfBand seam: editor-env resolution FIRST (pre-resolve refusal: exit 2, envelope profile null, zero requests), `Session::resolve` (the Phase-8 seam), 13-05's `edit_pipeline` with every stable refusal prefix propagating verbatim through the standard error envelope, THE ONE GATE (without `--yes` the staged diff summary IS the refusal; with it the same text is the blast-radius prose), then the client-level project-import (overwrite replace, ImportDenied honesty intact); NO ActionOutput::Edit variant exists — render.rs untouched (grep-proven), zero stdout in every mode
- contract_edit.rs: 8 binary tests with scripted-`$EDITOR` shims (real `#!/bin/sh` processes through the production spawn site) + wiremock-served real exports — the purity byte-scan (no-op AND push legs under `IGNITION_LOG=trace` + unknown-key config + `-vvv`), the pre-resolve refusal pinned by `expect(0)` wire mocks in both human and JSON modes, the marker-file proof the editor never opens on a bad path, the guard golden byte-for-byte, the splice proof (re-parsed import body: edited member spliced, untouched members byte-exact through the real binary), staleness firing identically with and without `--yes`, the fail-closed kept-tree proven on disk under redirected TMPDIR, and no-op cleanliness (temp tree removed)
- README: the edit contract section (the loop, the `$EDITOR`/`--wait` IDE contract, the behavior table for all four save outcomes, the zero-stdout rule), the Commands-table row, the fifth stdout-exception note, the destructive-operations entry, and the workspace↔edit cross-link (whole-tree vs single-resource)

## Task Commits

Each task was committed atomically:

1. **Task 1: `ign edit` surface — clap command + dispatch + guard + OutOfBand row + pinned set (ONE atomic task)** - `4bcffe0` (feat)
2. **Task 2: stdout purity byte-scan + first contract tests (editor-env, resource-path, guard)** - `7a05ca2` (test)
3. **Task 3: full-loop binary contract tests + README edit section** - `7f4ffa5` (test)

## Files Created/Modified
- `crates/ignition-cli/src/cli.rs` — `Edit(EditArgs)` on Commands + `EditArgs` (globals-once honored)
- `crates/ignition-cli/src/main.rs` — the `dispatch_edit` OutOfBand seam + `render_edit_summary` (dispatch-arm stderr prose; no envelope involvement) + the runtime-unreachable chassis arm
- `crates/ignition-tui/src/routes.rs` — the `edit` OutOfBand row + justification; module docs updated (edit fulfilled, mcp/lsp still reserved)
- `crates/ignition-cli/tests/tui_coverage.rs` — pinned OutOfBand set exactly [api call, completions, edit] with the Phase-8 citation; header doc updated
- `crates/ignition-cli/tests/contract_edit.rs` (created) — the 8-test binary contract suite (797 lines)
- `README.md` — edit section + Commands row + streaming fifth exception + destructive-ops entry + workspace cross-link

## Decisions Made
- **Main-level seam instead of an in-chassis arm.** The plan's own locks (no `ActionOutput::Edit`, render.rs untouched) leave no in-chassis shape that can exit 0 with zero stdout — dispatch must return an ActionOutput, and any new variant forces an exhaustive-match edit in render.rs. `main` therefore special-cases `Commands::Edit` before the chassis (the Completions-precedent shape) and `dispatch_edit` returns the ExitCode directly; refusals still render through the SAME `render_error` envelope with the resolved profile threaded (CORE-01 preserved). Key-links hold: the gate site is the dispatch layer, the pipeline call and guard composition live in main.rs's edit dispatch.
- **Client-level import call.** `session.project_import(zip, overwrite=true)` rather than the actions wrapper: the client seam carries the ImportDenied 200-denial honesty, and no abort-policy `project_find` pre-check belongs behind an already-gated destructive verb (the resource-put overwrite precedent).
- **One-text-two-fates guard.** The summary renders only on the `--yes` path; without it the identical text appears exactly once as the refusal message — no duplication, and Task 3's "stderr = the diff summary" golden stays clean.
- **TMPDIR redirection for temp-lifecycle proofs.** The spawned binary's `EditTempDir` trees are redirected into test-owned dirs, making the fail-closed kept-tree and the success/no-op removal assertions race-free (the kept path provably lands where the message says).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Ambient EDITOR leaked into the binary test harness**
- **Found during:** Task 2 (first contract_edit.rs run)
- **Issue:** the developer shell's `EDITOR=nvim` reached the spawned binary in the pre-resolve test (only `VISUAL` was stripped), so nvim spawned inside the test and blocked forever — the suite hung
- **Fix:** `ign_cmd` strips both `VISUAL` and `EDITOR` (matching the purity harness's knob-stripping discipline); tests that need a scripted editor set `EDITOR` explicitly after the strip
- **Files modified:** crates/ignition-cli/tests/contract_edit.rs
- **Verification:** full suite green in ~3 s (was: infinite hang)
- **Committed in:** 7a05ca2 (Task 2 commit)

**2. [Rule 3 - Blocking] clippy `manual_is_multiple_of` in the new AlternatingResponder**
- **Found during:** Task 3 (final sweep: `cargo clippy --workspace --all-targets -- -D warnings`)
- **Issue:** `n % 2 == 0` tripped the clippy 1.94 lint, failing the plan's own verification gate
- **Fix:** `n.is_multiple_of(2)` (zero behavior change)
- **Files modified:** crates/ignition-cli/tests/contract_edit.rs
- **Verification:** workspace clippy -D warnings green
- **Committed in:** 7f4ffa5 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both fixes are test-harness mechanicalities required by the plan's own gates. No scope creep; no production-code deviation.

## Issues Encountered
- Wiremock static mocks accumulate matched requests across sequential runs against one server (the guard golden's two runs hit one `expect(2)` mock 4 times). Fixed by pinning the per-run arithmetic explicitly (`expect(4)` with the two-runs×two-GETs arithmetic documented) and keeping per-leg servers where legs differ. Recorded here so future binary-test authors reach for the arithmetic comment (or `up_to_n_times`) on the first pass.
- The decode tree lays members out at RAW zip names (`<collection>/resources/<rest>`), not user paths — the fail-closed kept-tree assertion initially probed the user path. Fixed to `VIEW_ZIP`; the raw-name layout is what `member_path` maps to and is now documented at the assertion.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness
- **Phase 13 is COMPLETE: all 8 plans executed with summaries; all 5 SCs carry binary-level evidence.** SC-5's user-facing half (this plan) joins 13-05's pipeline proof; SC-1 (workspace family, 13-07), SC-2 (mapping/decode, 13-03/13-07), SC-3/SC-4 (historian spike outcome, 13-01/13-02) stand as previously verified
- Phase 14 (`mcp`/`lsp`) inherits: the OutOfBand pinned-test pattern (extend the vec + written justification + row-with-command atomicity), the main-level OutOfBand seam precedent for protocol-speaking verbs, and the routes.rs reserved-slug comments now naming exactly the two remaining slugs
- Three-Place discipline untouched (no new exit slugs — all edit refusals ride existing invalid_input/confirmation_required classes; the README exit table is grep-verified in agreement); zero new dependencies
- Final battery green at 7f4ffa5: `cargo test --workspace` (60/60 test binaries, 0 failures), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED

- Files verified on disk: all 6 key-files (1 created: contract_edit.rs at 797 lines ≥ 80 min_lines; 5 modified) plus the SUMMARY itself
- Commits verified in history: 4bcffe0 (Task 1), 7a05ca2 (Task 2), 7f4ffa5 (Task 3)
- Pinned contracts re-verified by grep: OutOfBand set exactly [api call, completions, edit] in tui_coverage.rs; 10 zero-byte stdout asserts in contract_edit.rs; the staleness-with---yes pin present; render.rs UNTOUCHED since this plan's first commit (4bcffe0^)
- Final battery re-run green at 7f4ffa5: cargo test --workspace 60/60 binaries ok, clippy -D warnings clean, fmt --check clean
- No new slugs/deps — the Three-Place discipline untouched
