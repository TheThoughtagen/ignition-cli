---
phase: 20-config-api-client
plan: 01
subsystem: testing
tags: [ignition-8.3.6, live-captures, scan-lock, config-resources, openapi, docker-rig]

requires: []
provides:
  - 20-LIVE-CAPTURES.md: verbatim 8.3.6 wire truth (C1a-C1j, C-null x2, C-mode, C3a, C-list, carried C-S3), Decisions D1-D12, 13-row Endpoint shapes table, assumptions A1-A9
  - Reusable loopback-only rig fixture (compose project ign-deploy-rig, 127.0.0.1:18290), token minting and a capture driver with a localhost-only guard
  - Trimmed live 8.3.6 OpenAPI extract
affects: [20-02, 20-03, 20-04, 20-05, 20-06]

actuals:
  tokens: 155000
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Capture-first: wire truth recorded verbatim (curl -i, UTC stamped) before any dependent code"
    - "Disposable rig bound to 127.0.0.1 only, secrets only in a 0600 state file outside the repo"

key-files:
  created:
    - .planning/phases/20-config-api-client/20-LIVE-CAPTURES.md
    - .planning/phases/20-config-api-client/20-openapi-extract.json
    - crates/ignition-core/tests/fixtures/live-rig-deploy/compose.yml
    - crates/ignition-core/tests/fixtures/live-rig-deploy/compose.bind.yml
    - crates/ignition-core/tests/fixtures/live-rig-deploy/mint-token.sh
    - crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh
    - crates/ignition-core/tests/fixtures/live-rig-deploy/db-connection-config.json
    - crates/ignition-core/tests/fixtures/live-rig-deploy/security-levels/
    - crates/ignition-core/tests/fixtures/live-rig-deploy/security-properties/
  modified: []

key-decisions:
  - "D1: competing acquire is an immediate 409 with an HTML body naming the actor; 408 never observed"
  - "D3: stale signature is a 500 (marker 'signature mismatch'); omitted and empty signatures are one identical 422 (fieldName 'signature')"
  - "D7: list is the effective view only and filters with object-style field[op]=value, not filter=; list-by-collection is names then find?collection="
  - "D11: bind-mounted resources/ is viable (A7 holds)"
  - "Endpoint shapes: all defaults kept (EmptyOk for scan_lock_acquire, scan_request, mode_create; JsonOrNoContent for scan_lock_info; NoContent unused)"

requirements-completed: [DAPI-06, DAPI-07]

duration: 35min
completed: 2026-10-12
status: complete
---

# Phase 20 Plan 01: Live Captures Summary

**Verbatim 8.3.6 wire truth for every Phase 20 endpoint, recorded on a loopback-only disposable rig, with Decisions D1-D12 and a 13-row Endpoint shapes table that 20-03, 20-04 and 20-05 cite by capture id.**

## Accomplishments

- Block C-S3 (S3.md lines 6-7 and 40-41, byte-for-byte, with source path) written first so 20-02 can cite it. First added in commit `7da5730`.
- Rig fixture `crates/ignition-core/tests/fixtures/live-rig-deploy/` (single service, `127.0.0.1:18290`, project `ign-deploy-rig`), `mint-token.sh` (cap-a, cap-b with a custom `Authenticated > Deploy` level, keys only in a 0600 state file outside the repo) and `capture.sh` (`assert_local` exits 2 before any request or file write).
- Every capture id recorded: C1a-C1j, C-null (no mode and mode production), C-mode, C3a, C-list. 24 `##` sections in `20-LIVE-CAPTURES.md`.
- Trimmed OpenAPI extract (scan, scan-lock, mode, gateway-info and the database-connection and security-properties resource routes) from the live 584-path document.
- Rig torn down with `down -v`: no `ign-deploy-rig` container or volume remains; tokens.env and the bind directory were deleted; no token key appears under `.planning/` or `crates/`.

## Decisions that overturned a research or OpenAPI assumption

20-03, 20-04 and 20-05 branch on these. Details and block citations are in `20-LIVE-CAPTURES.md` `## Decisions`.

- **D1:** a competing acquire is an immediate 409 (0.016 s with `acquireTimeout` 2), an HTML Jetty page, not JSON; 408 never appeared. The actor (`api-token:<name>:<ip>`) must be scraped from `Lock already held by ...`.
- **D2:** the same token re-acquiring is also 409 and does not extend the hold.
- **D3:** stale is a 500 JSON `problem` containing `signature mismatch`; omitted and empty signatures are one byte-identical 422 with `fieldName: "signature"` (not wire-distinguishable from each other).
- **D4:** a no-op scan advances `lastScanTimestamp` (A8 holds, about 1 s). The POST body is a trigger-time snapshot (`scanActive:true`, old timestamp). A scan by ANY actor, holder or not, releases the lock at trigger time.
- **D7:** A4 is falsified. `filter=field[op]=v` matches nothing; the object-style `field[op]=v` works but `list` sees only the effective view (a shadowed `core` copy cannot be listed). List-by-collection is `names` (`modes`) then `find?collection=`.
- **D9:** a body-less lock POST is a 500 NPE, not the declared 400. Acquire 200 returns the same JSON as the info read.
- **D8:** no scan-lock release route (DELETE and POST `/release` are 404); the live OpenAPI has 584 paths, not 737 or 617.
- Research assumptions A1-A3, A5-A9 hold (A1 refined to 409 only); only A4 is falsified. A3 resolved as `""`, never `null`, on 8.3.6.

Also recorded as requested:

- **D11 bind-mount verdict:** viable. First boot RUNNING with `core/` populated on the host, RUNNING again after the token mint restart and after the production-mode recreate.
- **D4 no-op-scan answer:** yes, a scan on an idle gateway advances `lastScanTimestamp` (1791815654089 to 1791815676996).
- **C-S3 first added in commit `7da5730`** (20-02 Task 2 cites it by block id).

## Task Commits

1. Carried block C-S3 (early, rig-independent) - `7da5730` (docs)
2. Task 1 fixture and capture driver - `8557ef5` (test); C1, C-null, C-mode, C3a captures - `69eca52` (docs)
3. Task 2 driver tweaks (stamp before request, CR strip, C-list probes) - `62469c7` (test); complete captures, Decisions, shapes, OpenAPI extract - `e415f2c` (docs)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Extra C-list probes for the list filter (D7)**
- **Found during:** Task 2 (C-list)
- **Issue:** the plan's `filter=collection[eq]=production` returned `matching:0` even for a control filter on `name`, so the plan's matrix could not answer D7. Reading the live OpenAPI showed `filter` is a form-style exploded object.
- **Fix:** added `clist_extra` (the undeclared-parameter and `filter=` probes) and `clist_extra2` (object-style `field[op]=value`) to `capture.sh`, appended under the single `## C-list` heading.
- **Files modified:** `crates/ignition-core/tests/fixtures/live-rig-deploy/capture.sh`, `20-LIVE-CAPTURES.md`
- **Commits:** `62469c7`, `e415f2c`

**2. [Rule 1 - Bug] capture.sh stamped the request time after the response**
- **Found during:** Task 1 review of the C1 blocks (the `@` stamp lagged the `Date` header by up to 2 s)
- **Fix:** later groups stamp before the request and strip carriage returns; the C1 blocks keep the original stamps and the Drift section says so (the `Date` header is authoritative).
- **Commit:** `62469c7`

**3. Sub-labels use `###` instead of per-request `##` headings.** `cap` writes a `### LABEL` sub-heading under one `## <capture id>` heading, because Task 2's acceptance requires exactly one `##` heading each for C3a, C-mode, C-list and the two C-null states.

The commits for Task 1 and Task 2 are not split exactly along the task boundary: the no-mode groups (cnull, cmode, c3a) were run during the same rig session as C1 and committed with it in `69eca52`.

### Carried-forward notes for downstream plans

- The `@ <UTC>` stamps in C1 blocks are post-response times.
- `scan-lock/projects` and `scan/projects` were not live-captured (wiremock-pinned in 20-03), as the plan states.

## Auth gates

None.

## Verification

- `bash -n` on both scripts; `capture.sh` against `http://10.4.8.15:8088` and against `http://localhost@10.4.8.15:8088` exits 2 and leaves the captures file's line count unchanged.
- Task 1 and Task 2 acceptance greps: 10 C1 headings, 5 C-null/C-mode/C3a/C-list headings, 12 Decision headings, 13 Endpoint shapes rows, Carried evidence / Assumptions / Drift sections present, C-S3 byte-identical to the spike lines, `jq` finds `scan-lock` in the extract, tokens.env was 0600, key-absence loop found nothing before deletion.
- All five CI gates green locally: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace`, `cargo build -p ignition-cli --no-default-features`. No Rust code changed.
- Teardown: `docker ps -aq --filter label=com.docker.compose.project=ign-deploy-rig` and `docker volume ls -q --filter name=ign-deploy-rig` both empty. The container `ign-jvm-live-test` on 19088 was not touched.

## Known Stubs

None.

## Threat Flags

None. The only new surface is a loopback-bound disposable rig and its scripts, covered by T-20-01 to T-20-05 (`assert_local`, 0600 state file, 127.0.0.1 binding, `down -v`).

## Self-Check: PASSED

All created files exist, all five commit hashes exist, and no `ign-deploy-rig` container or volume remains.
