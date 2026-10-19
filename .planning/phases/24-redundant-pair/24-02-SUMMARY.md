---
phase: 24-redundant-pair
plan: 02
subsystem: testing
tags: [ignition, redundancy, secret-providers, live-capture, contract-decision]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: the 24-01 pair rig, setup-pair.sh, capture24.sh, captures C-P1..C-P6
provides:
  - "C-P7 secret-name listings on both nodes and C-P8 sync-exhausted attempt, names only"
  - "## Decisions (C-P1..C-P8) and ## Contract proposal (24-02) with 19 items"
  - "## Contract decisions (24-02): the user's recorded answer, all items at their Recommended option"
  - "A decided COVERAGE.md (K5 = d, both redundancy read rows opted out with reasons)"
affects: [24-03, 24-04, 24-05, 24-06, 24-07, 24-08, 24-09, 24-10]

actuals:
  tokens: 33000
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "capture-first: every claim in the decisions quotes a capture block"
    - "contract checkpoint: one table, one verbatim answer, later plans assert it as a precondition"

key-files:
  created: []
  modified:
    - crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh
    - crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh
    - .planning/phases/24-redundant-pair/24-LIVE-CAPTURES.md
    - .planning/phases/24-redundant-pair/COVERAGE.md

key-decisions:
  - "K1=a, K2=b (resource.json as a parsed JSON document, no member excluded), K3=b (cut at the engine's lock_acquired), K4=a (toggleApproval), K5=d (log matcher, unit-tested only), K6=a, K7=a plus HTTP 500 counts as CannotList, K8=a, K9=a, V1-V10 as written"
  - "No item differs from its Recommended value; the user answered 'Approve all (Recommended)' on 2026-10-19"
  - "Later PLAN.md files 24-03..24-10 are not edited; the orchestrator relays the overrides below in each executor prompt"

requirements-completed: []

duration: not timed (commits 2026-10-19 15:31 and 15:48 local, plus this continuation)
completed: 2026-10-19
---

# Phase 24 Plan 02: Secret listings, sync-exhausted attempt and the Phase 24 contract Summary

**Names-only secret listings recorded on both nodes of the live pair, the sync-exhausted state (TEC-5105) not reachable in a 21-minute box, eight captures turned into decisions, and a 19-item contract (K1-K9, V1-V10) approved by the user with every item at its Recommended option.**

## What the captures showed

- C-P7: a fresh rig has no secret-provider resources; the names `default`, `internal` and the unknown `nope-24` answer 404 (HTML body) on both nodes. A `file` and an `internal` provider list `200 {"secrets":[]}` on both nodes, and the file provider lists the names created on gw-a. A `remote` provider answers 500 (HTML body) on both nodes; no provider answered 501. No token and a wrong key answer 401; a token without the Deploy permission answers 403. A name created on gw-a reaches gw-b in 0 to 5 s. `CAP24-Name` and `cap24-name` list as two distinct names, so the case is kept. A10 is wrong: tokens are node-local (token A is 401 on gw-b, token B 401 on gw-a), so a pair needs one token per node.
- C-P8 (TEC-5105 case): gw-b without a deployment mode against a `production` master produced one `Active deployment mode differs from peer node!` warning and then 19 minutes of `Cold`, `Good`, `peerConnected` true, `syncPending` false. The `did not match remote system snapshot` and `exceeds the max attempt count` lines never appeared, no redundancy event or provider `lastSyncError` appeared, and no resync was sent. The record states `not reachable on the rig` with the attempts listed (D-18 allows this). The pair was restored and a `### C-P4 after-exhaust` snapshot shows it settled.

## Contract decisions (user answer 2026-10-19: "Approve all (Recommended)")

| Item | Chosen | Note |
| --- | --- | --- |
| K1 | a | `SyncPredicate::TimestampAdvanced` |
| K2 | b | one `FILE_NORMALIZATION` row: `resource.json` as a parsed JSON document, no member excluded |
| K3 | b | gate cuts the link from an `ApplyObserver` at `lock_acquired`; D-11 unchanged |
| K4 | a | `PUT toggleApproval/<id>/false` on gw-a |
| K5 | d | log matcher, unit-tested only; `COVERAGE.md` unchanged (providers and events already OPT-OUT with reason) |
| K6 | a | one resync, no second budget; never a master rollback (TEC-5105) |
| K7 | a + sub-clause | 500 counts as CannotList alongside 501; any other non-2xx is an error |
| K8 | a | skip a 404 render-declared provider in the preflight with a named detail; verify still requires it |
| K9 | a | exact, case-sensitive names |
| V1-V10 | as written | V1 is the one-way door: `applied_backup_unsynced`, `deploy_backup_unsynced`, exit 6 |

No chosen option differs from its Recommended value.

## Later-plan overrides

The decided contract overrides the literal text of these later plans. Their PLAN.md files are unchanged by user choice; the executor of each plan must apply the override.

- **24-03** (REST client):
  - K5 = d, so no K5 read is added: no `DeployApi::redundancy_providers`, no `RedundancyProviderWire`, no `REDUNDANCY_PROVIDERS_PATH`, no `seed_redundancy_providers`, no `redundancy_providers` or `redundancy_events` row in `DEPLOY_ENDPOINTS`. The detection reads `GatewayApi::logs` (24-05).
  - K7 sub-clause: `secret_names` maps HTTP 500 to `SecretNameListing::CannotList` as well as 501 (the plan text says 501 only). Any other non-2xx stays an error. Add a contract test for the 500 case with the C-P7 `remote` provider body (HTML), next to `secret_names_501_is_cannot_list`. Check that the 500 mapping happens before any generic 5xx retry or classification the client applies.
- **24-05** (`deploy::pair`):
  - K2 = b: `FILE_NORMALIZATION` holds exactly one row for `resource.json`, and it compares the two files as parsed JSON documents with no member excluded (the plan's step 4 describes a row that re-compares "without the named member"; there is no named member to drop). Every other file stays sha256 byte-equal. Tests: a reordered or pretty-printed `resource.json` with equal values passes; a changed value fails; a `config.json` byte difference fails.
  - K5 = d: `is_sync_exhausted` is the log matcher only (message contains `exceeds the max attempt count`), tested with the verbatim spike line and one non-matching line. Drop the K5 a/b body-based tests (the C-P8 exhausted body does not exist). It returns a bool and no text.
  - K7: `check_secret_names` treats `CannotList` (501 or 500, mapped in 24-03) as `unverifiable`, status `warn`; the plan's wording "(501, K7)" widens to 500.
  - K8 branch is active: in `Preflight` mode a render-declared provider answering `NoProvider` goes to `skipped_providers`, not `missing`; in `Verify` mode it is `missing`.
  - K9 = a: exact, case-sensitive comparison (no folding).
- **24-06** (apply engine) and **24-10** (live gates):
  - K3 = b: the unsynced gate cuts the link from an `ApplyObserver` when it sees `ApplyEvent::LockAcquired` (after the preflights read both nodes, before the first write). 24-10 takes the "under K3 b" branch of `live_pair_unsynced_then_reconverge` and does not cut before the apply. 24-06 must keep `lock_acquired` delivered to the observer after the preflights and before any write, and may test that a cut made from the observer at that event ends `applied_backup_unsynced` (the plan's `PairLink::cut()`-before-apply case stays valid for the fake).
  - K4 = a: the cut is `setup-pair.sh cut` (toggleApproval false on gw-a), the restore is `setup-pair.sh link`; not `cut-net`.
  - K6 = a applies only to the sync-exhausted path (D-10): on the signal the wait ends, the one resync is sent, and the run reports `sync_exhausted` with no second budget (24-06 `backup_sync_exhausted_ends_early`). It does not change the timeout path: per D-07, `backup_resync_failure_is_recorded` and `backup_resync_refused_is_not_resent` still run the second budget after the resync, as the 24-06 plan text says.
- **24-07** (preflights):
  - K8 branch: the `secrets` preflight skips a render-declared provider that answers 404 with a detail naming the provider; it does not refuse. K7: a 501 or 500 provider is named unverifiable and does not fail.
- **24-09** (`deploy verify --node`):
  - K7 sub-clause: `verify_node_secrets_unverifiable` covers a provider that answers 501 or 500 (named in `unverifiable`, status `warn`, `ok` stays true). Verify still requires a render-declared provider that answers 404 (K8 verify side): it is `missing`.
- **24-04, 24-08**: no override.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `cap` echoed `tokenHash` in recorded bodies** (commit f1cf3cc)
- **Found during:** Task 1
- **Issue:** the provider and gateway-network bodies carry a `tokenHash` member; the first captures recorded it in clear.
- **Fix:** `cap` redacts `tokenHash` values before anything is recorded; the captures file holds no token value or hash (grep for the token header and the throwaway value prints 0).
- **Files modified:** `crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh`

**2. [Rule 3 - Blocking] `.config.profile.type` projection** (commit f1cf3cc)
- **Found during:** Task 1
- **Issue:** the plan's jq projection `.config.type` did not match the provider resource shape; the provider type sits in `.config.profile.type`.
- **Fix:** the projection reads `.config.profile.type` and still emits only name and type.
- **Files modified:** `crates/ignition-core/tests/fixtures/live-rig-pair/capture24.sh`

### Plan departures

- Extras beyond the plan, all read-only or names-only: a `provider` subcommand in `setup-pair.sh` to create the capture providers; `cp7remote` to record a `remote` provider as the candidate for a 501 listing (it answered 500, which led to the K7 sub-clause); a `cap24-name` case test (two names `CAP24-Name` and `cap24-name`) for K9; a wrong-key probe next to the no-token and no-permission calls.
- A10 (one token for both nodes) is wrong: tokens are node-local. Recorded in C-P4 and in the C-P4 decision; 24-10 uses `IGN_PAIR_TOKEN_A` and `IGN_PAIR_TOKEN_B`.
- The rig was torn down at the user's request after Task 1; it is down now and the 24-10 live gates need `setup-pair.sh up` again.
- C-P8: the 20-minute time box ran to 21 minutes (start 2026-10-19T21:07:14Z); the state was not reached, so no resync was sent and the before/after-resync series do not exist. The plan allows this (D-18); K5 = d and K6 = a follow its rule.
- Task 3 decision made through the orchestrator ("approve-all"); recorded verbatim under `## Contract decisions (24-02)`.

## Issues Encountered

- Whether a resync clears the exhausted state (C12) stays unknown. K6 = a does not depend on it, and the matcher is unit-tested only.

## Known Stubs

None.

## Threat Flags

None. T-24-04 held (throwaway value only in the script; grep of the captures file for it and for token headers prints 0), T-24-05 held (log lines filtered by the four fixed patterns), T-24-06 held (answer recorded verbatim with a UTC date), T-24-07 held (only GETs sent to gw-b).

## Requirements

DPAIR-01..04 are not marked complete: this plan produces captures and decisions, and the code ships in 24-03 to 24-10.

## Self-Check: PASSED

- `## Contract decisions (24-02)` present with 19 rows; `### C-P1`..`### C-P8` present (8); `re-decided at 24-02` in `COVERAGE.md`: 0; throwaway-value and token-header grep of the captures file: 0.
- Commits found: f1cf3cc, 6ee6978 (Tasks 1 and 2); the Task 3 commit follows this file.
