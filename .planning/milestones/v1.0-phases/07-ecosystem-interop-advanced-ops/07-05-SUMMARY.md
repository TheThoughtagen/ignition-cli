---
phase: 07-ecosystem-interop-advanced-ops
plan: 05
subsystem: api
tags: [eam, ignition, wiremock, uuid, serde, clap-help, error-classification]

# Dependency graph
requires:
  - phase: 07-ecosystem-interop-advanced-ops (plans 01-04)
    provides: EAM family (history/tasks/guarded create/force), classify seam, guard ladder, the live devops rig gateways
provides:
  - Wire-faithful EAM history decode (UUID-string taskIds)
  - eam task new composition matching the live 8.3.3 config.settings shape (422 eliminated)
  - Path-scoped 422 config-resource classification → invalid_input (exit 2)
  - TYPE help taxonomy enumeration (benign/mutating/refused + example)
affects: [07-06-PLAN (cli-research-backup verification asset), milestone UAT]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure compose_* helper extraction for unit-testable request-body composition"
    - "Path-scoped body-reading classify arms (422 edition of the EamNotController precedent)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/eam.rs
    - crates/ignition-core/src/actions/eam.rs
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/tests/eam_contract.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/tests/contract_eam.rs
    - README.md

key-decisions:
  - "EamHistoryItem.task_id is String — 8.3.3 controllers serialize taskId as a UUID string; #[serde(default)] stays tolerant"
  - "Create composition: config.profile={type,scheduleMode} ONLY (isSuspended server-owned, never sent on create); config.settings={targetGateways,targetGroups,+K=V}; zero --target defaults targetGateways to [\"_controller\"]"
  - "--definition deep-merges over the composed config.settings (was: profile) — the README settings shape was always right, the composition landed it in the wrong node"
  - "422 arm is path-scoped to /data/api/v1/resources/ and maps the gateway's {messages:[...]} body into InvalidInput (exit 2) with the message verbatim — client-composed body rejected by server = our payload problem, never internal_error"
  - "TYPE doc enumerates the guard ladder but NO possible_values — unknown future types must stay accepted and fail-safe to the --yes rung"

patterns-established:
  - "Raw-capture contract tests: mount the EXACT live debug-capture shape (not a sanitized image) as the wiremock body"
  - "Composition pins live in a pure fn (compose_task_definition) so the profile/settings split is unit-testable without a GatewayApi"

# Metrics
duration: 327min
completed: 2026-08-29
---

# Phase 7 Plan 5: EAM UAT Gap Closure Summary

**All three 07-UAT EAM gaps closed and live-verified against the real 8.3.3 controller: UUID-string history taskIds decode wire-faithful, `eam task new` composes the required `config.settings` shape (live create exits 0), config-resource 422s classify as exit-2 `invalid_input` carrying the gateway's own message, and `eam task new --help` enumerates the full guard-ladder taxonomy with a worked example.**

## Performance

- **Duration:** 327 min (machine load 80-200 from concurrent sessions dominated wall-clock; compile+test cycles took 20-30 min each)
- **Started:** 2026-08-29T16:12:12Z
- **Completed:** 2026-08-29T21:39:39Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments
- **Gap 1 (history decode):** `EamHistoryItem.task_id: i64 → String` — the real controller serializes `taskId` as a UUID string; `#[serde(default)]` could not rescue the type mismatch, so every live history read died as `internal_error`. New raw-capture-shaped contract test (`eam_history_decodes_the_raw_capture`) pins the exact `.planning/debug/eam-history-raw.json` image.
- **Gap 3 (create 422 + classification):** create body now matches the live working-definition shape (`config.profile={type,scheduleMode}`, `config.settings={targetGateways,targetGroups,…}` with the `--definition` overlay deep-merged over settings); zero `--target` defaults to `["_controller"]`. A path-scoped 422 classify arm on `/data/api/v1/resources/` surfaces the gateway's `messages` array as `invalid_input` (exit 2) — never `internal_error`.
- **Gap 2 (help UX):** TYPE doc enumerates benign/mutating/refused with FULL token lists matching `REFUSED_TYPES`/`MUTATING_TYPES` exactly, plus the unknown-type fail-safe note and the worked example `ign eam task new nightly-backup eam_backup --target gw-a`.
- Full workspace suite green: **857 passed, 0 failed** (26 opt-in live tests ignored); `cargo fmt --all` + `cargo clippy --all-targets -- -D warnings` clean.

## Task Commits

Each task was committed atomically:

1. **Task 1: EAM history decode is wire-faithful — UUID string taskIds** - `6e8236f` (fix)
2. **Task 2: eam task new composes config.settings; 422 → invalid_input** - `4bdbb81` (fix)
3. **Task 3: TYPE help enumerates the guard-ladder taxonomy** - `a2c6a8b` (feat)

**Plan metadata:** (see final commit below)

## Live Verification (gateway A, profile `uat`, http://localhost:9088, 8.3.3)

All probes run with `IGNITION_TOKEN=uatdiff:…` via the debug binary; gateway A was NOT reset (cli-research-backup asset preserved for 07-06).

**Gap 1 — `eam history` (exit 0, Failed/GNET entry as DATA):**
```
$ ign --profile uat eam history
[profile: uat]
2026-08-28T13:48:16.509Z  cli-research-backup (forced)  [Failed]  target=_controller  Attempt 1: Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'
(1 run(s))
EXIT=0
```
Compact mode confirms the passthrough taskId is the wire UUID, byte-equal to the raw capture:
```
{"ok":true,"profile":"uat","data":{"items":[{"taskId":"a2f4dab1-9a8f-4feb-9306-29e261f60453","taskName":"cli-research-backup (forced)",...,"level":"Failed","detail":"Attempt 1: Gateway network for agent '_controller' is currently not connected, the connection status is 'NotDefined'","taskType":"backup"}],"count":1}}
EXIT=0
```

**Gap 3 — `eam task new` creates (exit 0) and `eam tasks` lists it:**
```
$ ign --profile uat eam task new uat-backup-demo eam_backup
[profile: uat]
created uat-backup-demo (eam_backup / OnDemand)
{ "config": { "profile": {"scheduleMode":"OnDemand","type":"eam_backup"},
              "settings": {"targetGateways":["_controller"],"targetGroups":[]} },
  "name": "uat-backup-demo" }
EXIT=0

$ ign --profile uat eam tasks
cli-research-backup  type=eam_backup  schedule=OnDemand  state=-
uat-backup-demo  type=eam_backup  schedule=OnDemand  state=-
EXIT=0
```
The created `uat-backup-demo` definition is an OnDemand backup (never auto-fires) — left on the devops rig deliberately as a safe artifact.

**Gap 2 — `eam task new --help` taxonomy:**
```
<TYPE>
  Task type — the openapi taxonomy, three classes with different guard consequences:

  - benign (no --yes with the default OnDemand schedule): eam_backup

  - mutating (need --yes — they act on their agent targets when dispatched):
    eam_restart, eam_sendProject, eam_sendResource, eam_sendTags,
    eam_activateLicense, eam_updateLicense, eam_unactivateLicense

  - refused (exit 6 eam_task_type_refused — fleet-destructive; run from the EAM console):
    eam_restoreBackup, eam_installModules, eam_remoteUpgrade

  Any OTHER type fails safe to the --yes rung (unknown future types stay accepted, never silently unguarded).

  Example: ign eam task new nightly-backup eam_backup --target gw-a
```

**Guard-ladder regression (live binary):** `eam task new x eam_restoreBackup` → exit 6 `eam_task_type_refused`; `eam task new x eam_restart` (no --yes) → exit 2 `confirmation_required`; the `task_new_guard_ladder_refusals_do_zero_work` contract stayed green through both re-pins.

## Files Created/Modified
- `crates/ignition-core/src/client/eam.rs` — task_id String + wire-faithful unit fixtures
- `crates/ignition-core/src/actions/eam.rs` — profile/settings split via pure `compose_task_definition` + composition unit pins
- `crates/ignition-core/src/client/classify.rs` — path-scoped 422 → InvalidInput arm + `is_config_resource_url` tests
- `crates/ignition-core/tests/eam_contract.rs` — UUID taskIds, split-shape create bodies, live-shape list fixture
- `crates/ignition-cli/src/cli.rs` — TYPE taxonomy doc + `--definition` doc truth fix
- `crates/ignition-cli/tests/contract_eam.rs` — UUID goldens, raw-capture contract, no-target-default contract, 422-classification contract
- `README.md` — settings-merge + command-table rows updated to the config.settings composition

## Decisions Made
- `isSuspended` is never sent on create (unverified for create; the server defaults it on read-back) — profile carries `{type, scheduleMode}` only.
- The 422 arm reads the body (messages array joins into the reason; raw text otherwise; bare 422 note when empty) — the EamNotController body-reading precedent.
- No `possible_values` on TYPE: unknown future types must stay accepted and fail-safe to the `--yes` rung (server validation remains the backstop).
- TUI untouched: doc-string-only change; `tui_coverage` clap-walk row set unchanged, TUI Confirm parity intact (verified green in the workspace run).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Re-pinned the core integration suite `crates/ignition-core/tests/eam_contract.rs`**
- **Found during:** Task 2 (first full core test run)
- **Issue:** The plan listed only the CLI contract file, but the core integration suite `eam_contract.rs` also pinned the old wire images (numeric taskIds; settings-inside-profile create bodies) — 5 tests failed on the fixed model.
- **Fix:** Same re-pins as the CLI side: UUID taskIds in `history_page()`/force fixture, profile/settings-split expected create bodies, list fixture updated to the live split shape.
- **Files modified:** crates/ignition-core/tests/eam_contract.rs
- **Verification:** Full `-p ignition-core` suite green (all 18 result lines, 0 failures).
- **Committed in:** 4bdbb81 (Task 2 commit)

**2. [Rule 1 - Bug] `--definition` flag doc still said "over the composed profile"**
- **Found during:** Task 3 (`--help` output review)
- **Issue:** After Task 2's composition change, the cli.rs `--definition` help text was stale (merge target is `config.settings`, not profile).
- **Fix:** Doc string updated to "deep-merged over the composed `config.settings`".
- **Files modified:** crates/ignition-cli/src/cli.rs
- **Verification:** `eam task new --help` renders the corrected line.
- **Committed in:** a2c6a8b (Task 3 commit)

**3. [Rule 3 - Blocking] clippy `doc_lazy_continuation` on wrapped token lists**
- **Found during:** Task 3 verification (workspace clippy)
- **Issue:** The multi-line mutating/refused token lists in the TYPE doc comment lack list-continuation indentation — clippy `-D warnings` fails.
- **Fix:** Indented the continuation lines (renders identically in clap help).
- **Files modified:** crates/ignition-cli/src/cli.rs
- **Verification:** `cargo clippy --all-targets -- -D warnings` clean workspace-wide.
- **Committed in:** a2c6a8b (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All fixes were direct consequences of the plan's own wire-shape corrections landing across every fixture site; no scope creep.

## Issues Encountered
- The dev machine carried load averages of 80-200 from other concurrent agent sessions throughout execution; cargo build/test cycles took 10-30 minutes each and one 5-minute tool timeout left orphaned invocations. All suites completed green given patience — no code impact.
- One snapbox golden (force compact) needed `SNAPSHOTS=overwrite` re-pinning after a hand-edit brace-count slip; the written actual was verified against the fixture intent (UUID taskId passthrough only).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Ready for 07-06-PLAN.md. Gateway A remains UP and untouched (cli-research-backup (forced) Failed entry + the new uat-backup-demo OnDemand definition are both live verification assets).
- All three 07-UAT EAM gaps (1, 2, 3) closed; taxonomy additive (no new slugs — `invalid_input` reused per the two-place rule); guard ladder + TUI Confirm parity intact.

---
*Phase: 07-ecosystem-interop-advanced-ops*
*Completed: 2026-08-29*

## Self-Check: PASSED
