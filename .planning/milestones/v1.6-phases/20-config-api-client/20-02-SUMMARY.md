---
phase: 20-config-api-client
plan: 02
subsystem: api
tags: [error-taxonomy, exit-codes, redundancy, scan-lock, resource-signature, ignition-8.3.6]

requires:
  - phase: 20-01
    provides: "20-LIVE-CAPTURES.md block C-S3 (Master/Backup pair, pre- and post-failover)"
provides:
  - "CoreError::ScanLockHeld (slug scan_lock_held, exit 6) and CoreError::ResourceSignatureRejected (slug resource_signature_rejected, exit 6) with SignatureReason { Stale, Missing }"
  - "ActiveNode { OnlyA, OnlyB, Both, Neither } and active_node(a, b), decided by activityLevel only; ACTIVITY_ACTIVE / ACTIVITY_COLD consts"
  - "Independent/Master/Backup role vocabulary across redundancy.rs, version.rs, status.rs, README and the live allowlist"
affects: [20-03, 20-04, 21, 24]

tech-stack:
  added: []
  patterns:
    - "Four-place error rule (header table, code/exit_code/hint/endpoint, enumerated test + EXIT_SLUG_LITERALS, README row) landed in one commit"
    - "Error payloads structurally carry no signature or gateway problem text"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/error.rs
    - README.md
    - crates/ignition-core/src/client/redundancy.rs
    - crates/ignition-core/src/client/version.rs
    - crates/ignition-core/src/client/status.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/e2e_api_diagnostics.rs

key-decisions:
  - "active_node reads activityLevel only; role is configuration (post-failover pair keeps a=Master while activity moves to b)"
  - "Split-brain (both Active) is reported as ActiveNode::Both and never resolved; unrecognised or absent levels are not active"
  - "ResourceSignatureRejected carries only name, reason and endpoint; never a signature value or problem text"

patterns-established:
  - "Pair-capture tests copy C-S3 bodies verbatim and cite the block id plus S3.md line numbers"

requirements-completed: [DAPI-02, DAPI-07]

duration: 30min
completed: 2026-10-12
status: complete
actuals:
  tokens: 6400
  tasks: 3
  commits: 3
---

# Phase 20 Plan 02: Error slugs and active-node decision Summary

**Two new exit-6 slugs (`scan_lock_held`, `resource_signature_rejected`) landed in one four-place commit, plus a pure `active_node` that decides the serving node by `activityLevel` using the live Independent/Master/Backup vocabulary.**

## Task 1 commit (Phase 21 rebases onto this)

**`725d17c`** `feat(20-02): scan_lock_held and resource_signature_rejected slugs (four-place rule)`, touching exactly `README.md` and `crates/ignition-core/src/error.rs`, no co-author trailer. Its parent (`BASE` for Task 3 checks) is `2aeb94f`. Written without reading any 20-01 capture.

## Task Commits

1. Task 1: two slugs, `SignatureReason`, hints, endpoint arms, tests, README exit-6 row: `725d17c`
2. Task 2: `active_node`, `ActiveNode`, activity consts, redundancy.rs docs and C-S3 tests: `217a5ee`
3. Task 3: vocabulary sweep in version.rs, status.rs, README row, live allowlist, render.rs last-sync line: `98088cf`

## Accomplishments

- `ScanLockHeld { actor: Option<String>, kind, endpoint }`: Display says "the config scan-lock is held by <actor>" or "another actor" when the gateway named none; hint names the release conditions (holder's scan or holdTimeout) and no ign command.
- `ResourceSignatureRejected { name, reason: SignatureReason, endpoint }`: Display and hint never carry a signature or problem text; `deploy_slugs_never_carry_signature_or_problem_text` asserts the 64-hex sample is absent.
- `active_node` follows activity, not role: C-S3 pre-failover pair gives `OnlyA`, post-failover pair gives `OnlyB` with `a.role == "Master"`. Both Active gives `Both`, none gives `Neither`, `Warm` or absent is not active.
- `lastSyncTimestamp` documented as epoch-ms (C-S3: `1791586091827`); `-1` remains the never-synced sentinel.
- Block C-S3 already existed in `20-LIVE-CAPTURES.md` when Task 2 ran (it was added by 20-01 in `7da5730`), so the pair bodies were copied from the block (identical bytes to S3.md 6-7 and 40-41), not read from the spike directly.

## User-visible changes

One intentional correction to human output only. `ign redundancy status` last-sync line:

- Before: `last sync: <ms> epoch-ms (unit inferred, not capture-proven)`
- After: `last sync: <ms> epoch-ms`

Reason: block C-S3 proves the unit is epoch-ms on a live 8.3.6 pair. No test asserted that line. The `-1` branch and every other human line are byte-identical. JSON output unchanged: no key, value or envelope of `ign status`, `ign version` or `ign redundancy status` changed, and the six contract test files pass unedited (`git diff --name-only BASE HEAD` over them prints nothing). The only render.rs hunks are the `render_redundancy_status_human` doc comment and last-sync `println!`.

## Follow-ups

- Wire `active_node` into a command in Phase 24 (Redundant Pair), as recorded in the REQUIREMENTS.md DAPI-07 traceability row. No code under `crates/ignition-cli` calls it.
- 20-03 constructs `ScanLockHeld` in the route-scoped classify arm; 20-04 constructs `ResourceSignatureRejected` in the deploy-scoped write helper and may only ADD a `SignatureReason` variant (D3: omitted and empty signatures share one 422, stale is a 500).

## Deviations from Plan

None - plan executed exactly as written.

## Verification

- All five CI gates green locally: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`, `cargo test --workspace` (no failures), `cargo build -p ignition-cli --no-default-features`.
- Plan-scoped: `error::tests` 8 passed (including `readme_exit_table_agreement`, `exit_code_mapping_enumerated`, `deploy_slugs_never_carry_signature_or_problem_text`); `client::redundancy` 7 passed; `gateway_info_contract`, `status_contract`, `eam_contract`, `contract_status`, `contract_version`, `version_gateway_contract`, `contract_diagnostics` all pass.
- Greps: no `Primary` in the four named source/test files or the README redundancy row; `active_node` has no non-comment reference under `crates/ignition-cli`; `role` is not referenced inside `active_node`.

## Known Stubs

None.

## Threat Flags

None. No new network endpoint, auth path or file access; T-20-06 to T-20-08 mitigated as planned.

## Self-Check: PASSED

Commits `725d17c`, `217a5ee` and `98088cf` exist; all modified files exist; no stubs.
