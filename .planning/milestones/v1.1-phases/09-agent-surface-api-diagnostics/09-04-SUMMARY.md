---
phase: 09-agent-surface-api-diagnostics
plan: 04
subsystem: api
tags: [ignition, serde, wire-models, version-tolerance, clap, wiremock, contract-tests, tui-routes]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics (09-02)
    provides: 09-LIVE-CAPTURES.md — both rigs' verbatim /licenses, /trial, /redundancy, /overview/gan bodies (the parse fixtures)
  - phase: 09-agent-surface-api-diagnostics (09-03)
    provides: Session::resolve dispatch precedent + the api-call family this plan's rows sit beside
provides:
  - GatewayApi capability methods license_status / redundancy_status / gan_status over version-tolerant wire models
  - Three one-command curated reads: ign license status (trial merge), ign redundancy status (units capture-documented), ign gan status (zero-connection canonical shape)
  - tests/contract_diagnostics.rs — binary contract home, declared shared with 09-05
  - TUI Dashboard rows for the three leaves (clap walk green)
affects: [09-05 (extends contract_diagnostics.rs + diagnostics family), 09-06 (live gates prove these reads live)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Partial-curated wire model: morning-check fields typed, array elements + deep remainder passthrough (Vec<Value>/Option<Value> + flatten extra)"
    - "Unit-normalization helper on the wire model (last_sync_epoch_ms filters the -1 sentinel; inference flagged in docs)"
    - "Trait-mock stub chore: new GatewayApi methods require unreachable! stubs in all 13 test implementors"

key-files:
  created:
    - crates/ignition-core/src/client/license.rs
    - crates/ignition-core/src/client/redundancy.rs
    - crates/ignition-core/src/client/gan.rs
    - crates/ignition-core/src/actions/license.rs
    - crates/ignition-core/src/actions/redundancy.rs
    - crates/ignition-core/src/actions/gan.rs
    - crates/ignition-cli/tests/contract_diagnostics.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/src/actions/{apicall,connections,doctor,inspect,logs,projects,rig,script,sessions,tags,version,webdev}.rs (trait-mock stubs)
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - README.md

key-decisions:
  - "role stays a String, not an enum — only Independent was captured; an unknown future role must ride, never refuse"
  - "lastSyncTimestamp: -1 sentinel (never synced) normalized to None via last_sync_epoch_ms(); the ms unit is flagged INFERENCE in docs (not capture-proven — no sync ever happened on the fresh rigs)"
  - "GAN byte rates f64 (parses both 0 and 0.0 wire forms); counts i64 — the captured number shapes"
  - "license arrays other than hardware stay Vec<serde_json::Value> passthrough elements (element shapes NOT capturable on fresh rigs); hardware keeps the lenient key+items skeleton with details never modeled"
  - "license action merges the trial companion — /licenses carries NO mode/edition keys on the captures (capture wins over the plan's field sketch); mode + countdown ride TrialWire"
  - "contract_diagnostics.rs module doc declares the file as 09-05's shared home"

patterns-established:
  - "Capture-cited doc comments: every typed wire field names its capture section/rig"
  - "Unknown-key round-trip test in every client module (fake future point-release key rides extra)"
  - "Envelope passthrough proof at the binary level (mounted unknown key rides into data)"

# Metrics
duration: 168min
completed: 2026-09-07
---

# Phase 9 Plan 4: Curated Morning-Check Reads Summary

**Three one-command curated reads (`license`/`redundancy`/`gan` status) over capture-backed version-tolerant wire models — fixtures ARE the 09-02 live captures from both rigs**

## Performance

- **Duration:** 168 min
- **Started:** 2026-09-07T08:20:00Z
- **Completed:** 2026-09-07T11:08:58Z
- **Tasks:** 3
- **Files modified:** 22 (7 created, 15 modified)

## Accomplishments
- `ign license status` answers in ONE command: trial mode/countdown line + per-hardware-key item rows, deep/variable remainder riding flatten passthrough — both rigs' captured shapes parse as pinned tests
- `ign redundancy status` ships the flat 11-field model with units documented FROM the captures (uptime = ms since gateway start, wall-clock proven twice; `-1` = never-synced sentinel, ms interpretation flagged as inference)
- `ign gan status` ships the 5-field overview/gan model; the zero-connection capture is the canonical shape (f64 rates parse int and float wire forms)
- All three leaves dispatch through `Session::resolve`, carry Dashboard TUI rows in the SAME task (clap walk green), and are binary-contract-pinned in `contract_diagnostics.rs` including the 404→6 / 401→5 curated-pipeline non-leak proof

## Task Commits

Each task was committed atomically:

1. **Task 1: version-tolerant wire models + capability methods, capture-backed parse tests** - `6928bfc` (feat)
2. **Task 2: actions + CLI leaves + dispatch + render + TUI Dashboard rows** - `16d4700` (feat)
3. **Task 3: binary contract tests + README command rows** - `79bea2b` (feat)

## Files Created/Modified
- `crates/ignition-core/src/client/license.rs` - LicenseStatusWire (partial-curated + flatten), lenient hardware skeleton, item_count()
- `crates/ignition-core/src/client/redundancy.rs` - RedundancyStatusWire (flat 11-field, unit-documented) + last_sync_epoch_ms()
- `crates/ignition-core/src/client/gan.rs` - GanStatusWire (5 scalars, i64/f64 split per capture)
- `crates/ignition-core/src/client/mod.rs` - three GatewayApi capability methods (get_json authed) + module wiring
- `crates/ignition-core/src/actions/{license,redundancy,gan}.rs` - LicenseStatusResult (license + trial merge), {status} wrappers
- `crates/ignition-cli/src/cli.rs` - License/Redundancy/Gan top-level families, uniform Status leaves
- `crates/ignition-cli/src/main.rs` - three Session::resolve dispatch arms + ActionOutput/envelope arms
- `crates/ignition-cli/src/render.rs` - human tables (units as captured, never guessed)
- `crates/ignition-tui/src/routes.rs` - three Screen(Dashboard) rows
- `crates/ignition-cli/tests/contract_diagnostics.rs` - 4 binary contract tests (09-05's declared shared home)
- `README.md` - three command-reference rows (trial merge, units, zero-connection meaning)
- 12 action files - trait-mock stubs for the three new GatewayApi methods

## Decisions Made
- `role` is a String, not an enum (only `Independent` captured; unknown roles ride)
- `lastSyncTimestamp` normalization lives on the model (`last_sync_epoch_ms()` → `None` for -1/absent); docs flag the ms unit as inference
- Non-hardware license arrays typed as `Vec<serde_json::Value>` passthrough elements (element shapes not capture-proven on fresh rigs)
- The license morning-check MODE comes from the trial companion — the captures show `/licenses` has no mode/edition keys (capture wins over the plan's field sketch, recorded per plan instruction)
- `contract_diagnostics.rs` module doc declares it as 09-05's shared home per plan

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] GatewayApi trait growth required stubs in all 13 test-mock implementors**
- **Found during:** Task 1
- **Issue:** Adding `license_status`/`redundancy_status`/`gan_status` to the GatewayApi trait broke compilation of every test-mock implementor across 12 action files (E0046) — the plan's Task 1 file list named only client/{license,redundancy,gan,mod}.rs
- **Fix:** Inserted the established `unreachable!("not part of this action")` stubs (the 09-03 api_call chore pattern) into all 13 mocks; first scripted attempt dropped the api_call method bodies (script bug — restored from HEAD, re-applied correctly)
- **Files modified:** crates/ignition-core/src/actions/{apicall,connections,doctor,inspect,logs,projects,rig,script,sessions,tags,version,webdev}.rs
- **Verification:** cargo test -p ignition-core --lib green (346 passed)
- **Committed in:** 6928bfc (Task 1 commit)

**2. [Rule 1 - Bug] Doc-comment wording kept the plan's zero-hit verification gate honest**
- **Found during:** Task 1
- **Issue:** Two module doc comments mentioned the literal string `deny_unknown_fields` in prose, which would make the plan's `rg 'deny_unknown_fields' → ZERO hits` verification fail spuriously
- **Fix:** Reworded both comments ("unknown keys never refuse the parse")
- **Files modified:** crates/ignition-core/src/client/license.rs, crates/ignition-core/src/client/redundancy.rs
- **Verification:** rg returns zero hits in the three client files
- **Committed in:** 6928bfc (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both mechanical consequences of the planned trait change / verification gate; no scope creep. Capture-vs-sketch note (mode/edition absent from `/licenses`; count = typed hardware items) is a planned capture-wins ruling, not a deviation.

## Issues Encountered
- The full `cargo test --workspace` gate takes ~50 minutes wall-clock in this repo (53 test binaries); ran to completion in the background: 929 passed, 0 failed, doc-tests green. No code issue — noting the runtime cost for future plan sizing.

## User Setup Required

None - no external service configuration required. (Live proof of these reads is 09-06's job, per the plan's success criteria.)

## Next Phase Readiness
- Ready for 09-05 (diagnostics bundle family): `contract_diagnostics.rs` is declared and seeded as its shared test home; the bundle's Pitfall-2 capture (PascalCase `Generating`/`Valid`, absent-while-generating `fileSize`) is already locked in 09-LIVE-CAPTURES.md
- Ready for 09-06 (live gates): all three reads exist as one-command verbs whose wire models were built exclusively from the captured bodies — the live gate needs only to re-witness them

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-07*

## Self-Check: PASSED

All 7 created files exist on disk; all 3 task commit hashes present in git log; routes.rs Dashboard rows and README command rows verified present. Workspace suite green (929 passed / 0 failed) recorded during execution.
