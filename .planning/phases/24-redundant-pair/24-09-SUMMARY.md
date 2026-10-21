---
phase: 24-redundant-pair
plan: 09
subsystem: cli
tags: [ignition, redundancy, deploy-verify, secret-names, canary, readme]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: "24-05 BackupNode, compare_files and check_secret_names; 24-07 preflight secret check; 24-08 backup block and README exception; 24-02 contract decisions V6, V7, K7, K8, K9"
provides:
  - "ign deploy verify --node master|backup|both (default master, output byte-identical to before)"
  - "VerifyNode, deploy_verify_node, DeployVerifyResult.{backup, secrets} (skip_serializing_if)"
  - "data.secrets.nodes[]: per-node secret-name check, names only, master then backup"
  - "secrets-on-node human lines; shared render_backup_report_human"
  - "backup canary: apply_canary_backup, apply_canary_backup_refused_listing, verify_canary_both, core canary_backup_surfaces"
affects: [24-10]

actuals:
  tokens: 20500
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "an additive result member behind skip_serializing_if keeps the default output byte-identical"
    - "secret canary seeded on every surface the runner reads from the backup, scanned over streams and non-exempt run files"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/deploy/pair.rs
    - crates/ignition-core/src/deploy/apply/converge.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/common/mod.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-core/tests/deploy_verify.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - README.md

key-decisions:
  - "K7 sub-clause applied in verify: 501 and 500 are both CannotList (unverifiable, warn, ok stays true); any other non-2xx is an error envelope (exit 5 for a 403)"
  - "K8 verify side: a render-declared provider that a node answers 404 for is missing (SecretCheckMode::Verify)"
  - "K9: secret names compare exactly and case-sensitively"
  - "--node both asks the master for its secret listings through the same DeployApi it verifies through; --node backup lists the backup only"

patterns-established:
  - "Tests that need a rendered secret reference rewrite the tracer config and re-render into the live tree (reference_secrets) instead of editing live bytes"

requirements-completed: []

duration: not timed
completed: 2026-10-20
---

# Phase 24 Plan 09: ign deploy verify --node and the backup secret canary Summary

**`ign deploy verify --node master|backup|both` runs the backup checks on demand (redundancy, mode, REST read-back, files against the master's live copy) and checks that every secret name the rendered tree references exists on each node, names only; the default stays byte-identical, verify stays read-only, and a canary seeded on every backup surface never reaches an output or a run file.**

## What was built

- **Task 1 (tracer, c4c6e21, approved by the user):** `VerifyNode`, `deploy_verify_node`, `--node` (`VerifyNodeArg`, default `master`), `pair::verify_report` (verify's own verdict: no sync-timestamp clause, reasons `unreachable`, `not_syncing`, `content_differs`), the shared `render_backup_report_human`, `effective_files` moved into `pair.rs`. `--node backup|both` without `nodes.backup` is `invalid_input` (exit 2) before any contact; the envelope profile is the backup's for `--node backup`.
- **Task 2 (8224d84):** the secret-name check. `verify_secret_names` lists names per referenced provider on the master (through its `DeployApi`) and on the backup (through `BackupNode`) and judges them with `check_secret_names(.., SecretCheckMode::Verify)`. `data.secrets.nodes[]` carries `node`, `status`, `providers_checked`, `missing` (byte-sorted `{provider, name}`) and `unverifiable`. A `fail` on any node turns the verdict red (`deploy_verify_failed`, exit 6 after the envelope). Human output adds `secrets on <node>: <status> (<n> providers)` with `  missing: <provider>/<name>` and `  unverifiable: <provider>` lines (control characters dropped from names). README: Commands row, a `Checking the backup (--node)` paragraph, a `The secret-name check` paragraph and one sentence in the dated verify exception; `verify_verdict_exception_is_dated_and_documented` stays green.
- **Task 3 (0bcd3a7):** the canary over the backup. CLI: `apply_canary_backup` (unsynced and converged, `--json` and human), `apply_canary_backup_refused_listing` (403 body names the canary), `verify_canary_both` (green, red, refused; `--node both` and `backup`; both modes). Core: `canary_backup_surfaces` over `PairLink` (converged and unsynced). The seeds: a backup-only resource unit, a differing rendered backup file, an extra member of the backup status body, a backup log entry of an unrelated logger and a 403 listing body. Each test asserts the seed was real (the backup file still holds the canary, the status and listing routes were served, a `file_differs` finding names only the path). `CANARY_EXEMPT` is unchanged. No leak was found, so no production code changed for Task 3.

## Verification

- `cargo test -p ignition-core --test deploy_verify verify_node_`: 9 passed.
- `cargo test -p ignition-cli --test contract_deploy verify_cli_node`: 10 passed; `... canary`: 9 passed.
- `cargo test -p ignition-core --test deploy_apply canary`: 3 passed.
- `cargo test -p ignition-cli --test deploy_family_coverage`: 8 passed.
- `cargo test --workspace --no-fail-fast` (run once): 2640 passed, 1 failed: `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`, the known load failure unrelated to this plan. `actions::diagnostics::tests::unknown_states_keep_polling_until_the_deadline` and `contract_rig` passed in this run.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `grep -c -- '--node master|backup|both' README.md` prints 1; `CANARY_EXEMPT` grep prints 1.

## Deviations from Plan

1. **[Process] Task 1: no separate RED commit.** Builds take 5 to 8 minutes under load; the tests fail without the implementation.
2. **[Process] Tasks 2 and 3: implementation written before the tests** (same reason). The new tests were checked to depend on the code they cover (the secret tests need the secrets member; the canary tests assert their seeds were served).
3. **[Override, Task 1] `redundancy_check(.., false)` plus `verify_report`** instead of `report()`: verify has no pre-apply sample, so the sync timestamp is recorded, not compared.
4. **[Addition, Task 1]** `data.node` is `both` for `--node both`.
5. **[Addition, Task 1]** the shared human backup renderer prints findings indented and hides the wait and resync lines when `budget_s` is 0.
6. **[Refactor, Task 1]** `effective_files` moved from `converge.rs` into `pair.rs`.
7. **[Override, user-approved, Task 2] K7 sub-clause:** `verify_node_secrets_unverifiable` covers both 501 and 500 (the client already maps both to `CannotList`), status `warn`, envelope `ok` stays true. Any other non-2xx stays an error (a 403 is exit 5).
8. **[Override, user-approved, Task 2] K8 verify side:** a declared provider answering 404 counts as `missing` (`SecretCheckMode::Verify`); test `verify_node_declared_provider_without_a_node_is_missing`.
9. **[Override, user-approved, Task 2] K9:** exact, case-sensitive names; test `verify_node_secret_names_are_case_sensitive`.
10. **[Addition, Task 3] Test support beyond the plan's file list:** `mount_pair_backup_seeded` in `crates/ignition-cli/tests/common/mod.rs` (`mount_pair_backup` now delegates to it, behaviour unchanged) and `PairLink::status_extra` / `PairLink::log_entry` in `crates/ignition-core/tests/common/fake_deploy.rs`. The plan listed only `fake_deploy.rs` for the core side.
11. **[Addition, Task 3]** an extra CLI test `apply_canary_backup_refused_listing`: a backup that refuses a secret listing with an HTML body naming the canary ends the apply with an error envelope (exit 5), and the body appears in no stream or run file. The plan named the 403 body as a seed but not a separate test, because a refused listing cannot coexist with a converged or unsynced outcome.
12. **[Addition, Task 2] Extra tests** beyond the plan's six core behaviours: `verify_node_declared_provider_without_a_node_is_missing`, `verify_node_secret_names_are_case_sensitive`, `verify_node_master_has_no_backup_or_secrets`, and CLI `verify_cli_node_both_unverifiable_provider_is_a_warning`, `verify_cli_node_secret_listing_follows_the_node`.
13. **[Limitation] The 403 refused-listing body is exercised through real HTTP only in the CLI tests;** the core fake cannot answer a listing with an HTTP error.
14. **[Process] Tracer gate:** stopped after Task 1 for the interactive checkpoint; the user approved ("tracer verified").

## Auth gates

None.

## Known Stubs

None.

## Threat Flags

None. `deploy verify --node both` now issues GET secret-name listings to both gateways; that surface is the plan's own threat model (T-24-31, held: names only, canary tests).

## Requirements

DPAIR-01..04 are not marked complete (hard rule of this execution): the live gates of 24-10 remain.

## Self-Check: PASSED

- FOUND: crates/ignition-core/src/actions/deploy.rs (`deploy_verify_node`, `verify_secret_names`), crates/ignition-core/tests/deploy_verify.rs, crates/ignition-cli/tests/contract_deploy.rs, README.md.
- FOUND commits: c4c6e21, 8224d84, 0bcd3a7.
