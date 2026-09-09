---
phase: 10-eam-write-operations
plan: 02
subsystem: api
tags: [eam, wiremock, contract-tests, client-surface, reqwest, ignition-8.3]

# Dependency graph
requires:
  - phase: 10-eam-write-operations
    provides: 10-LIVE-CAPTURES.md Decisions-locked section (every wire shape modeled here is capture-backed)
  - phase: 08-foundations-session-core-config-contract
    provides: GatewayApi trait + Session seam, test-double stub chore, exit-class taxonomy
provides:
  - EAM write client surface in client/eam.rs: eam_task_suspend/resume/cancel (post_empty 204), eam_tasks_scheduled (envelope unwrap), eam_task_modify (single-element array PUT, Option body), eam_task_delete (collection=core, confirm only on opt-in)
  - Mutation outcome models: ResourceChange/MutationProblem/ModifyOutcome/DeleteOutcome parsing the captured {success, changes[], problem} shapes
  - EamScheduledTask 13-key wire-faithful model with String-const state vocabularies (EAM_TASK_STATES/EAM_CURRENT_STATES)
  - 29 wiremock contract tests in eam_contract.rs pinning every request (REQUEST-level: method+path+query+body) with capture-backed fixtures
affects: [10-03-actions-guards, 10-04-cli-tui, 10-05-live-gate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "REQUEST-pinning contract tests: assert the recorded request (method+path+query+body), not just the response — expect(1) + .mount() guard discipline"
    - "capture-locked String-const vocabularies (never enums) for wire state strings"
    - "signature-mismatch 500 recorded as module-doc FINDING — classification deferred to 10-03/10-04 action layer"

key-files:
  created:
    - crates/ignition-core/src/client/eam.rs (write surface; 472 lines added)
    - crates/ignition-core/tests/eam_contract.rs (write-verb pins; 588 lines added)
  modified:
    - crates/ignition-core/src/client/mod.rs (GatewayApi trait methods)
    - crates/ignition-core/tests/eam_contract.rs
    - 13 action files (test-double unreachable!() stubs for the 8 new trait methods)

key-decisions:
  - "Zero new classify arms — runtime verbs ride the existing path-scoped controller-403/not_found arms; contract tests PROVE the path-scoping catches the new URLs"
  - "Signature-mismatch 500 (captured: HTTP 500 + JSON problem body) is recorded as a module-doc FINDING for 10-03/10-04 — not classified in the client"
  - "eam_task_delete always sends collection=core; ?confirm= rides only on opt-in (capture Decision 3)"
  - "Modify PUT body is the full single-element array INCLUDING config.settings + original signature key (422-trap pinned at request level)"
  - "scheduled read rides the envelope unwrap; taskState/type stay String (never enum) per capture-locked vocabulary"

patterns-established:
  - "Per-verb contract pair: happy 204/200 pin + controller-403 classification pin (one per new verb)"
  - "Message-scoping proof test: 403 WITHOUT the controller message stays Auth — extends to new paths"
  - "Fixture provenance visible in test file: capture-backed per 10-LIVE-CAPTURES.md"

# Metrics
duration: ~175min (includes infra-outage recovery; ~90min agent execution + verification tail)
completed: 2026-09-09
---

# Phase 10 Plan 02: Client Surface Summary

**EAM write client surface — suspend/resume/cancel post_empty verbs, signature-keyed delete (collection=core), settings-mandatory array PUT modify, capture-locked scheduled-read model — with 29 REQUEST-pinning wiremock contract tests proving every wire shape against 10-LIVE-CAPTURES.md.**

## Performance

- **Duration:** ~175 min wall (includes executor infra outage; effective agent time ~90 min + orchestrator verification/commit tail)
- **Started:** 2026-09-09T06:52 (agent spawn)
- **Completed:** 2026-09-09T10:10
- **Tasks:** 2
- **Files modified:** 16 (2 created-in-plan, 1 trait, 13 test-double stubs)

## Accomplishments

- Path fns for all runtime verbs (suspend/resume/cancel raw names, scheduled/{true|false} literal words, modify as create-path twin, signature-keyed delete with encode_segment)
- EamScheduledTask: all 13 captured keys wire-faithful; taskState/type as String-const vocabularies — never enums
- Mutation outcome models parsing the captured {success, changes[], problem} envelope shapes
- All 15 GatewayApi test doubles stubbed with the established unreachable!() chore — zero silent trait breakage
- 29 contract tests: happy 204 pair per verb, controller-403 pair per NEW verb, message-scoping proof, modify 422-trap body pin, delete query pins, signature-mismatch FINDING test, 404 classification

## Task Commits

Each task was committed atomically:

1. **Task 1: Runtime verb paths + trait methods + capture-locked scheduled-read model** — `0acc03c` (feat)
2. **Task 2: Wiremock contract tests — per-verb REQUEST pinning + controller-403 pairs** — `6c5c214` (feat)

## Files Created/Modified

- `crates/ignition-core/src/client/eam.rs` — write-surface client fns + models (+472 lines)
- `crates/ignition-core/src/client/mod.rs` — GatewayApi trait methods (+151 lines)
- `crates/ignition-core/tests/eam_contract.rs` — 29 REQUEST-pinning contract tests (+588 lines)
- 13 action files (`actions/{apicall,connections,diagnostics,doctor,inspect,logs,projects,rig,script,sessions,tags,version,webdev}.rs`) — test-double stubs for 8 new trait methods

## Decisions Made

- Zero new classify arms: the existing path-scoped controller-403/not_found arms catch the new URLs — proven by test, not assumed (commit message: "verbs ride the path-scoped arms")
- Signature-mismatch 500 recorded as module-doc FINDING for the action layer (10-03/10-04 classify on the stable `signature mismatch` substring) — the client stays classification-free
- Delete policy per capture Decision 3: collection=core always, confirm only on opt-in
- Modify PUT: full single-element array body including config.settings — the 422 trap pinned at REQUEST level

## Deviations from Plan

### Auto-fixed Issues

**1. [Infra] Executor outage mid-Task-2 (API connection failure, then agent cancellation)**
- **Found during:** Task 2 (contract tests)
- **Issue:** Executor died after writing eam_contract.rs but before running full verify / committing / SUMMARY / STATE update; agent infra failed 3× consecutively
- **Fix:** Orchestrator completed the tail directly: ran Task 2 verify (contract suite 29/29 green; full `cargo test -p ignition-core` green across all 21 test binaries, 0 failures), committed Task 2 atomically, wrote this SUMMARY
- **Files modified:** none beyond the plan's own files
- **Verification:** full crate suite green (17 binaries in first run + tags 23, trial 7+2 ignored, webdev 13, live_gateway 12 ignored env-gated)
- **Committed in:** `6c5c214`

---

**Total deviations:** 1 (infrastructure recovery — no code change)
**Impact on plan:** Zero scope change; implementation was complete before the outage. Only verification/commit/bookkeeping moved to the orchestrator.

## Issues Encountered

- Host load average 57 (4-day uptime, heavy ambient load) made cold cargo builds run ~7 min and full-suite runs exceed 15-min shell timeouts twice — solved by output-to-file + incremental per-binary runs; results all green
- (Inherited from 10-01, honored here) find-body raw TAB bytes require lenient parsing — client parsers written non-strict per capture Decision 10

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- 10-03 (actions/guards) can build directly on: the 8 new trait methods, MutationOutcome/ModifyOutcome/DeleteOutcome models, the signature-mismatch FINDING (classify on stable substring), delete confirm-per-opt-in policy, and the late-500 suspend-window constraint from captures
- 10-04 (CLI/TUI) gets REQUEST-level proof that every wire call matches captures — the guard ladder composes over a proven surface
- 10-05 (live gate) can drive the full guarded lifecycle through the real client + action layers
- Open items carried: Running/Pending taskState rows and confirm-demand delete shape remain uncaptured (need GNET-connected-agent rig) — modeled passthrough per locked Decisions 2 and 3

---
*Phase: 10-eam-write-operations*
*Completed: 2026-09-09*

## Self-Check: PASSED

- Both task commits present: `0acc03c` (feat, 15 files +1057) and `6c5c214` (feat, +588)
- `cargo test -p ignition-core` full suite green: 21 test binaries, 0 failures (live_gateway 12 ignored, env-gated by design)
- Contract suite: 29 passed — per-verb happy+controller-403 pairs confirmed by test names; `grep -c "eam_not_controller\|EamNotController"` = 11
- SUMMARY.md created; STATE.md updated
