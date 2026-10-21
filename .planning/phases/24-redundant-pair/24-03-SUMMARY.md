---
phase: 24-redundant-pair
plan: 03
subsystem: api
tags: [ignition, redundancy, secret-providers, deploy-api, wiremock, lint]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: captures C-P3 and C-P7 and the approved contract (K1-K9, V1-V10) from 24-02
provides:
  - "DeployApi::secret_names -> SecretNameListing {Listed(BTreeSet<String>), NoProvider, CannotList}"
  - "DeployApi::redundancy_resync (one POST, never retried)"
  - "DEPLOY_ENDPOINTS rows redundancy_resync (C-P3) and secret_names (C-P7); 16 rows"
  - "FakeGateway::{seed_secret_names, secret_provider_cannot_list, resyncs}; ForbidWrites refuses the resync; FaultyDeploy fails both by id"
  - "lint::referenced_secret_names(node, &mut BTreeSet<(String, String)>)"
affects: [24-05, 24-06, 24-07, 24-09, 24-10]

actuals:
  tokens: 36000
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "listing statuses decided before classify: 404 NoProvider, 500/501 CannotList, the rest classified"
    - "names-only collector sharing is_referenced with the secret lint"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/deploy_api.rs
    - crates/ignition-core/src/deploy/lint.rs
    - crates/ignition-core/src/deploy/apply/hold.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/common/faulty_deploy.rs
    - crates/ignition-core/tests/common/forbid_writes.rs
    - crates/ignition-core/tests/deploy_client_contract.rs

key-decisions:
  - "K5 = d: no K5 read, so DEPLOY_ENDPOINTS has 16 rows (the plan's no-K5 count); client/redundancy.rs is unchanged"
  - "secret_names sends its own request and maps 404/500/501 before client::classify; every other status is classified (401/403 -> auth_rejected, 502 -> internal)"
  - "referenced_secret_names recurses into lookalike objects, so a real reference nested inside one is still collected"

requirements-completed: []

duration: not timed
completed: 2026-10-19
---

# Phase 24 Plan 03: Secret listing, resync and the reference collector Summary

**`DeployApi` gains the names-only `secret_names` listing and the never-retried `redundancy_resync`, each a declared and capture-tied endpoint with wiremock contract tests and updated test doubles, plus `lint::referenced_secret_names`, the pure names-only collector of `Referenced` secret objects.**

## What was built

- `DEPLOY_ENDPOINTS` grew from 14 to 16 rows after `restart_pending`: `redundancy_resync` (POST `/data/api/v1/redundancy/gwaction/resync`, `EmptyOk`, `C-P3`) and `secret_names` (GET `/data/api/v1/secret-providers/{provider}/secrets`, `Json`, `C-P7`). The three unit pins (table, ids, count 16) and `wire_matches_the_endpoint_table` (now filling `{provider}` with `cap24-file`) cover both.
- `SecretNameListing`: a 200 `{"secrets":[...]}` is `Listed(BTreeSet<String>)`; 404 (C-P7 HTML) is `NoProvider`; 501 and 500 are `CannotList`; the provider name goes through `validate_segment` first (hostile names `../x`, `a/b`, empty, `a b` send zero requests). Names are exact and case-sensitive (K9): the C-P7 `CAP24-Name`/`cap24-name` pair lists as two entries.
- `redundancy_resync`: one POST through `send_shaped` with shape `EmptyOk`; either 200 body is accepted and discarded; a 500 is `internal` and wiremock sees exactly one POST.
- Doubles: `ForbidWrites` refuses `redundancy_resync` (now six write methods) and forwards `secret_names`; `FaultyDeploy` fails each by id; `FakeDeploy` serves seeded listings (an absent provider is `NoProvider`) and counts resyncs; `LockGateway` is `unused()` for both.
- `lint::referenced_secret_names`: names only, uses the lint's own `is_referenced`; duplicates collapse, the same name under two providers collects twice, output is byte-ordered, lookalikes (extra member, lower-case type, non-string name, extra `data` member, `Embedded`) add nothing.

## Facts the plan asked to record

- Final `DEPLOY_ENDPOINTS` count: 16.
- K5 branch taken: d (no K5 read, no third method).
- 501 variant produced by `classify`: none is produced. `classify` has no 501 arm; an unlisted 5xx falls to `CoreError::Internal` ("unexpected HTTP ..."). `secret_names` therefore decides 404, 500 and 501 itself, before `classify`, and never reaches the `Internal` arm for them. No other 5xx retry exists in the client (`send_and_classify` sends once), so the 500 mapping cannot be preempted.

## Deviations from Plan

### Contract overrides relayed by the orchestrator (24-02 decisions)

**1. [Override K5 = d] No K5 read added**
- The plan's conditional K5 `a`/`b` items were dropped: no `DeployApi::redundancy_providers`, no `RedundancyProviderWire`, no `REDUNDANCY_PROVIDERS_PATH`, no `seed_redundancy_providers`, no `redundancy_providers` or `redundancy_events` row. The detection reads `GatewayApi::logs` (24-05). `client/redundancy.rs` is untouched and Task 2's K5 tests do not exist.

**2. [Override K7 sub-clause] HTTP 500 is `CannotList`**
- `secret_names` maps 500 to `CannotList` as well as 501 (the plan said 501 only). Any other non-2xx stays an error (`secret_names_other_non_2xx_is_an_error`, a 502). Added `secret_names_500_is_cannot_list` with the C-P7 `remote` provider HTML body next to `secret_names_501_is_cannot_list`; the mapping runs before `classify`, and the test asserts one request.

**3. [Override K9 = a] Exact, case-sensitive names** — the listing is a `BTreeSet<String>` of the strings as listed; `secret_names_lists_the_captured_body` pins `CAP24-Name` and `cap24-name` as two names.

**4. [Override C-P7/C-P3 facts]** The 404 test uses the C-P7 HTML body, the resync test uses `{"resync_state":"started"}`, and nothing treats the resync answer as a completion signal. Provider type (`.config.profile.type`) is not read by this plan.

### Auto-fixed Issues

**5. [Rule 1 - Bug in plan text] `auth_failed` is not the code name**
- **Found during:** Task 2 (`secret_names_403_is_auth`)
- **Issue:** the plan and CONTEXT say `auth_failed`; `CoreError::Auth` has the code `auth_rejected`.
- **Fix:** the test asserts `auth_rejected` plus the status and that no page text appears in the error.
- **Files modified:** `crates/ignition-core/tests/deploy_client_contract.rs`
- **Commit:** e38deaa

### Plan departures

- Tasks 1 and 2 were committed together (e38deaa): writing the Task 1 contract tests first already covered every Task 2 behaviour, so splitting the contract file would have been artificial. The tracer's `<verify>` (`secret_names_lists_the_captured_body`, `redundancy_resync_posts_once`, `wire_matches_the_endpoint_table`) passed before I went on; I continued past the interactive tracer checkpoint because the caller allowed stopping but did not require it and the verification is fully automated.
- Task 2 asks for 6 capture-id comments in the contract file; the new tests each carry a C-P3/C-P7 comment (the count criterion passes).
- A test I added beyond the plan: `a_lookalike_does_not_hide_a_reference_inside_it` pins the recursion choice.

## Verification

- `cargo test -p ignition-core --lib client::deploy_api` (6 passed), `--lib deploy::lint` (32 passed), `--test deploy_client_contract` (75 passed), `cargo test -p ignition-core --no-run` and `cargo clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p ignition-core --no-fail-fast`: every test binary reports ok (27 binaries with tests, 0 failed).

## Issues Encountered

- One `cargo test -p ignition-core` run reported `live_gateway` as failed while a workspace clippy build ran at the same time; the test (`guard_drop_during_unwind_inside_runtime_still_cleans_up`, the only non-ignored one) passed on rerun alone and in the `--no-fail-fast` run. Not touched by this plan; treated as load-sensitive flakiness.

## Known Stubs

None.

## Threat Flags

None. T-24-08 (hostile provider names send zero requests), T-24-09 (names-only type, no value field), T-24-10 (one POST, never retried, `ForbidWrites` refuses it) and T-24-11 (401/403 classified to `auth_rejected` with the status only) are covered by tests.

## Requirements

DPAIR-01 and DPAIR-03 are not marked complete: this plan ships the client calls and the collector only; the phase completes in 24-10.
