---
phase: 10-eam-write-operations
plan: 01
subsystem: research-captures
tags: [eam, live-capture, wire-truth, docker-rigs, ignition-8.3, controller-mode]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics
    provides: proven rig recipe (09-RIG-NOTES), headless commissioning + token provisioning wire recipes, capture-first template (09-LIVE-CAPTURES format)
  - phase: 07-ecosystem-interop-advanced-ops
    provides: openapi extract (endpoint inventory), controller-flip recipe (module-settings installMode), eam-create-422 evidence
provides:
  - 10-LIVE-CAPTURES.md — verbatim rig-labeled wire truth for all 12 research open questions + 12 citable "Decisions locked by captures"
  - 10-RIG-NOTES.md — ops log: rig recipe, headless EAM controller provisioning (rung a), teardown proof
  - Capture-locked vocabulary facts (taskState/currentState String sets, ScheduledTask 13-key shape, mutation envelopes, 500-problem signature-mismatch shape) for 10-02 client models
affects: [10-02-client-models, 10-03-actions-guards, 10-04-cli-tui, 10-05-live-gate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "capture-first: live dual-rig wire captures before any model code (09-02 pattern, re-proven)"
    - "read-modify-write with signature for all EAM config mutations (newSignature per-write, never cached)"
    - "String-const vocabularies with per-element rig provenance (never enums for wire state strings)"

key-files:
  created:
    - .planning/phases/10-eam-write-operations/10-LIVE-CAPTURES.md
    - .planning/phases/10-eam-write-operations/10-RIG-NOTES.md
  modified: []

key-decisions:
  - "suspend/resume runtime verbs SYNC config.profile.isSuspended into the definition (bidirectional, capture-proven both rigs)"
  - "signature mismatch = HTTP 500 + JSON problem{message,stacktrace} body, classify on stable substring 'signature mismatch' (prose drifts per rig)"
  - "rename via PUT is NOT supported (404 empty) — any rename verb must compose create-new + delete-old"
  - "delete without ?confirm= succeeds for lone resources; collection= param carries 'core' (collection name), not the type — recommend no-confirm default with confirm-retry policy (10-03 decides)"
  - "runtime-verb unknown-task is 500-HTML (suspend/resume) or silent 204 (cancel) — never 404; name validation rides the find-based preview read"
  - "EAM controller mode IS headlessly provisionable (module-settings installMode flip, rung a) — recorded for 10-05 live-gate rig reuse"

patterns-established:
  - "Controller flip recipe: singleton GET -> full-record clone -> one-key mutate -> array PUT with original signature"
  - "Non-strict JSON tolerance: find healthcheck stacktraces may contain raw TAB bytes — parsers must be lenient"
  - "Late-500 window: suspend right after task creation 500s until the scheduler trigger registers (~80s observed)"

# Metrics
duration: 24min
completed: 2026-09-09
---

# Phase 10 Plan 01: Live Wire Captures Summary

**Dual-rig capture of EAM task-lifecycle + config-mutation wire truth (8.3.3 + 8.3.6, both made controllers headlessly) — 12/12 research questions answered verbatim, several plan hypotheses falsified (suspend-500s, rename-404s, signature-mismatch-as-500), Decisions-locked section citable by every later plan.**

## Performance

- **Duration:** 24 min
- **Started:** 2026-09-09T09:45:18Z
- **Completed:** 2026-09-09T10:09:31Z
- **Tasks:** 2
- **Files modified:** 2 (both planning docs created)

## Accomplishments

- Both rigs (8.3.6 :18188, 8.3.3 :19188) spun, commissioned headlessly, token-provisioned, and promoted to EAM **controller mode with zero browser/UI** — landing rung (a) of the plan's fallback ladder on both rigs (the module-settings `installMode` flip, scripted from 07-RESEARCH)
- All 12 research open questions answered with verbatim rig-labeled evidence; every probe section labeled per rig (18 mentions each of 8.3.3/8.3.6); zero shapes guessed
- Five plan/research hypotheses corrected by capture: suspend of untriggered tasks = 500 (not 204/404); rename-PUT = 404 unsupported (not rename-or-create); signature mismatch = 500+JSON problem (not 400/409); `?collection=` takes `core` (not the type name); ScheduledTask record has 3 keys beyond the extract (`isForced`/`isRunning`/`progress`)
- Full teardown proven: zero ign-p10 containers/volumes, ports freed, both this-run AND stale Phase-9 token envs shredded

## Task Commits

Each task was committed atomically:

1. **Task 1: Spin both rigs, provision EAM controller mode, create scratch tasks** — `d82cabc` (chore — ops only, zero code)
2. **Task 2: 12-probe capture run → 10-LIVE-CAPTURES.md + teardown** — `3cb2b34` (docs)

**Plan metadata:** (final docs commit — see git log)

## Files Created/Modified

- `.planning/phases/10-eam-write-operations/10-LIVE-CAPTURES.md` — 376-line verbatim capture record: controller flip, suspend/resume isSuspended sync, taskState/can* truth, delete confirm ladder, signature-mismatch 500s, rename 404, PUT echo/422, 404-shape falsification, pagination envelope, problem-appearance rules, rig-drift table, scheduleDetails forms, 12 locked decisions
- `.planning/phases/10-eam-write-operations/10-RIG-NOTES.md` — 84-line ops log: rigs/ports/containers, commissioning + token recipes (3rd rig-generation proof), controller-provisioning recipe, scratch-task seeds, capture-run ops notes, teardown proof

## Decisions Made

- Landed rung (a) — headless controller provisioning — by scripting the 07-RESEARCH controller flip (singleton GET → clone → flip `installMode` → signature PUT) rather than accepting stock-gateway 403 captures (rung b); the flip itself doubled as the first PUT-envelope capture
- Wiped the prior aborted session's commissioned 8.3.3 volume before booting (clean-rig provenance over faster start)
- Recorded the plan's `?collection=eam-tasks` probe guess as falsified (404) and captured the correct `collection=core` variant — probe-spec corrections are captured, not silently fixed
- Recorded honestly uncapturable shapes as limitations (Running/Pending taskState rows — no connected-agent rig; confirm-demand delete shape — no dependent resources on a quiet rig) rather than guessing

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Docker daemon (OrbStack) not running at session start**
- **Found during:** Task 1 (rig spin)
- **Issue:** `docker` CLI failed — OrbStack socket absent
- **Fix:** `open -a OrbStack`, waited for daemon (29.4.0)
- **Files modified:** none (environment)
- **Verification:** `docker info` answered; all subsequent rig ops proceeded
- **Committed in:** n/a (env only)

**2. [Rule 1 - Bug] Stale prior-session rig leftovers contaminated the rig namespace**
- **Found during:** Task 1 (rig spin)
- **Issue:** an `ign-p10-836` container + a commissioned `ign-p10-833_*` volume from an aborted 2026-09-08 attempt at this plan still existed (8.3.3 came up RUNNING on old data — capture-provenance hazard)
- **Fix:** `docker rm -f -v ign-p10-836` + `docker compose down -v` before fresh boots
- **Files modified:** none (environment)
- **Verification:** fresh both-rigs COMMISSIONING state on new volumes; documented in RIG-NOTES
- **Committed in:** d82cabc (Task 1 commit, documented)

**3. [Rule 1 - Bug] First gateway-info sanity probe sent bare key instead of full `name:key`**
- **Found during:** Task 1 (token provisioning)
- **Issue:** `${T_836#*:}` stripped the token name; header requires the FULL `name:key` string (401 on both rigs)
- **Fix:** sent the full staged value; 200 on both
- **Files modified:** none
- **Verification:** gateway-info 200 both rigs
- **Committed in:** d82cabc (documented honestly in RIG-NOTES)

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs — all environment/procedure, zero code)
**Impact on plan:** All fixes were necessary to execute at all (daemon up, clean rigs, correct auth header). No scope creep; the probe list ran exactly as specified (with one probe-spec correction captured as evidence, not silently changed).

## Issues Encountered

- The plan's delete-probe spec (`?confirm=true&collection=eam-tasks`) used the type name where the wire expects the collection name — captured BOTH shapes (404 falsification + correct 200) rather than skipping; feeds Decision 3.
- Gateway `find` bodies embed raw TAB bytes inside healthcheck stacktrace strings (non-strict JSON) — python strict parser and jq both reject; lenient parsing used for the rest of the run. Locked as a parser-tolerance requirement for 10-02 (Decision 10).
- The first suspend attempt on the Scheduled task 500'd because the scheduler trigger hadn't registered yet (~80 s window after the cron PUT) — retested successfully; the late-500 window is now a documented model constraint.

## User Setup Required

None — no external service configuration required. (Rigs were ephemeral; nothing persists beyond the two planning docs.)

## Next Phase Readiness

- 10-02 (client models) can cite `10-LIVE-CAPTURES.md` "Decisions locked by captures" for every wire shape: mutation envelopes (`success/changes/newSignature/problem/references`), ScheduledTask 13-key shape, lifecycle verb behaviors (204/500-HTML), signature-mismatch 500+problem classification target, rename-unsupported, settings-mandatory
- 10-03 (actions/guards) gets: delete-confirm policy recommendation (Decision 3), find-before-write validation justification (Decision 7), late-500 suspend-window constraint
- 10-05 (live gate) gets: the headless controller-provisioning recipe (RIG-NOTES) reusable for the WHK-gate scratch-task procedure
- Open items for later phases: Running/Pending taskState rows and the confirm-demand delete shape remain uncaptured (need a GNET-connected agent rig); both are modeled passthrough per locked Decisions 2 and 3

---
*Phase: 10-eam-write-operations*
*Completed: 2026-09-09*

## Self-Check: PASSED

- 10-LIVE-CAPTURES.md / 10-RIG-NOTES.md / 10-01-SUMMARY.md all exist on disk
- Task commits `d82cabc` (chore) and `3cb2b34` (docs) present in git log; zero code files touched (planning docs only, verified via `git show --stat`)
- Rig-label counts: 8.3.3 ×18, 8.3.6 ×18 (≥12 required); Decisions-locked section present and covers all 12 probes
- Teardown proof: zero ign-p10 containers/volumes, ports 18188/19188 free, token envs shredded
- Key-links: CAPTURES→10-02 via Decisions section; RIG-NOTES→10-05 via controller recipe (5 mentions)
