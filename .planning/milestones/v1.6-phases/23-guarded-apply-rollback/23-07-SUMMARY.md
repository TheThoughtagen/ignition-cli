---
phase: 23-guarded-apply-rollback
plan: 07
subsystem: deploy
tags: [apply-engine, preflight, designers, wrong-target, hold-budget, deploy_preflight_failed, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-05 run_apply and HoldModel; 23-17 the single exit point and HeldLock; 23-04 ApplyHost::probe; 23-03 RunDir and EventLog; 23-02 vocabulary (option-a)
provides:
  - deploy/apply/preflight.rs, the seven preflight checks and run_preflights, aggregated into one deploy_preflight_failed refusal in a fixed order
  - run_apply order: run directory, events, write set, preflights, run.json (once), plan.json, then ledger, staging, lock
  - --designers fail, wait and proceed as DesignersPolicy (mode, timeout, interval), recorded in run.json and the events
  - NodeTarget.gateway_name (optional), validated, README target-table row
affects: [23-09, 23-10, 23-11, 23-13, 23-15, 23-16]

actuals:
  tokens: 15400
  tasks: 3
  commits: 6

tech-stack:
  added: []
  patterns:
    - "an unanswerable check is its own error, never a finding: run.json and result.json are written first, then the error is returned"
    - "the wait loop is a pure decision (wait_step) plus a thin loop, so the timeout-equals-elapsed edge is a unit test and the integration tests use real short intervals"
    - "wiremock defaults mounted at priority 250 so a test's own mock for the same route wins without unmounting"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/preflight.rs
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/target.rs
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/deploy_transport.rs
    - README.md

key-decisions:
  - "NodeTarget.gateway_name WAS added: option-a item 2 of the 23-02 checkpoint keeps both the REST-to-host binding and the optional field, and C-sysname showed the two names agree on a fresh rig"
  - "wrong_target compares the master only: the engine holds one GatewayApi (the master's); the backup's gateway_name is validated and not yet used"
  - "run.json is written after the preflights even when one aborts with its own error; designers_seen and preflights are then null"
  - "write_host runs in Task 1, not Task 2, because the tracer's own acceptance (call log has only probe) needs it"

patterns-established:
  - "detail strings name checks, counts, seconds and sources, never values: wrong_target says 'the gateway's REST name is not the host's systemName', not the two names"

requirements-completed: []
requirements-advanced: [DAPL-04]

duration: about 2h (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 07: The apply preflights and one deploy_preflight_failed refusal Summary

**Apply now runs seven preflights after the run directory exists and before any staging or lock, and reports every failure together as `deploy_preflight_failed` (exit 6) with `run.json`, `events.ndjson` and `result.json` written and nothing changed on the gateway.**

## Check ids and what each does

Fixed report order (`PreflightCheck::ALL`), one finding per check (`pass`, `fail` or `skipped`):

| Check | Fails when | Test |
|---|---|---|
| `mode_master` | the master's normalised `deploymentMode` is not the target `mode` | `preflight_mode_and_designers_reported_together` |
| `mode_backup` | the same through the backup node; `skipped` (never `pass`) when the target has no `nodes.backup`; fails when the target names a backup and the caller supplies no connection | `preflight_backup_mode_mismatch`, `preflight_backup_mode_matching_passes`, `preflight_no_backup_node_is_skipped_not_passed` |
| `wrong_target` | REST `gateway-info` `name` is not the host's `systemName` (one scoped `TreeReader::read` of `ignition/system-properties`, mode collection over `core`), or not `nodes.master.gateway_name` when set; a missing value on either side fails and names the side | `preflight_wrong_target`, `..._declared_gateway_name`, `..._host_without_a_system_name` |
| `write_rest` | `scan_projects` answers 403 | `preflight_write_rest_403` |
| `write_host` | `ApplyHost::probe` says not writable (unix test: ign-deploy directory at 0500) | `preflight_write_host_unwritable` |
| `designers` | any Designer session is open under `fail`; under `wait` still open at the timeout; detail lists user, project and host per session (capped at 10, then "and N more") | `preflight_designers_*` |
| `hold_budget` | `hold_for(estimate_commit_s(writes + deletes), p95_scan_s)` is more than `hold_s`; detail names file count, estimated commit, scan, needed hold and budget in seconds; never clamps | `preflight_hold_budget_boundary` plus a unit test |

An unreachable gateway or host is not a finding: it aborts with its own error after `run.json` and `result.json` are written (`preflight_unreachable_gateway_is_network_error` exit 4, `preflight_unreachable_host_is_transport_failed` exit 4).

## `gateway_name`: added

`#[serde(default)] pub gateway_name: Option<String>` on `NodeTarget`, in `deny_unknown_fields` schema. Validated on both nodes (not empty or whitespace, no control characters); target unit tests `gateway_name_is_optional_and_parses_on_either_node` and `gateway_name_empty_or_with_control_characters_is_refused`. README target-table row added. Only the master's value is compared today (see Next Phase Readiness). Four other `NodeTarget` literals in tests (`ledger.rs`, `transport/mod.rs`, `deploy_transport.rs`) gained `gateway_name: None`; this is why `deploy_transport.rs` and `ledger.rs` appear in the file list.

## `run_apply` order now

1. `RunDir::create_fresh` (git-ignore guard), `EventLog`, `write_set`, `run_started`.
2. `run_preflights` (read-only; the host probe makes and removes one probe file, and creates the empty `ign-deploy/<gateway>/<mode>` directories, as it did before).
3. `run.json` (once, pass or fail) with the flags, `designers`, `designers_timeout_s`, `designers_seen` (user, project, host) and `preflights` (id and status); then `plan.json`.
4. On a failed check: event `preflight_checks` (ids and statuses), then `DeployPreflightFailed`; the one exit point writes `result.json` and the `failed` event.
5. Otherwise the 23-05 path (ledger, `prepare`, lock, commit, scan, verify).

`ApplyContext` gained `backup_deploy: Option<&dyn DeployApi>`; `ApplyOptions` gained `designers: DesignersPolicy`. New events: `preflight_checks`, `designers_poll { open }`, `designers_proceed { sessions }`.

## `--designers`

`DesignersPolicy { mode, timeout, interval }`, `Default` is `fail` / `DESIGNERS_TIMEOUT_DEFAULT_S` (600 s) / `DESIGNERS_POLL_S` (10 s). `wait` reads the list, emits one `designers_poll` per read, sleeps `min(interval, timeout - elapsed)` and rereads until empty or `elapsed >= timeout` (so a timeout equal to the elapsed wait times out), then fails with the last list. `proceed` passes, emits `designers_proceed` and `run.json` carries `"designers": "proceed"` with the sessions.

## Deviations from Plan

**1. [Plan departure] `PrepareOptions` does not carry `DesignersPolicy`.** The plan's Task 3 step 2 asks for both `PrepareOptions` and `ApplyOptions` to carry it. `deploy_apply_prepare` runs before the run directory and reads nothing from the Designer list, so a field there would be unused. `ApplyOptions` (the execute path) carries it. 23-13 can add the field to `PrepareOptions` if the CLI wants one struct, with no engine change.

**2. [Plan departure] `write_host` lands in Task 1.** Task 1's behaviour says the `FaultyHost` call log has only `probe`; that needs `ApplyHost::probe` to run in the tracer. Task 2 added the other four checks.

**3. [Rule 3 - Blocking] Existing 23-05 and 23-17 call-list assertions began with `prepare`.** `probe` is now the first host call of every run, so four assertions (`apply_happy_path_local`, `fault_write_failed_mid_commit_restores`, `fault_pre_image_changed_is_plan_stale`, `fault_competing_lock_is_scan_lock_held`) gained `"probe"`, and the shared rig now answers `gateway-info`, `designers` and the project scan and holds a `systemName` file. Committed with the Task 1 RED test (`b32dc29`).

**4. [Test shape] The idempotency and "nothing changed" assertions compare file lists.** The host probe creates the empty `ign-deploy/<gateway>/<mode>` directories (23-04 behaviour), so "no `ign-deploy` directory" would be wrong; the tests assert no new file.

No other deviations. No auth gates.

## Plan edge coverage

| Edge | Where |
|---|---|
| DAPL-04 adjacency | `preflight_hold_budget_boundary` (need equals hold passes, one second more fails), `wait_timeout_equal_to_the_elapsed_time_has_passed`, `preflight_designers_wait_times_out` |
| DAPL-04 empty | `preflight_no_backup_node_is_skipped_not_passed`, `preflight_designers_none_pass_everywhere` (all three modes) |
| DAPL-04 ordering | `preflight_all_fail_in_fixed_order` |
| DAPL-04 idempotency | `preflight_is_idempotent` (two runs, same list, two run directories, host files unchanged) |
| DAPL-04 concurrency (backstop) | not tested here by design: a Designer or foreign edit after the preflights is caught by the 23-05/23-17 under-lock pre-image check (`fault_pre_image_changed_is_plan_stale`); C2 in 23-LIVE-CAPTURES records the lock queues API writes and does not block filesystem writes, and C2b (a Designer save under the lock) is still a manual check |

## Human-verify checkpoints

None in this plan (`human_verify_mode=end-of-phase`); the tracer feedback gate was met autonomously: the tracer's `<verify>` (`preflight_mode_and_designers_reported_together`) passed before the tracer GREEN commit and again after Task 3.

## Verification

- `cargo test -p ignition-core --test deploy_apply`: 42 passed (14 `preflight_*` before Task 3, 18 after; the rest are 23-02 to 23-17)
- `cargo test -p ignition-core --lib deploy::`: passed, including the target and preflight unit tests
- `cargo test --workspace --no-fail-fast`: 2374 passed, 0 failed across 86 test binaries; the four `contract_rig.rs` tests ran green (Docker daemon up)
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo fmt --all -- --check`: clean

## Known Stubs

None. `grep` for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` over the changed files finds nothing.

## Threat Flags

None beyond the plan's register. T-23-28 (wrong gateway) is mitigated by `wrong_target` and tested; T-23-29 by `write_rest` and `write_host`; T-23-30 by the `fail` default, bounded `wait` and recorded `proceed`; T-23-31 by `hold_budget` and its boundary test. T-23-32 is accepted as planned: user, project and host appear in the private run directory and the refusal text. Refusal details otherwise carry no gateway value (checked by `preflight_wrong_target`).

## Next Phase Readiness

- **23-13 (the verb):** build `backup_deploy` from the `nodes.backup` profile when the target has one (without it `mode_backup` fails, by design), pass `ApplyOptions.designers` from `--designers` / `--designers-timeout` (`DesignersPolicy::default()` already holds the 600 s / 10 s production values), and map `DeployPreflightFailed` to exit 6 like the other refusals. The README `### deploy apply` section should list the seven check ids; only the target-table row was added here.
- **23-12 / 23-11:** manual rollback has its own designers option in the plan; `run_preflights` is reusable if rollback wants the same Designer policy (it takes a `Steps` and a `PreflightInput`, and the designer check can be lifted out).
- **Backup `gateway_name`:** accepted and validated but not compared, because the engine holds only the master's `GatewayApi`. A later plan that gives the backup a `GatewayApi` can compare it with no schema change.
- **C2b** (a Designer save under a held lock) remains a manual check owed to the user; until it is recorded the Designer preflight is the only assumed guard against a UI save during an apply.

## Self-Check: PASSED

- Created and modified files exist: `crates/ignition-core/src/deploy/apply/preflight.rs`, `.../apply/mod.rs`, `.../deploy/target.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `README.md`, this SUMMARY.
- Commits `b32dc29`, `d80a950`, `03810f5`, `6526d03`, `44f27ba`, `6e84187` exist in `git log`.
- `grep -c 'pub async fn run_preflights' crates/ignition-core/src/deploy/apply/preflight.rs` prints 1.
