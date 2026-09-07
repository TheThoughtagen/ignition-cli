---
phase: 09-agent-surface-api-diagnostics
plan: 06
subsystem: testing
tags: [e2e, live-gates, env-gated, docker-rigs, ignition-8.3, diagnostics-bundle, api-call]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics (09-02)
    provides: both-rig capture truth (09-LIVE-CAPTURES.md) + the rig provisioning/commissioning recipe (09-RIG-NOTES.md)
  - phase: 09-agent-surface-api-diagnostics (09-03/04/05)
    provides: the shipped CLI surface under test (api call, license/redundancy/gan status, diagnostics bundle family)
provides:
  - e2e_api_diagnostics.rs — six env-gated live gates (quiet green no-op without envs), the phase's live-verification oracle
  - Recorded both-rig PASS evidence for every gate (success criteria 3 + 4 live-verified, not claimed)
  - First-time live capture of the gateway-info body (ignitionVersion key shape, both 8.3.3 and 8.3.6)
affects: [phase-14-transports (MCP catalog trust), any future live-rig gate runs, point-release variance monitoring]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "env-gated live gate: #[ignore] + early-return skip when envs absent — plain and -- --ignored runs are green no-ops without IGNITION_LIVE_URL/TOKEN"
    - "mutations opt-in: the one mutating gate requires IGNITION_LIVE_MUTATIONS=1 with the e2e_webdev refusal early-return"
    - "capture-wins rule: gate assertions cite 09-LIVE-CAPTURES sections; a falsified hypothesis is asserted as falsified, never resurrected"

key-files:
  created:
    - crates/ignition-cli/tests/e2e_api_diagnostics.rs
  modified:
    - .planning/phases/09-agent-surface-api-diagnostics/09-RIG-NOTES.md

key-decisions:
  - "DELETE probe asserts the CAPTURED 404-empty answer (exit 6 not_found) — the plan's 405/exit-2 hypothesis stayed falsified per capture LOCKED decision 8; classify.rs maps every 404 before the api-call catch-all"
  - "gateway-info spot-key corrected mid-run from the version hypothesis to the LIVE-captured ignitionVersion key — the run's one honest failure, exactly the point-release-variance failure mode this phase closes"
  - "Read-only discipline is structural: zero mutating verb strings in the file (args are separate tokens); bundle generate is the sole mutation, MUTATIONS-gated, touching nothing else (capture-proven benign loop)"
  - "Sync tests + one std::sync::Mutex LIVE_GATE serializer instead of tokio — no raw reqwest probes needed; everything rides the real binary"

patterns-established:
  - "Live-gate env contract: IGNITION_LIVE_URL + IGNITION_LIVE_TOKEN (full name:key) for reads; IGNITION_LIVE_MUTATIONS=1 unlocks ONLY the single opt-in mutation"
  - "Both-rig matrix as the verification oracle: every gate PASS recorded per rig with wall times in 09-RIG-NOTES.md before teardown"

# Metrics
duration: 2h 11min
completed: 2026-09-07
---

# Phase 9 Plan 06: Live Gate Matrix (Both Rigs) Summary

**Six env-gated live gates over the real `ign` binary, ALL PASS on both docker rigs (8.3.6 + 8.3.3) with recorded timings — success criteria 3 + 4 live-evidenced, plus a first-time gateway-info wire capture that corrected one gate mid-run.**

## Performance

- **Duration:** 2h 11min (16:31Z → 18:42Z; ~1h of it rig commissioning + the workspace test build)
- **Started:** 2026-09-07T16:31:24Z
- **Completed:** 2026-09-07T18:42:27Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- The phase's verification oracle shipped and RAN: six env-gated gates (license/redundancy/gan reads, api-call envelope, read-only 4xx partition, bundle round-trip) — **6/6 PASS on 8.3.6 AND 6/6 PASS on 8.3.3**, plus the MUTATIONS-gated bundle round-trip (generate→wait→download→status, ZIP magic + fileSize equality) PASS on both rigs (3.82 s / 4.00 s)
- Quiet no-op contract proven: no-env `cargo test` → 6 ignored green; no-env `-- --ignored` → 6 skip-passes green; CI never touches a rig
- First-time live capture of the `GET /data/api/v1/gateway-info` body — key shape identical across 8.3.3/8.3.6 (`ignitionVersion`, `redundancyRole`, `edition`, `allowUnsignedModules`, `license{mode,expirationDate,…}`), recorded in 09-RIG-NOTES.md
- Full phase closure: rigs provisioned per the 09-02 recipe (~15 min to RUNNING this time, warm layers), gates run, teardown verified (zero ign-p9 containers/volumes, ports freed, token scratch removed), workspace suite green

## Task Commits

Each task was committed atomically:

1. **Task 1: write the env-gated live gates (quiet no-op without envs)** - `d5a1226` (feat)
2. **Task 2: run the full gate matrix on BOTH rigs, record, teardown** - `d832068` (docs)

**Plan metadata:** the `docs(09-06): complete live-gate matrix plan` commit (see `git log --grep "docs(09-06)"`)

## Files Created/Modified

- `crates/ignition-cli/tests/e2e_api_diagnostics.rs` (created, 558 lines) — the six live gates + env helpers + LIVE_GATE serializer; every assertion cites 09-LIVE-CAPTURES
- `.planning/phases/09-agent-surface-api-diagnostics/09-RIG-NOTES.md` (modified) — 09-06 run section appended: re-provisioning, per-rig PASS lines with timings, the gateway-info delta capture, teardown record

## Decisions Made

- **DELETE probe asserts the captured answer, not the plan hypothesis:** the plan's gate-5 sketch said exit-2 `gateway_client_error` ("the capture proved 405") — the capture proved 404-EMPTY, and `classify.rs` maps every 404 to `NotFound` BEFORE the api-call catch-all fires. Per the plan's own escape clause ("if the capture recorded something else, assert THAT"), the gate pins exit 6 `not_found`, with the falsification documented inline.
- **Spot-key corrected to live truth:** gate 4 originally spot-checked `version` in the gateway-info body — a hypothesis (09-02 never captured that body). It failed honestly on rig A's first pass; the gate now asserts `ignitionVersion` (the gateway-native key `GatewayInfo` itself renames in). Both rigs green after.
- **Structural read-only proof:** no `POST`/`--method PUT`/`--method DELETE` string appears anywhere in the gate file (args are separate tokens; the pinned DELETE probe is array elements) — the plan's rg check returns zero matches, the strongest possible reading of the read-only discipline.
- **Sync test shape:** plain `#[test]` + one `std::sync::Mutex` LIVE_GATE (no tokio/reqwest needed — every step rides the real binary), keeping the e2e_webdev one-at-a-time discipline.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] api-call envelope gate spot-key failed live; corrected to the captured key**
- **Found during:** Task 2 (rig A first pass, 17:10:23Z)
- **Issue:** the gate asserted `data.result.data.version`, but the real gateway-info body carries the version under `ignitionVersion` (the wiremock mocks serve the alias name, which `GatewayInfo` only tolerates via `#[serde(alias)]`)
- **Fix:** gate spot-key changed to `ignitionVersion`, comment cites the run's own capture; delta recorded in 09-RIG-NOTES.md per the plan's Task-2-step-3 protocol
- **Files modified:** crates/ignition-cli/tests/e2e_api_diagnostics.rs
- **Verification:** both rigs 6/6 PASS after the fix; this is the documented point-release-variance failure mode the phase exists to close
- **Committed in:** d832068 (Task 2 commit; the gate file fix rode the Task-2 edit pass, file first committed in d5a1226)

---

**Total deviations:** 1 auto-fixed (1 bug — gate assertion grounded in capture over hypothesis)
**Impact on plan:** No scope creep; the fix is the plan's own "gate must encode live truth, never the hypothesis" directive executed.

## Issues Encountered

- The `cargo test --workspace` phase check needed ~35 min of cold-ish compilation (ran in background; 54/54 test-result lines green, zero failures). No product issues.

## User Setup Required

None — no external service configuration required. (Rigs were disposable; everything torn down.)

## Next Phase Readiness

- **Phase 09 is COMPLETE (6/6 plans):** the agent surface + API diagnostics milestone work is delivered and live-verified on two gateway point releases — curated reads, raw passthrough, bundle family, all proven against wire truth, not just wiremock.
- Phase 14 (transports/MCP) can trust the capture-grounded models: the gate matrix doubles as the point-release-variance early-warning harness (`IGNITION_LIVE_URL/TOKEN (+MUTATIONS) cargo test -p ignition-cli --test e2e_api_diagnostics -- --ignored` against any future rig).
- Standing prerequisite blockers for later phases unchanged: licensed-Historian rig access (Phase 13), real multi-level UDT export (Phase 11).

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-07*

## Self-Check: PASSED

- crates/ignition-cli/tests/e2e_api_diagnostics.rs exists on disk
- 09-RIG-NOTES.md 09-06 run section present (per-rig PASS lines + teardown)
- Both task commits verified in git log (d5a1226, d832068)
