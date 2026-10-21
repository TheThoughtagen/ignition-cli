---
phase: 24-redundant-pair
plan: 07
subsystem: core
tags: [ignition, redundancy, preflight, master-active, backup-read, secrets, deploy-apply]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: "24-05 BackupNode (status, read_files, secret_names, has_files), collect_referenced_secrets, declared_secret_providers, check_secret_names; 24-03 DeployApi::secret_names and the fake's secret seeding; 24-04 active_node; the 24-02 contract decisions"
provides:
  - "PreflightCheck::{MasterActive, BackupRead, Secrets}; PreflightCheck::ALL has ten ids in the V4 order"
  - "PreflightInput.declared: Option<&Declared> (None in rollback)"
  - "master_active_finding, backup_read_finding, secrets_finding"
  - "FakeGateway::seed_secret_with_value (canary value kept beside a name, never listed)"
affects: [24-08, 24-09, 24-10]

actuals:
  tokens: 12300
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "a check whose read is the check turns the read error into a finding; every other unanswerable read aborts with its own error"
    - "details list node: provider/name pairs, capped at APPLY_LIST_SHOWN, and never a value"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/apply/preflight.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/deploy_apply.rs

key-decisions:
  - "Final preflight order: mode_master, mode_backup, wrong_target, write_rest, write_host, designers, hold_budget, master_active, backup_read, secrets"
  - "master_active decides by activityLevel through active_node, never by role; OnlyB, Both and Neither fail; rollback runs it too (K3 b: D-11 unchanged)"
  - "backup_read and secrets are skipped in rollback (declared is None there) and say so in the detail"
  - "A declared backup node with no gateway connection fails master_active and secrets (cannot confirm), and skips backup_read, which has nothing to read"

patterns-established:
  - "mount_redundancy / pair_levels / set_levels test helpers: a per-node redundancy status behind wiremock"

requirements-completed: []

duration: not timed
completed: 2026-10-20
---

# Phase 24 Plan 07: master_active, backup_read and secrets preflights Summary

**An apply or rollback to a redundant pair is now refused before the lock, with every reason in one refusal, when the master is no longer the only active node, when the backup's mode collection cannot be read by the backup account, or when a secret name the render references is missing on either node; the report has ten ids in the fixed V4 order.**

## What was built

- **Task 1 (tracer, approved by the user).** `master_active` reads `GET /data/api/v1/redundancy` on the master (through `PreflightInput.gateway`) and on the backup (`BackupNode::status`) and passes only when `active_node` says `OnlyA`. `OnlyB` (the pair switched over), `Both` (split-brain) and `Neither` fail with a detail that names both activity levels; it is `skipped` without `nodes.backup`. A healthy `Cold` standby with `uptime: -1` passes (Pitfall 5). `PreflightInput` gained `declared: Option<&Declared>` (`Some` in apply, `None` in rollback), and `rollback_runs_master_active` shows a rollback to a standby master refused with nothing written, locked or scanned. Commits 872360c (RED) and 554b887 (GREEN).
- **Task 2.** `backup_read` and `secrets` appended to `ALL`.
  - `backup_read` reads the backup's mode collection once through `BackupNode::read_files(&Scope::from_manifest(&declared.manifest, &declared.target.mode))`. A read error FAILS the check and the detail carries the error code (`transport_failed` for a mode-0000 directory); success names the file count. It is `skipped` with no backup node, in rollback, for a backup with no `ssh` or `local` block (detail: "the file-hash check did not run") and when the backup handle is absent. The module doc records why this one check turns a read error into a finding.
  - `secrets` collects the `(provider, name)` pairs the rendered tree references, lists each referenced provider on the master and, when declared, the backup, and runs `check_secret_names(.., SecretCheckMode::Preflight)` per node. Any missing name fails; the detail lists `node: provider/name` pairs up to `APPLY_LIST_SHOWN` plus "and N more", then the skipped and unverifiable providers. A listing that cannot be answered at all (a network error) aborts with its own error after `run.json` is written. It is `skipped` with no references and in rollback.
  - Commits 7e50f77 (RED) and bc88e07 (GREEN).

## Contract branches applied

- **K3 = b.** `master_active` is unchanged from D-11 (refuses `OnlyB`, `Both`, `Neither`); the live gate that cuts the link does so from an observer at `lock_acquired`, after these preflights, which is 24-06 and 24-10.
- **K7 = a (with the sub-clause).** A provider answering 501 or 500 is `CannotList`: named "unverifiable" in the detail, status `pass`. Any other non-2xx stays an error and aborts with its own code (that classification is in 24-03's client).
- **K8 = a.** A provider the render itself declares (`ignition/secret-provider/<P>/...`) that answers 404 is skipped with a detail naming it and does not refuse. A provider the render does not declare that answers 404 has all its names missing and fails (test `preflight_secrets_undeclared_missing_provider_fails`).
- **K9 = a.** Names compare exactly and case-sensitively (test `preflight_secrets_names_are_case_sensitive`: `CAP24-Name` on the master, `cap24-name` on the backup fails).

## Deviations from Plan

### User-approved overrides of the plan text

**1. [Override K8 = a] Plan truth said a 404 on a declared provider is skipped; the plan task text allowed a refusal reading.** Implemented as skip-with-detail, as above. `deploy verify` (24-09) still requires the provider.

**2. [Override K7 = a + sub-clause] "Cannot list" is 501 and also 500.** Both are named unverifiable and do not fail.

**3. [Override K9 = a] Exact, case-sensitive names.**

### Auto-fixed and harness deviations

**4. [Rule 3 - Blocking] The OnlyB detail avoids the word "failover".** The T-23-39 source guard `apply_source_never_names_the_restart_route` bans it in non-comment lines under `src/deploy`. The detail reads "activity moved to the backup (the pair switched over)". Keep avoiding that word in later plans. (Task 1.)

**5. [Test harness] `Rig::with_backup` is async and starts a wiremock backup server answering redundancy status `Cold` (`Rig.backup_server`).** `mount_passing_preflights` answers the master as `Active` at priority 250. New helpers: `redundancy_body`, `mount_redundancy`, `pair_levels`, `set_levels`, `refusals`, `run_status`, `finding_of`. (Task 1.)

**6. [Test harness] `Rig.backup_fake`.** Task 2's secret tests seed what the backup's providers list, so `Rig` keeps the backup's `FakeGateway` (set by `with_backup` and `with_pair`). New helpers: `backup_files`, `lock_collection`/`unlock_collection` (unix), `reference_secret`, `declare_secret_provider`, `run_dir_text`. The tests edit `rig.declared.outcome.tree` to add a config file with a `Referenced` member or a provider directory.

**7. [Plan departure] `FakeGateway::seed_secret_with_value`.** The plan allowed "a canary value stored beside a name and never returned by `secret_names`". Added; `preflight_secrets_missing_on_backup` asserts the canary is absent from the refusal detail and from every file in the run directory (`run.json`, `result.json`, `events.ndjson`, `plan.json`). It guards the contract rather than exercising a leak path, since `DeployApi::secret_names` returns names only.

**8. [Tracer gate] Stopped after Task 1; the user approved ("tracer verified").**

**9. [Own test bug] `preflight_backup_read_fails_on_an_unreadable_backup` first asserted `host.calls() == ["probe"]`;** the test also ran a passing `preflights()` call first, so two probes were recorded. It now uses `assert_only_probes` (all calls are probes). Fixed within the test before the GREEN commit.

### Added beyond the plan

Tests: `preflight_backup_read_skipped_without_a_backup_node`, `preflight_secrets_without_a_backup_node_check_the_master_only`, `preflight_secrets_names_are_case_sensitive`, `preflight_secrets_undeclared_missing_provider_fails`, `preflight_secrets_listing_that_cannot_be_answered_aborts`. The all-fail test now fails all ten checks (the `backup_read` failure is expected only when the process is held to permission bits, as `write_host` already was).

## Not done here

- The README preflight table and the human rendering of the new ids are 24-08 and 24-09, as the orchestrator said.
- `RollbackContext` and `ApplyContext` needed no change; both already carried `backup`.

## Verification

- `cargo test -p ignition-core --lib deploy::apply::preflight`: 10 passed (the ten-id order pinned).
- `cargo test -p ignition-core --test deploy_apply preflight_`: 35 passed after the test fix (every Phase 23 preflight test unchanged except the two order lists).
- `cargo test -p ignition-core --test deploy_apply rollback_runs_master_active`: passed; the recorded `PreflightChecks` event shows `master_active` fail and `backup_read` and `secrets` skipped.
- `cargo clippy --workspace --all-targets -- -D warnings`: exits 0. `cargo fmt --all` applied (only this plan's files changed).
- `cargo test --workspace --no-fail-fast`: every test binary passes except three, all unrelated to this plan and not fixed here (the machine's load average was 70 to 95):
  - `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`: the known load flake (see `deferred-items.md`).
  - `ignition-core --lib actions::diagnostics::tests::unknown_states_keep_polling_until_the_deadline`: a 100 ms deadline test that timed out under load; no Phase 24 code involved. Not in `deferred-items.md` before this run.
  - `ignition-cli --test contract_rig` (4 tests: `rig_up_with_module_json_reports_provisioned_modules` and three `*_after_undeclaring_*`): `docker compose up` answered "no space left on device" on the host; environmental.
- A first RED run of the new engine tests took 13 minutes because a refusal that did not happen let the apply run to its 300 s backup budgets twice; with the GREEN code the same tests finish in about 17 s.

## Known Stubs

None.

## Threat Flags

None. T-24-24 (master_active in apply and rollback; OnlyB, Both, Neither and rollback tests), T-24-25 (names only, canary test), T-24-26 (backup_read refuses before the lock; unix mode-0000 test) and T-24-27 (`skipped` is its own status; tests assert it for REST-only, no-backup, rollback and no-reference) are covered by tests.

## Requirements

DPAIR-01 and DPAIR-03 are not marked complete: this plan ships the engine checks; the CLI flag and README (24-08), verify (24-09) and the live gates (24-10) follow.

## Self-Check: PASSED

`crates/ignition-core/src/deploy/apply/preflight.rs` carries `PreflightCheck::{MasterActive, BackupRead, Secrets}`; the four commits (872360c, 554b887, 7e50f77, bc88e07) exist. The ten-id order is pinned by `ids_are_the_contract_words_in_report_order` and the engine tests.
