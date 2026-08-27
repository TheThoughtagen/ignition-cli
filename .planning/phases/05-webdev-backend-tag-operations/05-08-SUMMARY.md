---
phase: 05-webdev-backend-tag-operations
plan: 08
subsystem: api
tags: [webdev, alarms, uuid, prefix-expansion, traceback, wiremock, ignition]

# Dependency graph
requires:
  - phase: 05-07
    provides: live rig healthy at 1.0.0 + staged token (trial clock), the e2e/live-gate precedents
provides:
  - the alarms view→ack loop closed both directions: human table prints FULL event ids (copy-paste-verbatim), ack expands short prefixes against the active list (ambiguous/unknown refuse invalid_input exit 2 naming candidates/miss)
  - route traceback surfacing: a WebDev denial carrying error.traceback appends "\nroute traceback: {tb}" to the webdev_route_error message (absent traceback byte-identical)
  - wire-level proof conventions for id normalization (recorded acknowledge body carries the EXPANDED uuid)
affects: [UAT re-test (Gap 2 pin), phase-6 TUI (alarm surfaces), agents keying on webdev_route_error messages]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "id normalization at the action layer: full-shape passthrough (len==36 + hyphens at 8/13/18/23 — shape check only, no uuid dep), prefix expansion via the sibling list action's own precondition"
    - "denial enrichment threading: optional envelope fields flow RouteBody::Denied → RouteProbe::Denied → CoreError message string (variant untouched, taxonomy frozen)"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-core/src/client/webdev.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/webdev.rs
    - crates/ignition-cli/tests/contract_tags.rs
    - crates/ignition-core/tests/tags_contract.rs
    - crates/ignition-core/tests/webdev_contract.rs
    - README.md

key-decisions:
  - "BOTH ends of the loop (planner-locked): table prints the FULL UUID AND ack accepts short prefixes — the loop works from either copy-paste habit"
  - "Full-UUID detection is a SHAPE check (len==36, hyphens at 8/13/18/23) — deliberately no uuid crate; the route stays the authority on id validity (invalid-but-full-shaped ids surface the route traceback live-proven)"
  - "Prefix expansion calls tags_alarms_active (its own precondition) — one extra probe round trip only when a short id is present; all-full input takes zero extra round trips"
  - "Refusal shapes: ambiguous prefix lists the FULL candidate ids; unknown prefix names the miss + points at `tags alarms active --json` — both invalid_input exit 2"
  - "Traceback rides the existing WebdevRouteError message String — no error.rs edit, slug/exit/taxonomy frozen; absent traceback renders byte-identical (no-traceback goldens moved zero bytes)"
  - "RouteProbe::Denied grew traceback: Option<String> so the precondition's direct construction threads it identically to denial_to_error"

patterns-established:
  - "Print-what-ack-accepts: human tables that feed verb inputs print the canonical wire form"
  - "Honest expansion refusals: candidates-or-miss naming at exit 2, never silent guessing"

# Metrics
duration: 60min
completed: 2026-08-27
---

# Phase 5 Plan 08: Gap Closure — Alarms View→Ack Loop Summary

**`tags alarms active` now prints FULL event ids that ack verbatim (with short-prefix expansion + honest exit-2 refusals), and WebDev route denials surface their Python tracebacks — UAT Gap 2 closed, live-proven on both directions.**

## Performance

- **Duration:** 60 min
- **Started:** 2026-08-27T01:31:01Z
- **Completed:** 2026-08-27T02:31:16Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- **The view→ack seam fixed both ends**: the human table prints the 36-char UUID verbatim (copy-paste straight into `tags alarms ack`), and ack still accepts the 8-char-prefix habit via prefix expansion against the active-alarm list — mixed short/full ids expand independently, the wire call always carries full UUIDs (recorded-request proof).
- **Honest refusals**: ambiguous prefixes exit 2 naming the full candidate ids; unknown prefixes exit 2 naming the miss + the already-acked hint + where full ids ride.
- **The black box opened**: `error.traceback` from the route envelope now appends to the `webdev_route_error` message — the "Invalid UUID string" failure class that hid behind a generic denial in UAT is diagnosable from CLI output alone (live-proven with a `NumberFormatException` traceback).
- **Live proof on a refreshed rig**: trial had expired mid-gap → `rig reset --yes`, headless token re-provisioned via the 04-VERIFICATION recipe (`uattok` at the staged path), routes redeployed healthy at 1.0.0 — then the exact UAT-failure loop ran green.

## Task Commits

Each task was committed atomically:

1. **Task 1: View→ack loop — full event_id in the table + short-id prefix expansion in ack** — `0ff9302` (feat)
2. **Task 2: Surface the route traceback in webdev_route_error** — `b1932e8` (feat)

## Live Ack-Loop Transcript (the UAT Gap 2 pin)

```
$ ign tags alarms active
[profile: uat]
eventId                                source                                       state                    priority name
b9b0ba84-bf04-408c-97ac-084b3d70a68e   prov:p5gap2:/tag:AlarmFull:/alm:HighLimit    Active, Unacknowledged   High     HighLimit
606e4aab-59ff-46fe-87a7-812c2fb0a08c   prov:p5gap2:/tag:AlarmPrefix:/alm:HighLimit  Active, Unacknowledged   High     HighLimit

$ ign tags alarms ack b9b0ba84-bf04-408c-97ac-084b3d70a68e --username uat   # FULL printed id, verbatim
[profile: uat]
acknowledged 1 alarm(s)          # exit 0

$ ign tags alarms ack 606e4aab --username uat                              # the 8-char PREFIX habit
[profile: uat]
acknowledged 1 alarm(s)          # exit 0 — expanded against the active list

$ ign tags alarms active          # both rows now: Active, Acknowledged

$ ign tags alarms ack deadbeef --username uat --compact
{"ok":false,...,"error":{"code":"invalid_input","message":"...no active alarm's eventId starts with `deadbeef` — it may already be acknowledged or cleared; full ids ride `tags alarms active --json`"}}   # exit 2

$ ign tags alarms ack zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz --username uat --compact   # full-shaped INVALID uuid
{"ok":false,...,"error":{"code":"webdev_route_error","message":"...alarms route error\nroute traceback: Traceback (most recent call last):\n ... NumberFormatException: java.lang.NumberFormatException: Error at index 0 in: \"zzzzzzzz\"\n"}}   # exit 6 — the former black box, now visible
```

## Files Created/Modified
- `crates/ignition-cli/src/render.rs` — active table prints the full event_id (38-wide column; --json untouched)
- `crates/ignition-core/src/actions/tags.rs` — `is_full_uuid_shape` + `normalize_ack_ids` (passthrough/expand/refuse) ahead of the acknowledge wire call
- `crates/ignition-core/src/client/webdev.rs` — `parse_route_body` extracts `error.traceback`; `denial_to_error(..., traceback: Option<&str>, ..)` appends `\nroute traceback: {tb}`
- `crates/ignition-core/src/client/mod.rs` — route_call + probe thread the traceback through
- `crates/ignition-core/src/actions/webdev.rs` — precondition's direct `WebdevRouteError` construction threads the traceback identically
- `crates/ignition-cli/tests/contract_tags.rs` — moved active/ack goldens to full ids; NEW wire-level expansion golden + refusal goldens + traceback golden
- `crates/ignition-core/tests/tags_contract.rs` — ack fixture upgraded to full-UUID ids (no-lookup pin intact)
- `crates/ignition-core/tests/webdev_contract.rs` — probe Denied initializer carries `traceback: None`
- `README.md` — active row documents the full-uuid column + copy-paste contract; ack row documents id normalization

## Decisions Made
See key-decisions above — the notable one: full-UUID detection stays a shape check with no uuid dependency, so invalid-but-full-shaped ids still reach the route and now surface its traceback (live-proven) instead of a client-side guess.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Upgraded the ignition-core client-seam ack test to full-UUID ids**
- **Found during:** Task 1
- **Issue:** `tags_contract.rs::alarms_ack_pins_three_arg_body_and_remainder` used short ids ("e-1"/"e-2") — with normalization, short ids trigger the active lookup and the body_json-pinned mock no longer matches (intended behavior change breaking an outdated fixture, not in the plan's files list)
- **Fix:** fixture ids upgraded to full-UUID shape; the test's one-alarms-request pin now doubles as the no-lookup passthrough proof
- **Files modified:** crates/ignition-core/tests/tags_contract.rs
- **Verification:** suite green
- **Committed in:** 0ff9302 (Task 1 commit)

**2. [Rule 1 - Bug] Removed a duplicate probe mount that dead-ended wiremock verification**
- **Found during:** Task 1
- **Issue:** my new wiremock tests mounted `mount_tags_probe` explicitly beside `mount_alarms_action` — which mounts its own probe internally; the first-mounted probe absorbed both precondition hits, the duplicate never matched, and server-drop verification failed ("Mock #1 … 0 matched")
- **Fix:** dropped the redundant mount in both new tests (mount_alarms_action's contract includes the probe)
- **Files modified:** crates/ignition-cli/tests/contract_tags.rs
- **Verification:** all contract_tags tests green
- **Committed in:** 0ff9302 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking fixture upgrade, 1 test-harness bug)
**Impact on plan:** Both were mechanical consequences of the intended behavior change. No scope creep.

## Issues Encountered
- **Trial expiry mid-gap (expected, handled per plan context):** the rig's trial hit 0 before execution started → `rig reset --yes`, token re-provisioned via the documented 04-VERIFICATION recipe, routes redeployed healthy at 1.0.0 (~118 min clock at spot-check time). Recorded as normal flow, not a deviation.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- UAT Gap 2 CLOSED: `alarms active` prints ackable ids, ack accepts both forms with honest refusals, tracebacks visible — the phase's 2 gaps are now both closed (05-07 Gap 1, 05-08 Gap 2), ready for UAT re-test (10/12 → target 12/12) and phase transition.
- Rig state: ignition-devops UP, fresh trial (~2h from 02:05Z), token `uattok:…` at `/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/opencode/token.txt`, routes healthy at 1.0.0, fixture provider cleaned up.

---
*Phase: 05-webdev-backend-tag-operations*
*Completed: 2026-08-27*

## Self-Check: PASSED
