---
phase: 24-redundant-pair
plan: 01
subsystem: testing
tags: [ignition, redundancy, docker-compose, live-capture, gateway-network]
status: complete

requires:
  - phase: 20-config-api-client
    provides: scan-lock, scan, resources find/POST/PUT/DELETE wire shapes
  - phase: 23-apply-engine
    provides: the apply write path (files into the bind-mounted collection, lock, scan) that C-P1 mimics
provides:
  - "Disposable two-node 8.3.6 pair rig (compose project ign-pair-rig) with setup-pair.sh and capture24.sh"
  - "24-LIVE-CAPTURES.md with Captures Owed, Pair rig, and raw blocks C-P1 to C-P6 plus C-P4 snapshots"
  - "Measured answers for Assumptions A1, A2, A3, A5, A10 that 24-02 turns into decisions"
affects: [24-02, 24-05, 24-06, 24-07, 24-10]

actuals:
  tokens: 64600
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "capture-first: every block is preceded by the exact command and a driver-taken UTC time"
    - "polls run as capture24.sh subcommands through shcap so the lines land verbatim in the record"

key-files:
  created:
    - crates/ignition-core/tests/fixtures/live-rig-pair/compose.yml
    - crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh
    - crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh
    - .planning/phases/24-redundant-pair/24-LIVE-CAPTURES.md
  modified: []

key-decisions:
  - "Ping settings applied: pingRate 1000 ms, pingTimeout 1000 ms, pingMaxMissed 1200 (defaults were 1000, 300, 10)"
  - "The REST path of C-P1 uses POST create then PUT with its signature on a resource of its own, not a PUT on a pre-existing resource, so no shared resource is touched"
  - "The rig is left running and settled for 24-02; teardown is docker compose -p ign-pair-rig down -v"

requirements-completed: [DPAIR-04, DPAIR-01, DPAIR-02]

duration: 2h
completed: 2026-10-19
---

# Phase 24 Plan 01: Pair rig and live redundancy captures Summary

**A disposable two-node 8.3.6 redundant pair (gw-a Master on 18300, gw-b Backup on 18301, TLS gateway-network link, mode production, bind-mounted resource collections) and six measured captures showing that the backup timestamp advances, resource.json is not byte-equal across nodes, and a toggleApproval cut promotes the backup at once.**

## Performance

- Duration: about 2 hours (rig bring-up in the first session, Tasks 2 and 3 in this one, about 15 minutes of captures)
- Tasks: 3 of 3, commits 8b04401, c63bf5d, 1bbc2d0
- Setup step times (from `step-times.log`): up 134 s, mint 120 s, pair 150 s, mode 886 s (the last includes a false 600 s timeout, see deviations; the pair was settled by 20:32:10Z, mode definition reached gw-b in 23 s, relink 33 s after the gw-b recreate and 8 s after the gw-a recreate)
- Ping values: pingRate 1000 ms, pingTimeout 1000 ms, pingMaxMissed 1200, read back from both nodes in the Pair rig section

## What each capture showed

- C-P4 settled: gw-b `peerConnected` true, `syncPending` false, `Cold`, both `projectState` Good, both `deploymentMode` production; token A answers 200 on gw-a and 401 on gw-b, token B the reverse (A10: tokens are node-local, one per node is required).
- C-P1 file path (scan-lock, files into gw-a `production/`, config scan): gw-b `lastSyncTimestamp` advanced (1792441595954 to 1792442647207) and the resource read back on gw-b 10 s into the poll, with a second advance 4 s later; `syncPending` was never true at 2 s resolution, so there is no `### C-P4 during-sync` block and the record says so. A1 (timestamp advance on an incremental change) holds.
- C-P1 REST path (POST create, then PUT with its signature): read back on gw-b within 2 s each time, timestamp advanced each time, `syncPending` never true. A1 holds for REST writes too.
- C-P2: `config.json` is byte-equal on both nodes for every changed unit, and the untouched `core/ignition/general-alarm-settings` is byte-equal in both files. `resource.json` is NOT byte-equal for any changed unit: the backup rewrites it (key order, pretty printing, `uuid` and `lastModificationSignature` order, the single-line file written by the driver becomes multi-line). A2 as written in D-03 does not hold for `resource.json`; a comparison must use `config.json` bytes or a normalised `resource.json`. A POST of a database-connection also creates companion units (`store-and-forward-engine`, `tag-definition`) in core that sync the same way.
- C-P3: `POST /data/api/v1/redundancy/gwaction/resync` sent to gw-a answers 200 `{"resync_state":"started"}`; gw-b `lastSyncTimestamp` advanced within 2 s and the pair read settled throughout; `syncPending` was never true.
- C-P5: `toggleApproval/<id>/false` on gw-a (connection id `ign-pair-a-backup|<uuid>`, HTTP 200) dropped the link on both nodes within 2 s with both `/StatusPing` and gw-b `GET redundancy` still answering 200 for all 60 s. `toggleApproval true` plus the trust loop restored `peerConnected` and settled in 7 s. The two-network fallback was not needed by the plan's rule.
- C-P6: after that same cut gw-b reported `Active` (projectState `Out of Date`) from the first reading and stayed Active for the 120 s window while gw-a stayed Active: both nodes Active, so `active_node` would read `Both`. A3 holds for a revoked-approval cut despite pingMaxMissed 1200. Reconnect settled in 9 s.
- C-P5 fallback (extra, beyond the plan): `docker network disconnect` of gw-b from `gan` left `peerConnected` true on both nodes for 180 s and gw-b stayed Cold; on reconnect `peerConnected` was true after 1 s and the pair settled after 16 s with a fresh `lastSyncTimestamp`. A network cut, unlike the approval cut, does not promote the backup inside the window.

## For 24-02 (what these facts mean for the open decisions)

- D-06 timestamp-advance clause is supported by C-P1 and C-P3.
- D-03 needs changing: byte equality of `resource.json` fails on a healthy pair.
- D-11 versus D-17 collision is real for the toggleApproval cut: it promotes the backup at once, so a gate that cuts that way makes `active_node` read `Both`. The network-level cut keeps gw-b Cold but also keeps `peerConnected` true for minutes under these ping settings, so it does not look like a cut to the status fields.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Four script bugs fixed during Task 1** (commit 8b04401)
- jq `// empty` dropped `false`, so `syncPending=false` read empty and the final `settled` wait false-timed-out at 600 s; fixed with `if . == null then empty else tostring end`
- `$(call ...)` lost the HTTP status to the subshell; status now goes to a file reloaded by `sync_status`
- a `read` without a trailing newline aborted under `set -e`
- an EXIT trap referenced a local variable

**2. [Rule 1 - Bug] `projectState` truncated to `Out` in the first C-P6 poll** (commit 1bbc2d0)
- Found during: Task 3
- Issue: `rstate` joined fields with spaces and `read` split `Out of Date`
- Fix: spaces in `projectState` become `_` in later polls; the C-P6 block already captured is kept and carries a note, the full value is in `### C-P4 during-cut-c-p6`

### Plan departures

- No `mint-token-pair.sh`; mint is a `setup-pair.sh` subcommand, as the plan describes.
- `setup-pair.sh` reuses `../live-rig-deploy/security-levels` and `security-properties` instead of copying them.
- C-P1 REST path creates its own resource (`CAP24_REST_<epoch>`) with POST and then PUTs it with its signature, instead of a PUT on a pre-existing resource.
- No `### C-P4 during-sync` block: `syncPending` was never true in any 2 s poll of C-P1 or C-P3. The plan allows noting that it never was; the record carries that note.
- Added `cp5net` (the two-network cut with both the 2 s link series and the 5 s activity series) although the API cut dropped the link; it answers the C-P5 question about whether the fallback differs and informs D-11 versus D-17. Also added a second C-P4 snapshot `end-of-24-01-final` after the extra run so the last snapshot reflects the settled end state.
- Findings recorded in the captures file beyond the plan: A10 (node-local tokens), gw-b `gateway-info.name` reads `ign-pair-a`, ping settings applied, step durations.

## Issues Encountered

- Trial licenses on the rig lapse about 22:16Z; the rig must be used or recreated before then.
- `setup-pair.sh mode` reports a false timeout (see deviation 1); fixed in the script for the next run.

## Known Stubs

None.

## Threat Flags

None. Both drivers refuse a non-loopback base URL with exit 2 before any request (the `evil.example#@localhost` probe was run on `capture24.sh` and `setup-pair.sh`), tokens live only in `~/.cache/ign-pair-rig/tokens.env`, and the captures file holds no token value (grep for both keys prints 0).

## Rig state at hand-off

The pair is running and settled: `setup-pair.sh status` reads `gw-a Master Active true false ... Good production` and `gw-b Backup Cold true false ... Good production`. The `CAP24_*` units were removed from both nodes. Teardown when finished: `docker compose -p ign-pair-rig down -v`.

## Self-Check: PASSED

- Files found: compose.yml, setup-pair.sh, capture24.sh under `crates/ignition-core/tests/fixtures/live-rig-pair/`, and `24-LIVE-CAPTURES.md`
- Commits found: 8b04401, c63bf5d, 1bbc2d0
- `## Captures Owed`, `## Pair rig (24-01)`, `## C-P1` to `## C-P6`, `### C-P4 settled`, `after-cleanup`, `after-cut`, `after-reconnect`, `end-of-24-01` present; token grep prints 0
