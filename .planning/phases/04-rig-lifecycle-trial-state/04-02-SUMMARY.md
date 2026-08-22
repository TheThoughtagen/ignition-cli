---
phase: 04-rig-lifecycle-trial-state
plan: 02
subsystem: rig
tags: [docker, compose, rig, reset, logs, streaming, tokio-process, guard, lsof]

# Dependency graph
requires:
  - phase: 04-rig-lifecycle-trial-state
    plan: 01
    provides: ComposeRunner seam, RigPlan discovery, LOCKED arg builders (down -v / logs_args pre-pinned), commissioned_wait, rig CLI tree + profile:null contract
provides:
  - rig_reset action — the guarded volume-teardown + bring-up cycle (preview → down -v --remove-orphans → fresh-eyes preflight → up → commissioned wait)
  - reset_preview (compose.rs) — project-labeled volume names via plain-docker volume ls, name-prefix-filtered
  - rig_logs action — compose log passthrough through a sink (one-shot captured, follow streamed); the THIRD sanctioned stdout exception
  - ComposeRunner::run_streaming — the piped-stdout streaming seam (lines forwarded as they arrive, stderr drained concurrently, Ctrl-C = foreground-group kill)
  - RigCommand::Reset/Logs CLI arms; require_confirmation guard-before-discovery binary pin (third instance of the destructive pattern)
  - commissioned_probe extracted in main.rs — the header-less rig-URL client shared by up and reset
affects: [04-03 trial (guard precedent + logs for diagnosis), 04-04 snapshot/restore (fresh-eyes preflight + commissioned-wait reuse), phase-06 TUI]

# Tech tracking
tech-stack:
  added: [] # no new crates — the tokio process feature already shipped in 04-01
  patterns:
    - "Streaming runner seam: dyn for<'a> FnMut(&'a str) + Send sinks (explicit HRTB required — elided FnMut(&str) trait objects mismatch across async_trait desugaring)"
    - "Concurrent stderr drain in streaming spawns (a full stderr pipe deadlocks a line-reading stdout loop)"
    - "Own-project-occupant fixtures keep preflight tests off the advisory lsof path — determinism on machines with running rigs"
    - "Guard-before-resolution extended to docker-only verbs: refusal exit 2 does zero DISCOVERY work (binary-pinned exit-2-not-exit-7)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/rig.rs
    - crates/ignition-core/src/rig/compose.rs
    - crates/ignition-core/src/rig/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_rig.rs
    - README.md

key-decisions:
  - "reset preview runs BEFORE the version gate (plan's numbered order): on docker-less machines the volume-ls spawn failure errors first — still exit 7 with the spawn reason, trading the friendly install hint for the plan's literal sequence"
  - "rig_logs skips the version gate entirely (plan specifies only the logs run): a missing docker surfaces as the exit-7 spawn failure on the logs invocation itself"
  - "RigLogsResult carries only {streamed} — the lines themselves streamed during execution (the LogsTail precedent); render_ok intercepts RigLogs in EVERY mode"
  - "Reset CLI timeout is u64 default 300 (family consistency with Up) over the plan's u32 sketch — zero behavioral difference"
  - "run_streaming sink needs explicit dyn for<'a> FnMut(&'a str) + Send — the elided form compiles per-signature but mismatches across async_trait boundaries (recorded for every future streaming seam)"
  - "Preview under-report on unlabeled/foreign-labeled volumes is inherent to the label filter (plan-LOCKED mechanism): docker's own down -v removes by compose declaration regardless — observed live (stale pre-label volume missed by preview, correctly removed, fresh volume now correctly previewed)"

patterns-established:
  - "Guard ordering pin by error-class contrast: exit 2 in a no-rig environment where discovery would exit 7 proves the guard fired first — reusable for every future destructive verb (04-03/04-04 already plan it)"
  - "Streaming actions take &mut (dyn FnMut(String) + Send) and count their own lines; dispatch owns the printing"

# Metrics
duration: 18min
completed: 2026-08-22
---

# Phase 4 Plan 2: Rig teardown + observability Summary

**`rig reset` (guarded preview → `down -v --remove-orphans` → fresh-eyes preflight → up → commissioned wait) + `rig logs` raw passthrough via a new streaming runner seam — live-verified end-to-end on the real ignition-devops rig, including the volume-preview naming exactly what reset removed**

## Performance

- **Duration:** 18 min
- **Started:** 2026-08-22T19:40:17Z
- **Completed:** 2026-08-22T19:58:54Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments

- **The reset cycle** (`rig_reset`): preview (project-labeled volume names, name-prefix defense) → version gate → `down -v --remove-orphans` (the LOCKED teardown — request shape pinned on the fake-runner call log) → port pre-flight BETWEEN the halves (teardown frees own ports; fresh eyes catch mid-cycle re-grabs with attribution + a torn-down-state hint) → `up --wait` → `commissioned_wait` reused verbatim (poll.rs diff-empty). Uncommissioned fresh volume = exit-0 data with the wizard URL.
- **The streaming seam** (`ComposeRunner::run_streaming`): piped stdout forwarded line-by-line until EOF/child exit, stderr drained CONCURRENTLY (pipe-deadlock-proof), spawn failure maps to 127. `rig_logs`: one-shot rides `run`, follow rides `run_streaming`; NO envelope in any mode (`rig logs --json` = same passthrough — the third sanctioned stdout exception, README §Streaming); compose diagnostics to our stderr via tracing, never the data sink.
- **CLI wiring**: `Reset {--timeout 300}` (guard fires BEFORE runner/discovery exist — binary-pinned by exit-2-not-exit-7 in a no-rig cwd) and `Logs {--tail 200, -f, [SERVICE]}` (println sink during execution; `render_ok` intercepts `RigLogs` like `LogsTail`). `commissioned_probe` extracted — the header-less client shared by up/reset.
- **Live end-to-end verification on the real rig**: guard refused (exit 2, profile null, hint) → `--yes` cycle ran (down -v → preflight → up → RUNNING) → a stale pre-label volume was honestly missed by the preview the first time, `down -v` removed it anyway, and the SECOND reset previewed + removed `ignition-devops_gateway_data` exactly — `{"removed_volumes":["ignition-devops_gateway_data"],"state":"running"}`. `rig logs --tail 6` streamed real gateway boot lines raw in both human and `--json` modes.

## Task Commits

Each task was committed atomically:

1. **Task 1: rig_reset action — guarded volume teardown + bring-up** - `746b415` (feat)
2. **Task 2: rig_logs action — passthrough streaming via run_streaming seam** - `2f665ac` (feat)
3. **Task 3: CLI wiring — reset arm (guarded), logs arm (streaming), README** - `c605ef5` (feat)
4. **Follow-up fix: deterministic rig action tests on machines with a running rig** - `80e797d` (fix)

**Plan metadata:** (see final docs commit)

## Files Created/Modified

- `crates/ignition-core/src/actions/rig.rs` - rig_reset + RigResetResult, rig_logs + RigLogsResult, reset test section, streaming tests, own-occupant fixtures
- `crates/ignition-core/src/rig/compose.rs` - reset_preview, run_streaming trait method + piped-spawn production impl (+ concurrent stderr drain), gated live streaming test
- `crates/ignition-core/src/rig/mod.rs` - reset_preview re-export, fake runner streaming impl
- `crates/ignition-cli/src/cli.rs` - RigCommand::Reset/Logs variants
- `crates/ignition-cli/src/main.rs` - guard-before-resolution reset dispatch, commissioned_probe helper, logs stdout sink, ActionOutput::RigReset/RigLogs
- `crates/ignition-cli/src/render.rs` - RigLogs interception (third exception), render_rig_reset_human / render_rig_logs_human
- `crates/ignition-cli/tests/contract_rig.rs` - reset guard zero-work pin, --yes discovery fallthrough, logs --help surface
- `README.md` - rig reset/logs command rows, reset semantics + logs passthrough sections, third streaming exception, destructive-ops list

## Decisions Made

- Preview-before-version-gate follows the plan's literal numbered order (a docker-less machine gets the volume-ls spawn error rather than the friendly compose install hint — both exit 7, both carry the reason).
- `rig_logs` runs no version gate (plan specifies only the logs run); a missing docker surfaces on the logs invocation itself.
- Reset's `--timeout` is `u64` default 300 (consistency with `Up`) over the plan's `u32` sketch — no behavioral difference.
- The streaming sink signature is `&mut (dyn for<'a> FnMut(&'a str) + Send)`: explicit HRTB is REQUIRED across async_trait boundaries (the elided `FnMut(&str)` compiles per-signature but mismatches at call sites) — recorded for every future streaming seam.
- Preview's label-filter under-report on unlabeled volumes (observed live) is inherent to the plan-LOCKED mechanism and self-heals: compose-created volumes carry the label, so the SECOND reset on any rig previews correctly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Rig action tests were machine-dependent when a real rig runs**
- **Found during:** final full-workspace verification (after live reset smoke)
- **Issue:** The empty docker-ps fixtures fell through to `port_preflight`'s advisory `lsof` pass, which observed the host's own rig (OrbStack forwarding 9088/9443) — 6 tests failed the moment live verification left the rig RUNNING. The 04-01 machine-isolation lesson, lsof edition.
- **Fix:** Script OWN-PROJECT occupants for the preflight fixtures: recreate-safe (the honest shape for up/reset on a running rig) AND non-empty, so the lsof fallback never runs. All tests green with the live rig publishing the fixture's exact ports.
- **Files modified:** crates/ignition-core/src/actions/rig.rs
- **Verification:** full workspace green while the rig stays up; `up_cycle_outputs` / `free_ports_for_own_project` / reset fixtures shared.
- **Committed in:** 80e797d

**2. [Rule 3 - Blocking] Streaming sink trait object needed explicit higher-ranked lifetimes**
- **Found during:** Task 2 (first compile)
- **Issue:** `&mut (dyn FnMut(&str) + Send)` compiles per-signature but mismatches across async_trait desugaring (`dyn for<'a> FnMut(&'a str)` vs lifetime-tied elision) — E0308 + E0597 at every seam.
- **Fix:** Explicit `dyn for<'a> FnMut(&'a str) + Send` on the trait method, the production spawn helper, and both fakes.
- **Files modified:** crates/ignition-core/src/rig/compose.rs, crates/ignition-core/src/rig/mod.rs, crates/ignition-core/src/actions/rig.rs
- **Verification:** workspace compiles, 61 rig tests green.
- **Committed in:** 2f665ac (part of task commit)

**3. [Rule 2 - Missing critical] Concurrent stderr drain in the streaming spawn**
- **Found during:** Task 2 (seam design)
- **Issue:** Reading piped stdout line-by-line while stderr fills its 64-KiB pipe buffer would deadlock the child (compose diagnostics arrive mid-stream on `logs -f`).
- **Fix:** stderr read to string in a spawned task, joined after stdout EOF + child wait.
- **Files modified:** crates/ignition-core/src/rig/compose.rs
- **Verification:** live `rig logs` runs against the real rig (stderr diagnostics present, no hang); gated live test.
- **Committed in:** 2f665ac (part of task commit)

---

**Total deviations:** 3 auto-fixed (1 bug, 1 blocking, 1 missing critical)
**Impact on plan:** All fixes were correctness/determinism requirements surfaced by live verification. No scope creep — LOCKED contracts untouched (envelope, exit taxonomy, poll.rs diff-empty).

## Issues Encountered

- One transient failure of `wait_restart_witnessed_path_golden` under first full-workspace parallel load (untouched path; passes in isolation and in every subsequent run) — noted as a pre-existing timing flake, not addressed here.
- Live verification unexpectedly ran a REAL reset (the smoke's cwd isolation doesn't stop level-5 convention discovery — only `IGNITION_RIG_ROOTS` does): turned into the plan's success-criterion evidence instead, then deliberately re-run once to pin the preview behavior on a correctly-labeled volume.

## Authentication Gates

None — docker-only family, header-less probes by design.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- RIG-01 COMPLETE (live-verified): reset leaves a fresh rig with volumes named in the data.
- RIG-02 half done: `rig logs [-f]` tails container output; the trial half (04-03) can reuse the streaming/README conventions and the guard pin pattern (04-03/04-04 already cite the reset precedent).
- `run_streaming` is available for any future follow-mode need (04-04 restore diagnostics, Phase-6 TUI log panes).
- The rig is currently UP and healthy (`ignition-devops`, gateway RUNNING) — ready for 04-03's live trial work.
- Live gateway flows (trial reset) still need rig creds per 04-03's user_setup.

---
*Phase: 04-rig-lifecycle-trial-state*
*Completed: 2026-08-22*

## Self-Check: PASSED

All key-files exist on disk; all 4 task commits (746b415, 2f665ac, c605ef5, 80e797d) verified in git log; must-have artifact `actions/rig.rs` at 1228 lines (≥ 100).
