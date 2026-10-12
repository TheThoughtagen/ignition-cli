---
phase: 20-config-api-client
plan: 03
subsystem: api
tags: [scan-lock, scan, deploy-api, wiremock, classify, poll, ignition-8.3.6]

requires:
  - phase: 20-01
    provides: "20-LIVE-CAPTURES.md blocks C1a-C1j, decisions D1/D2/D4/D8/D9/D12 and the Endpoint shapes table"
  - phase: 20-02
    provides: "CoreError::ScanLockHeld (slug scan_lock_held, exit 6)"
provides:
  - "DeployApi trait (scan_lock_acquire, scan_lock_info, scan_status, scan_request), dyn-compatible, separate from GatewayApi"
  - "client/scan.rs: ScanKind, LockRequest, ScanLockInfo, LockObservation, LockVerdict, lock_verdict, ScanStatus, scan_complete, ScanOutcome (pure, no clock)"
  - "scan_and_wait: waits out a foreign scan, one trigger POST, Complete only on inactive plus advanced timestamp, NotComplete on a 200 that never advances"
  - "Route-scoped 409/408 scan-lock arm in classify.rs with parse_lock_holder (actor scraped from the Jetty page)"
  - "ResponseShape, EndpointDecl, DEPLOY_ENDPOINTS (4 rows), endpoint_decl, send_shaped"
affects: [20-04, 20-05, 21, 22, 23, 24, 25]

tech-stack:
  added: []
  patterns:
    - "Declared response shape per endpoint, enforced in one stateless send_shaped"
    - "Pure verdict functions with no clock; structural include_str! pin over the source (scan_rs_reads_no_clock)"
    - "poll::poll probe state carries every borrow (WaitCtx) so the boxed probe future is bounded by the poll call"
    - "One-shot enrichment of a refusal from a follow-up read, never a POST retry"

key-files:
  created:
    - crates/ignition-core/src/client/scan.rs
    - crates/ignition-core/src/client/deploy_api.rs
    - crates/ignition-core/tests/deploy_client_contract.rs
  modified:
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/client/mod.rs

key-decisions:
  - "D1 made 409 the primary contract pin (capture-proven, HTML body naming the actor); 408 is mapped by the same arm and pinned through the enrichment test (empty body, actor from one GET)"
  - "D4 recorded that a non-holder's scan POST is accepted (200) while a lock is held, so no scan-route classify arm was added; scan_by_other_actor_while_lock_held pins the 200 and the 204 lock read that follows"
  - "ScanStatus requires scanActive and lastScanTimestamp (no serde defaults): a reading without them is a loud parse error, never a default that reads as complete"
  - "scan_lock_acquire timeout is a per-request acquire seconds + ACQUIRE_TIMEOUT_SLACK (10 s); neither POST is ever retried, refusals included"

patterns-established:
  - "Capture-pinned contract tests: bodies copied verbatim from 20-LIVE-CAPTURES.md, each test cites its block"
  - "Mutation check on the completion predicate: dropping the timestamp comparison turns two scan_ tests red"

requirements-completed: [DAPI-03, DAPI-04]

duration: 50min
completed: 2026-10-12
status: complete
actuals:
  tokens: 14800
  tasks: 2
  commits: 2
---

# Phase 20 Plan 03: Scan-lock and scan client Summary

**A `DeployApi` trait over the real reqwest pipeline: a held scan-lock comes back as exit-6 `scan_lock_held` naming the holder scraped from the 8.3.6 Jetty page, and `scan_and_wait` reports a scan complete only when `scanActive` is false and `lastScanTimestamp` has advanced on the same gateway.**

## Task Commits

1. **Task 1 (tracer): held lock returns `scan_lock_held` naming the holder, end to end** - `dec3935` (feat)
2. **Task 2: scan completion by `scanActive` plus an advanced `lastScanTimestamp`** - `69202a7` (feat)

## Accomplishments

- Tracer slice through every layer this phase touches: `scan.rs` models, `DeployApi` and `send_shaped`, the `client/mod.rs` seam, the route-scoped classify arm, the 20-02 error, and wiremock serving bytes copied from C1b/C1c/C1e. All driven through `&dyn DeployApi`.
- `lock_verdict` takes no clock and never reads `timestamp`: a 512-case proptest over arbitrary i64 timestamps and the `scan_rs_reads_no_clock` structural pin both hold. `remaining >= margin` is inclusive.
- `scan_lock_info` decides Free from a 204 status before any body read; a 200 with an empty body is an `internal` error naming `scan_lock_info`, never Free.
- `scan_and_wait`: a scan already active at the first read is waited out before the trigger POST (its completion is never counted as ours); one trigger POST; poll to inactive plus advanced; budget exhaustion is `Ok(NotComplete { last, waited })`, not an error. Auth failure fails fast and never reaches the POST.
- The acquire carries a per-request timeout of `acquireTimeout + 10 s` (`acquire_request_timeout`, unit-tested), the full `name:key` token, and the exact body `{"acquireTimeout":10,"holdTimeout":5}` (field order is wire order).
- `GatewayApi` and its doubles are untouched; `crates/ignition-core/src/deploy/` does not exist.

## Final DEPLOY_ENDPOINTS rows

| id | method | path template | shape | capture_ref |
| --- | --- | --- | --- | --- |
| scan_lock_acquire | POST | `/data/api/v1/scan-lock/{kind}` | EmptyOk | C1b |
| scan_lock_info | GET | `/data/api/v1/scan-lock/{kind}` | JsonOrNoContent | C1c |
| scan_status | GET | `/data/api/v1/scan/{kind}` | Json | C1i |
| scan_request | POST | `/data/api/v1/scan/{kind}` | EmptyOk | C1i |

All four match the 20-LIVE-CAPTURES Endpoint shapes table (`endpoint_rows_match_the_capture_table`).

## Decisions Made

- **D1 primary pin: 409.** The classify arm maps `S::CONFLICT | S::REQUEST_TIMEOUT` on `/data/api/v1/scan-lock/`. Only 409 is capture-proven; 408 is OpenAPI-declared and exercised through `lock_held_without_actor_is_enriched_from_lock_info`.
- **D4 added no scan-route classify arm.** A non-holder's scan answered 200 and ran while the lock was held; the trigger POST is accepted, its body ignored.
- **A-04a holds:** D4 confirmed a no-op scan advances `lastScanTimestamp`, so the DAPI-04 predicate stands unchanged and Task 2 did not halt.
- **D12 consequence recorded in the `LockVerdict::NotHeld` doc:** a 204 is also the normal state after a successful scan trigger, which is why the verdict is named `NotHeld`, not `Expired`.
- **A-07d / assumption delta:** the holder stays the gateway's opaque `actor` string; the client models no identity of its own.

## Deviations from Plan

**1. [Process - tracer gate] Interactive tracer checkpoint replaced by an automated re-run**
- **Found during:** Task 1 completion
- **Issue:** the plan's tracer gate, in a non-auto run, asks for a `checkpoint:human-verify` before expansion. Auto mode is off (`auto_advance` false), but `.planning/config.json` has `human_verify_mode: "end-of-phase"` and the orchestrator asked for the whole plan to run to a SUMMARY in one sequential pass.
- **Fix:** re-ran the tracer's `<verify>` (the 8 `lock_` tests plus the classify unit tests) green before any expansion, then continued. The human verification of the slice is left to the end-of-phase gate.
- **Commit:** `dec3935`

**2. [Rule 1 - Bug] Probe future lifetime rejected by the compiler**
- **Found during:** Task 2 (`scan_and_wait`)
- **Issue:** with poll state `()`, the boxed probe future had to be `'static`, so borrowing `api` and the pre-trigger reading failed (E0597).
- **Fix:** the borrows ride the poll state (`WaitCtx`), the pattern `bundle_wait` already uses with its Mutex state.
- **Files modified:** `client/deploy_api.rs`
- **Commit:** `69202a7`

**3. [Plan wording] `parse_lock_holder` has no JSON branch**
- The plan said to read a JSON field "where D1 shows it". D1 shows only the Jetty HTML page, so only that marker is parsed. An unrecognised body gives `None` and the caller enriches from one `scan_lock_info`.

**4. [Stricter than plan] `ScanStatus` decisive fields are required**
- The plan said "lenient". Unknown keys still ride in `extra`, but `scanActive` and `lastScanTimestamp` have no serde default, so a body missing either is an `internal` parse error rather than a default that could read as complete.

**5. [TDD note] RED not observed as a failing run**
- Contract tests were written before the implementation, but the first compile of the pair was already green. To compensate, the completion predicate was mutation-checked: removing the timestamp comparison turned `scan_trigger_200_but_status_never_advances_is_not_complete` and `scan_foreign_scan_active_at_before_is_not_counted` red; restored afterwards.

## Behaviour change to be aware of

The scan-lock arm is route-scoped in `classify`, so it also applies to the raw `ign api call` pipeline: a 409/408 on `/data/api/v1/scan-lock/*` there now maps to `scan_lock_held` (exit 6) instead of `gateway_client_error` (exit 2). `api_classify_contract`, `sessions_contract` and `eam_contract` pass unedited.

## Verification

- All five CI gates green locally: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace` (71 test binaries ok, 0 failed), `cargo build -p ignition-cli --no-default-features`.
- Plan-scoped: `deploy_client_contract` 14 passed; `client::scan` 6 passed (including `lock_verdict_ignores_timestamp`, `lock_verdict_boundary_is_inclusive`, `scan_rs_reads_no_clock`); `client::classify` 14 passed.
- Acceptance greps: `GatewayApi` has no scan-lock items; zero `expose()` outside comments in `deploy_api.rs` and `scan.rs`; `apply_auth` appears 3 times in `deploy_api.rs`; `test ! -d src/deploy`; no `preserve_order` or `arbitrary_precision`.

## Requirements

- **DAPI-03 and DAPI-04 marked complete:** their full text is satisfied by code that now exists (acquire, remaining time, expiry via 204 without a clock; trigger plus `scanActive`/advanced-timestamp completion).
- **DAPI-06** was already complete from 20-01 and is unchanged.
- **DAPI-07 left Pending:** the held-lock half (`scan_lock_held` naming the holder) now exists, and `active_node` exists from 20-02, but the traceability row records the CLI surface deferred to Phase 24 by user decision, so it was not flipped here.

## Known Stubs

None.

## Threat Flags

None. No new endpoint or auth path beyond the planned scan-lock and scan routes; T-20-10 to T-20-15 are mitigated as planned (single `apply_auth` site, `scan_complete` predicate, no POST retry, 256-char holder cap, clock-free verdict, acquire timeout slack).

## Self-Check: PASSED

Commits `dec3935` and `69202a7` exist; `scan.rs`, `deploy_api.rs` and `deploy_client_contract.rs` exist; no TODO, skip or ignore markers; no co-author trailer on either commit.
