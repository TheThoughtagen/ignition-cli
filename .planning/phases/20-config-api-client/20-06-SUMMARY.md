---
phase: 20-config-api-client
plan: 06
subsystem: api
tags: [live-gate, ignition-8.3.6, deploy-api, fake-gateway, differential-test, loopback-guard]

requires:
  - phase: 20-01
    provides: "live-rig-deploy fixture (compose.yml, mint-token.sh) and 20-LIVE-CAPTURES.md"
  - phase: 20-05
    provides: "FakeGateway/FakeDeploy and the shared scenario with ScenarioTiming::{fake, live}"
provides:
  - "tests/live_deploy_api.rs: local_url_guard, RIG_GATEWAY_NAME, CI-run guard tests, and one ordered #[ignore] live gate over the lock, scan, mode, create, find, update, list and names methods (not resource_singleton, resource_list_in_collection, a filtered list, ScanKind::Projects or from_record updates until the next live run; see 20-REVIEW-FIX WR-04)"
  - "20-06-live-gate.log: the committed, key-free output of the green live run"
  - "Dated Live gate 20-06 entry under Drift and limits in 20-LIVE-CAPTURES.md"
affects: [22, 23, 24]

tech-stack:
  added: []
  patterns:
    - "Live gate refuses by parsed host (loopback only) before any request, then by gateway-info name before any write"
    - "Output carries section result lines only, never a token or a signature"

key-files:
  created:
    - crates/ignition-core/tests/live_deploy_api.rs
    - .planning/phases/20-config-api-client/20-06-live-gate.log
  modified:
    - .planning/phases/20-config-api-client/20-LIVE-CAPTURES.md

key-decisions:
  - "Booted the rig with plain compose.yml (no bind mount); D11 records both variants as viable and the gate needs no host access to resources/"
  - "No wire drift, so no client, fake or contract file was touched"

requirements-completed: [DAPI-01, DAPI-02, DAPI-03, DAPI-04, DAPI-05, DAPI-06]

duration: 35min
completed: 2026-10-12
status: complete
actuals:
  tokens: 5200
  tasks: 2
  commits: 2
---

# Phase 20 Plan 06: Live gate against the ephemeral 8.3.6 rig Summary

**One recorded green run of the `DeployApi` lock, scan, mode, create, find, update, list and names methods against a disposable loopback 8.3.6 gateway (not run live in that run: `resource_singleton`, `resource_list_in_collection`, a filtered list, `ScanKind::Projects`, `from_record`/`with_collection` updates), with the `FakeDeploy` trace equal to the live trace tag for tag and no wire drift.**

## Task Commits

1. **Task 1: live gate file with loopback and rig-name guards** - `0630780` (test)
2. **Task 2: live run, committed log, drift entry** - `42d9785` (docs)

## Live-gate result

`test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 13.24s`

Committed log: `.planning/phases/20-config-api-client/20-06-live-gate.log` (commit `42d9785`). Per-section lines:

```
PASS guard: loopback URL, gateway name is ign-deploy-rig
PASS A: lock held by cap-a, cap-b refused naming cap-a, then Free after expiry
PASS B: scan_and_wait Complete, lastScanTimestamp advanced
PASS D: no active mode, production mode created and listed, still no active mode
PASS C: create, effective/core/production reads, update, stale refusal leaves signature, list, names, duplicate refused
PASS E: live trace equals fake trace (13 steps)
PASS Z: cleanup scan complete
```

## Fake-vs-live parity

Both traces, identical:

`lock:acquired, lock:held-by-a, lock:refused-held-by-a, lock:free, scan:complete, create:ok, find:ok, update:ok, update:stale, create-duplicate:internal, mode:created, modes:listed, deployment-mode:none`

The 20-05 risk (a collection-less PUT refused live) did not materialise: `update:ok` on the live gateway. Which copy such a PUT targets is still not shown by a capture; the scenario's resource had only a core copy at that point.

## Drift reconciled

None. No client, fake or contract file was modified.

## Deviations from Plan

None - plan executed as written. The rig used `compose.yml` alone rather than the bind variant D11 chose for the captures; the plan allowed either ("else compose.yml alone") and the gate does not read `resources/` from the host.

## Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo build -p ignition-cli --no-default-features`: clean.
- `cargo test --workspace`: 1544 passed, 0 failed, 41 ignored (live-gated).
- Without `IGNITION_LIVE_DEPLOY`, `-- --ignored` is a quiet green skip; both guard unit tests run in CI.
- Key-leak check: no key from tokens.env in the log or anywhere under `.planning` or `crates`; the committed log has 0 sixty-four-hex strings.
- Teardown: `down -v` run; no `ign-deploy-rig` container or volume remains; tokens.env and the state-dir log deleted. `ign-jvm-live-test` and the other containers were not touched.
- `src/deploy` absent; no `preserve_order` or `arbitrary_precision` in any Cargo.toml; Block C-S3 present in 20-LIVE-CAPTURES.md.

## Known Stubs

None.

## Threat Flags

None. The gate's surface is the one covered by T-20-27 to T-20-31 (loopback guard, name guard, key-free output, rig teardown, fake-vs-live diff).

## Self-Check: PASSED

- FOUND: crates/ignition-core/tests/live_deploy_api.rs
- FOUND: .planning/phases/20-config-api-client/20-06-live-gate.log
- FOUND commit 0630780
- FOUND commit 42d9785
