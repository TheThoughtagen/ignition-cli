---
phase: 07-ecosystem-interop-advanced-ops
plan: "02"
subsystem: backup
tags: [gwbk, backup, eam, ignition-eam, wiremock, clap, ratatui]

# Dependency graph
requires:
  - phase: 04-rig-lifecycle-trial-state (04-04)
    provides: backup_download/backup_restore client methods, download_to_file streaming, restore POST shape, guarded-verb protocol
  - phase: 05-webdev-backend-tag-operations (05-04)
    provides: the config-resource family pattern (array-body POST, list/find) the EAM definitions seam reuses
provides:
  - "ign backup download [-o FILE] [--type roaming|all] — standalone gwbk on any profiled gateway"
  - "ign backup restore <FILE> --yes — the 8th --yes-guarded destructive verb (guard-before-resolution, binary-pinned)"
  - "ign eam history / eam tasks [NAME] — the EAM read family (runtime + config-resource seams)"
  - "ign eam task new / ign eam task force — the planner-locked typed guard ladder + the 3-request force sequence"
  - "eam_not_controller + eam_task_type_refused additive exit-6 slugs (two-place exit table, enumerated test extended)"
  - "task_create_guard pure ladder + parse_setting scalar auto-typing + deep_merge (reusable, unit-pinned)"
affects: [07-03 (script run), 07-04 (interop trio), EXT-03 v2 (EAM fleet verbs)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "classify-seam state-gate: path- + content-scoped 403 arm (the trial_not_expired pattern, classify edition)"
    - "ONE pure guard ladder fn shared by CLI pre-resolution, action re-check, and TUI Confirm gating"
    - "config-resource definition CRUD reusing the tag-provider array-body shape"
    - "chained dashboard Input forms (DashboardForm slot — the rig/projects form discipline, dashboard edition)"

key-files:
  created:
    - crates/ignition-core/src/actions/backup.rs
    - crates/ignition-core/src/client/eam.rs
    - crates/ignition-core/src/actions/eam.rs
    - crates/ignition-core/tests/eam_contract.rs
    - crates/ignition-cli/tests/contract_backup.rs
    - crates/ignition-cli/tests/contract_eam.rs
    - crates/ignition-cli/tests/e2e_eam.rs
  modified:
    - crates/ignition-core/src/client/backup.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/state.rs
    - crates/ignition-tui/src/update.rs
    - README.md

key-decisions:
  - "BackupType param-ized the ONE trait method (roaming default pinned by the path-builder unit test); rig snapshot passes Roaming explicitly — behavior byte-identical"
  - "backup restore = the 8th --yes-guarded verb, guard BEFORE resolution with the consequence+restart-block named in the operation string; the standalone restore is THIN (no bundled wait — rig restore owns the witnessed wait; README documents the window)"
  - "eam_not_controller classified at the classify seam, scoped by path prefix (/data/eam/) AND body content (configured as a controller) — both negatives wiremock-pinned; generic 403s stay auth_rejected"
  - "NO EAM controller verb and NO ign backup list (README honesty over verb theater); the manual installMode flip is a README recipe"
  - "Guard ladder LOCKED: backup+OnDemand unguarded / 7 mutating types + any non-OnDemand schedule need --yes / restore-install-upgrade REFUSE (eam_task_type_refused naming EXT-03) / unknown types fail-safe to NeedsYes"
  - "Settings composition: profile spreads type/scheduleMode/targetGateways/K=V auto-typed scalars; --definition deep-merges over the profile (objects merge, arrays/scalars replace); floats are NOT auto-typed (bool/int only — the tags-write rule)"
  - "force owner resolution via find (scheduledTaskState.details.owner, fallback eam) + history re-read — outcomes (Failed/GNET/trial) ride as DATA"

patterns-established:
  - "State-gate classification at classify() for module-scoped seams (message-classified, dual-scoped)"
  - "Pure-verdict guard fns as the single authority for conditional guards (CLI/TUI/action all call the same fn)"
  - "Chained dashboard forms via DashboardForm (multi-step Input chains on the global screen)"

# Metrics
duration: 133min
completed: 2026-08-28
---

# Phase 7 Plan 2: Standalone Backup Verbs + EAM Tasks Summary

**Standalone gwbk download (`--type roaming|all`) + the 8th guarded restore, plus the EAM family: history/tasks reads with the `eam_not_controller` state gate and the typed task-new/force guard ladder — zero new dependencies, 793 tests green.**

## Performance

- **Duration:** 133 min
- **Started:** 2026-08-28T16:55:24Z
- **Completed:** 2026-08-28T19:08:31Z
- **Tasks:** 3
- **Files modified:** 31 (7 created, 24 modified)

## Accomplishments
- `ign backup download|restore` on any profiled gateway — the Phase 4 wire surfaced as actions (export-convention default naming with the `.part` rename; rig-restore pre-checks), restore binary-pinned as the 8th `--yes`-guarded verb (exit 2 / profile null / zero resolution)
- EAM read family through TWO seams: runtime history (explicit-200-limit discipline, outcomes as data) and config-resource definitions (stock-gateway-safe) — with the controller 403 classified to the additive `eam_not_controller` slug (never mislabeled `auth_rejected`; path + content scoped, both negatives pinned)
- The planner-locked create ladder as ONE pure function (exhaustively unit-pinned over the taxonomy), shared by CLI pre-resolution guard, action re-check, and TUI Confirm gating; force = find→204→history 3-request sequence with owner resolution and honest outcome surfacing
- e2e skeleton honestly forking on controller state (`IGNITION_LIVE_EAM_CONTROLLER=1` + mutations opt-in; trial/GNET prerequisites as env-notes)

## Task Commits

Each task was committed atomically:

1. **Task 1: Standalone backup verbs** — `f458758` (feat) + `b2e6af0` (fix: golden cwd isolation)
2. **Task 2: EAM read family + state gate** — `5d1a51d` (feat)
3. **Task 3: EAM guarded writes + e2e skeleton** — `92c5555` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-core/src/client/backup.rs` — BackupType enum + path builder (roaming default pinned)
- `crates/ignition-core/src/client/eam.rs` — EAM wire constants (runtime + config-resource paths) + wire-faithful models
- `crates/ignition-core/src/client/mod.rs` — backup_download type param; 5 EAM trait methods + impls; 13 test doubles updated twice
- `crates/ignition-core/src/client/classify.rs` — the path/content-scoped 403 arm (eam_not_controller)
- `crates/ignition-core/src/error.rs` — 2 additive exit-6 slugs in both places; enumerated + hint tests extended
- `crates/ignition-core/src/actions/backup.rs` — download (.part rename convention) + restore (pre-checks, flat result)
- `crates/ignition-core/src/actions/eam.rs` — reads + the pure ladder + create/force + parse_setting/deep_merge
- `crates/ignition-cli/src/{cli,main,render}.rs` — Backup/Eam command trees, dispatch with guards, renderers
- `crates/ignition-core/tests/eam_contract.rs` + `crates/ignition-cli/tests/contract_backup.rs` + `contract_eam.rs` + `e2e_eam.rs` — wire/seq/body pins, goldens, live gate
- `crates/ignition-tui/src/{routes,state,update}.rs` — 6 rows, ACTIONS menu (7→13), DashboardForm chain, PendingAction×2, parity at 18 verbs
- `README.md` — Backups section, EAM section (gate/flip/ladder/forms/force), exit-table + destructive-ops updates

## Decisions Made
- (see key-decisions in frontmatter — all planner decisions implemented as locked; no architectural deviations)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Isolated the download JSON golden's cwd**
- **Found during:** Task 1 (golden verification)
- **Issue:** the compact-JSON download golden reused the cwd-less spawn helper — the default-naming download wrote `mock-gateway.gwbk` into the crate directory
- **Fix:** run that golden in a tempdir like the human golden; removed the stray artifact
- **Files modified:** crates/ignition-cli/tests/contract_backup.rs
- **Verification:** test passes; no `.gwbk` artifacts remain
- **Committed in:** b2e6af0

**2. [Rule 2 - Missing Critical] Restore file pre-checks on the standalone action**
- **Found during:** Task 1 (action authoring)
- **Issue:** the plan called backup_restore "thin orchestration"; without pre-checks a missing/empty/directory `--file` would fail mid-network (the client's read error) instead of the established usage-class shape
- **Fix:** reused rig restore's three pre-checks verbatim (exists + regular + non-empty → exit 2 pre-network) — wiremock-proven zero-request
- **Files modified:** crates/ignition-core/src/actions/backup.rs, crates/ignition-core/tests/backup_contract.rs
- **Verification:** pre-check test (zero requests) + goldens green
- **Committed in:** f458758 (part of Task 1)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical)
**Impact on plan:** Both small and consistency-driven; no scope creep.

## Issues Encountered
- The `--definition` file's merge LEVEL was ambiguous in plan prose ("deep-merged over the base {name, profile: {type, scheduleMode}}") — resolved as merge-over-`config.profile` (the file's top level IS the settings fragment), matching the pinned verbatim bodies; the composition is test-locked in both wiremock pins so the contract is now explicit.
- `MockServer::received_requests()` returns `Option<Vec>` (vs the scoped guard's `Vec`) — handled with `unwrap_or_default()` at each use.
- serde_json maps are key-sorted (no preserve_order) — the "pinned verbatim" create bodies pin the deterministic sorted order (recorded at the pins).

## User Setup Required
None — no new external services. (The e2e gate is opt-in via env vars per the established live-suite convention; no USER-SETUP.md needed.)

## Next Phase Readiness
- 07-02 delivered BKUP-01 + BKUP-02 in full; guard count now 8 (+ the conditional EAM new/force guards — 20 require_confirmation sites)
- tui_coverage green with all 6 new rows; every new verb landed its TUI surface in the same plan (the CI tripwire holds)
- Ready for 07-03 (script run — smallest plan: route contract already pinned; ride require_routes + the persisted secret)
- Zero new dependencies maintained (the success criterion)

---
*Phase: 07-ecosystem-interop-advanced-ops*
*Completed: 2026-08-28*

## Self-Check: PASSED

All 7 created files exist on disk; all 4 task commits verified in git log.
