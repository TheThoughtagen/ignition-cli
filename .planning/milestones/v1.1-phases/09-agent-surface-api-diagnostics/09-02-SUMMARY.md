---
phase: 09-agent-surface-api-diagnostics
plan: 02
subsystem: api-diagnostics
tags: [live-capture, ignition-8.3, docker-rig, api-token, diagnostics-bundle, wire-truth]

# Dependency graph
requires:
  - phase: 04-rig-lifecycle-trial-state
    provides: headless token-provisioning recipe (collection:"core" + security-properties 403-fix) and the docker rig launch patterns
provides:
  - "09-LIVE-CAPTURES.md — rig-labeled verbatim bodies for /licenses, /trial, /redundancy, /overview/gan, bundle generate→status→download, and 4xx probes on BOTH 8.3.3 and 8.3.6"
  - "BUNDLE_GENERATING_STATES evidence: exactly [\"Generating\"] (PascalCase), terminal = Valid, fileSize absent-until-Valid (bytes)"
  - "Unit decisions: uptime = ms-since-gateway-start (wall-clock proven ×2), fileSize = bytes, lastSyncTimestamp = -1 never-synced sentinel (unit not capture-proven)"
  - "Live 4xx partition for 09-06 gates: 404+HTML (unknown path) vs 404+EMPTY (wrong method, NOT 405) vs 401+HTML (bad auth)"
  - "Headless 8.3 commissioning recipe via commissioner wire API (bootstrap → eula-accept → start-gateway) — image env vars ACCEPT_EULA/GATEWAY_ADMIN_PASSWORD are dead on 8.3"
affects: [09-04-license-models, 09-05-bundle-models, 09-06-diagnostics-cli-gates, future rig-automation phases]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "capture-never-guess: PascalCase wire enums + absent-vs-null-vs-zero keys pinned from live rigs before any model is written"
    - "commissioner wire-API commissioning: /bootstrap → /get-step → /post-step {id,step,data} step driving"

key-files:
  created:
    - .planning/phases/09-agent-surface-api-diagnostics/09-LIVE-CAPTURES.md
    - .planning/phases/09-agent-surface-api-diagnostics/09-RIG-NOTES.md
  modified: []

key-decisions:
  - "Bundle states captured PascalCase: Generating → Valid; BUNDLE_GENERATING_STATES = [\"Generating\"]; unobserved states must passthrough, never assume"
  - "uptime unit = ms-since-gateway-start (two independent wall-clock proofs); lastSyncTimestamp = -1 sentinel → model Option<i64>, ms per sibling evidence flagged as inference"
  - "DELETE on a real read-only path answers 404 EMPTY, not 405 — the plan's 405 hypothesis falsified by capture; 4xx partition is 404-HTML / 404-empty / 401-HTML"
  - "License array ELEMENT shapes uncapturable on fresh rigs (no leases) → morning-check fields typed, element interiors passthrough (09-04 split confirmed)"
  - "8.3 commissioning is headless via commissioner wire API; docker-entrypoint ignores ACCEPT_EULA/GATEWAY_ADMIN_PASSWORD (8.1-era vars)"

patterns-established:
  - "StatusPing 200 ≠ commissioned: must poll until details=COMMISSIONING disappears (state RUNNING + no details)"
  - "token keys ride a chmod-600 /tmp scratch env sourced per command batch (persistent-shell env does not survive between tool calls); shredded at teardown"

# Metrics
duration: 62 min
completed: 2026-09-07
---

# Phase 09 Plan 02: Live Rig Wire Captures Summary

**Verbatim wire captures from live 8.3.3 + 8.3.6 rigs — bundle states (`Generating`→`Valid` PascalCase), ms-since-start uptime, and the 404-empty (not 405) method-refusal partition — locking every shape 09-04/09-05 will model as capture-backed evidence.**

## Performance

- **Duration:** 62 min
- **Started:** 2026-09-07T01:45:08Z
- **Completed:** 2026-09-07T02:48:02Z
- **Tasks:** 2
- **Files modified:** 2 (both created; both planning docs, no code changes per plan)

## Accomplishments

- Both rigs (8.3.6 :18188 `ign-p9-836`, 8.3.3 :19188 `ign-p9-833`) spun, commissioned, and headlessly token-provisioned; authed `GET /data/api/v1/gateway-info` → 200 per rig
- All six probe groups captured VERBATIM and rig-labeled in 09-LIVE-CAPTURES.md: licenses, trial, redundancy, overview/gan, bundle generate→status→download, 4xx probes — with a 9-item "Decisions locked by captures" section
- Pitfall 2 dodged: bundle state strings are **PascalCase** (`Generating`, `Valid`) — lowercase assumptions would have shipped wrong; `BUNDLE_GENERATING_STATES = ["Generating"]` is recorded evidence
- The 4xx partition 09-06 needs is captured live — including the falsification of the plan's own 405 hypothesis (real answer: 404 + empty body)
- Full teardown verified: zero `ign-p9*` containers/volumes, ports freed, key material shredded
- Bonus discovery: complete headless commissioning recipe for 8.3 via the commissioner wire API (image env vars are 8.1-era and ignored)

## Task Commits

Each task was committed atomically:

1. **Task 1: Spin + commission + headless-token-provision both rigs** - `58b7831` (chore)
2. **Task 2: Capture every wire shape per rig → 09-LIVE-CAPTURES.md, then teardown** - `026c7ef` (docs)

## Files Created/Modified

- `.planning/phases/09-agent-surface-api-diagnostics/09-LIVE-CAPTURES.md` — verbatim rig-labeled captures for all six probe groups + Decisions-locked-by-captures (state vocabulary, units, license nesting, gan shape, 4xx partition)
- `.planning/phases/09-agent-surface-api-diagnostics/09-RIG-NOTES.md` — ops log: container names/IDs, ports (18188=8.3.6, 19188=8.3.3), commissioning timeline, token NAMES (never keys), launch/teardown commands, the env-loss recovery note

## Decisions Made

- Bundle state vocabulary locked from captures only: generating set = `["Generating"]`, terminal = `["Valid"]`; anything else must passthrough (unobserved)
- `uptime` = ms-since-gateway-start (proven twice against wall clock: 460959 ms and 617422 ms each land exactly on their capture moments); `lastSyncTimestamp` = -1 never-synced sentinel → model as Option, ms interpretation flagged as inference (sibling `effective.lastUpdated` is epoch-ms)
- `fileSize` = bytes, absent from status while generating (key missing, not null/0), equals Content-Length exactly when Valid
- License morning-check fields typed; array ELEMENT interiors ride passthrough (empty arrays on fresh rigs are uncapturable — recorded as known limitation)
- gan byte rates are JSON floats (`0.0`) → f64; counts stay integers
- 4xx classification: 404+HTML ⇒ unknown path; 404+empty ⇒ wrong method on real path; 401+HTML ⇒ bad auth; all refusals, read-only posture holds

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ACCEPT_EULA/GATEWAY_ADMIN_PASSWORD env vars are dead on the 8.3 image — commissioned via the commissioner wire API instead**
- **Found during:** Task 1 (commissioning wait)
- **Issue:** Both rigs sat at `needs_commissioning` (`/StatusPing` → `{"state":"RUNNING","details":"COMMISSIONING"}`) for 15+ min; the image's 732-line `docker-entrypoint.sh` (read in full) contains zero handling of those env vars (8.1-era); Phase-4's summary recorded the env but not this mechanic
- **Fix:** Extracted the commissioning flow from `/res/sys/js/commissioner/commissioner.js` and drove it with curl: `GET /bootstrap` (steps: `{"eula":"license"}`) → `POST /post-step {"id":"license","step":"eula","data":{"accept":true}}` (201) → `POST /post-step {"id":"finished","step":"finished","data":{"startGateway":true}}` (200) → poll `/StatusPing` until plain `{"state":"RUNNING"}` — fully headless, no browser
- **Files modified:** 09-RIG-NOTES.md (recipe + timeline recorded)
- **Verification:** Both rigs reached `{"state":"RUNNING"}` at 02:29:30Z; admin/password login then worked in the Phase-4 OIDC ladder
- **Committed in:** 58b7831

**2. [Rule 3 - Blocking] Provisioned token keys lost to non-persistent shell env — re-provisioned with b-suffixed names**
- **Found during:** Task 2 (first capture batch 401'd; `${#T}` = 0)
- **Issue:** Keys existed only in the session shell env, which did not persist between tool calls; keys had deliberately never been printed, so they were unrecoverable
- **Fix:** Re-ran the proven provisioning script (fresh tokens `p9tok836b`/`p9tok833b`), staged keys in `/tmp/ign-p9-rigs/tokens.env` (chmod 600, outside the repo), sourced per command batch, shredded at teardown; orphaned first-pass entries died with the rigs
- **Files modified:** 09-RIG-NOTES.md (honest ops note + lesson)
- **Verification:** authed gateway-info → 200 on both rigs with the new tokens
- **Committed in:** 026c7ef (with the Task-2 docs)

---

**Total deviations:** 2 auto-fixed (2 blocking-environment, zero architectural)
**Impact on plan:** Both fixes were required to complete the plan's own gates; neither changed scope. Bonus artifacts (headless commissioning recipe, env-persistence lesson) strengthen future live-gate runs.

## Issues Encountered

- Commissioning took ~40 min of the run (15-min stuck-wait ×2 before root-causing via container logs + entrypoint read); everything after the wire-API discovery was fast
- The plan's 405 hypothesis for DELETE-on-gateway-info was falsified by the live capture (real answer: 404 + empty body) — handled per the plan's own instruction ("the capture is the truth"), recorded in the Decisions section

## User Setup Required

None - no external service configuration required (rigs are disposable docker containers; no secrets persisted anywhere).

## Next Phase Readiness

- 09-04 (license/redundancy/gan models) can cite a captured body for every field it types — zero guessed shapes
- 09-05 (bundle models) has its state vocabulary, absent-vs-present fileSize semantics, and download headers captured from both versions
- 09-06 (CLI gates) has its live 4xx partition evidence (404-HTML / 404-empty / 401-HTML)
- No blockers; rigs torn down cleanly

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-07*

## Self-Check: PASSED

- All 3 created files verified present on disk (`09-LIVE-CAPTURES.md`, `09-RIG-NOTES.md`, this SUMMARY)
- Both task commits verified in git log (`58b7831`, `026c7ef`)
- Post-teardown docker state verified: zero `ign-p9*` containers/volumes, ports 18188/19188 free
- Plan verification passed: 8.3.3 + 8.3.6 present in all six probe sections; Decisions section covers generating/terminal sets, uptime unit, lastSyncTimestamp unit, fileSize unit, license top-level shape, gan zero-connection body; secrets sweep clean
