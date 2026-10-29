---
phase: 24-redundant-pair
plan: 06
subsystem: core
tags: [ignition, redundancy, backup-node, deploy-apply, converge, verdict]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: "24-05 BackupNode (read-only), converged/classify/wait_step/report/compare_files; 24-03 DeployApi::redundancy_resync; the 24-02 contract decisions"
provides:
  - "deploy::apply::converge::converge_backup: the backup phase of an apply"
  - "OutcomeKind::AppliedBackupUnsynced (applied_backup_unsynced), BACKUP_UNSYNCED_SLUG (deploy_backup_unsynced), exit 6 through DeployApplyResult::failure_exit_code"
  - "ApplyEvent::{BackupSampled, BackupWaitStarted, BackupPoll, BackupResync, BackupVerdict}"
  - "ApplyOptions.backup (BackupPolicy), ApplyOutcome.backup, DeployApplyResult.backup (data.backup, result.json)"
  - "PairLink fake pair model; FakeGateway::{with_pair, reload_disk, config_lock_held}"
affects: [24-07, 24-08, 24-09, 24-10]

actuals:
  tokens: 52000
  tasks: 3
  commits: 6

tech-stack:
  added: []
  patterns:
    - "a phase that runs after the run is final and cannot undo it: the verdict is a kind of outcome, not an error"
    - "the one write of a phase is a method on a Phase struct with its record held in an Option set once, so a second send has no code path"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/converge.rs
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/live_deploy_apply.rs
    - crates/ignition-cli/src/render.rs

key-decisions:
  - "The sample is read in drive() right after the preflights pass and before take_gwbk; a failed read returns its own error before any write (V5)"
  - "An event-log failure inside the backup phase becomes the events_log_unwritable warning, never an error; the phase never returns Err"
  - "The master's live tree for the file compare is read once through ctx.reader and kept; the backup's is read on each content check"
  - "DeployApplyResult.backup is boxed (Option<Box<BackupReport>>) so ApplyPrepared::Noop stays under clippy's large_enum_variant limit; the serialized shape is unchanged"

patterns-established:
  - "PairLink: a backup modelled by two data directories and a wiremock status/log responder"

requirements-completed: []

duration: not timed
completed: 2026-10-20
---

# Phase 24 Plan 06: apply ends with backup convergence proof Summary

**An apply to a redundant pair now samples the backup before the first write, and after the master is finalized waits outside the lock, polling only the backup's own reads, then reports `data.backup`; a backup that cannot be proven ends `applied_backup_unsynced` (exit 6) after exactly one resync to the master, and the master deploy is never undone.**

## What was built

- **Task 1 (tracer).** `converge_backup` in the new `deploy/apply/converge.rs`; the backup's `lastSyncTimestamp` is sampled after `preflights.passed()` and before `take_gwbk` (`backup_sampled`); after `fetch_snapshot` the phase emits `backup_wait_started`, polls the backup's status, runs the mode and REST read-back checks when the signals pass, and ends with `backup_verdict`. `ApplyOptions.backup`, `ApplyOutcome.backup` and `DeployApplyResult.backup` carry the report into `data.backup` and `result.json`. The fake pair model `PairLink` lives in `tests/common/fake_deploy.rs`.
- **Task 2.** After the first budget (or the exhausted signal) the one `redundancy_resync` is sent through the MASTER's `DeployApi` (the only call site in `converge.rs`, `self.ctx.deploy.redundancy_resync()`); its record is an `Option<ResyncRecord>` set once. On a timeout a second budget runs, then one final evaluation with the content checks forced; the verdict is `applied_backup_unsynced` with `ok` false, slug `deploy_backup_unsynced`, `cause` null, exit 6 through `failure_exit_code`. `run_apply`'s single exit and `base_result` set `ok` and `slug` by kind.
- **Task 3.** The `files` check reads the master's live tree once (`ctx.reader`) and the backup's tree (`BackupNode::read_files`) on each content check, over the effective view (mode over `core`), and compares every rendered path and every path the apply deleted through `compare_files`. The K6 a early exit, the interrupt edge and the repeat-apply edge are tested.

## Deviations from Plan

### Contract overrides relayed by the orchestrator (24-02 decisions)

**1. [Override K3 = b] The unsynced gate cuts the link from an `ApplyObserver` at `LockAcquired`**
- The engine guarantees `lock_acquired` is delivered to the observer after the preflights and the backup sample and before the commit. It already did (the event is emitted after `HeldLock::acquire`, before `host.commit`); the sample was moved ahead of `take_gwbk`, so the order is `preflights, backup_sampled, gwbk_downloaded, staged, lock_acquired, committed`.
- Test `backup_cut_at_lock_acquired_ends_unsynced`: an observer calls `PairLink::cut()` on `LockAcquired`; it asserts the master file still holds its old bytes at that moment, the scan-lock is held, and the order sampled < lock_acquired < committed in the events the observer saw; the run ends `applied_backup_unsynced` with reason `not_syncing`, record `applied`, no restore events. The plan's `PairLink::cut()`-before-apply case (`backup_unsynced_not_syncing`) is kept too.

**2. [Override K4 = a] The live cut is `toggleApproval false` on gw-a**
- Nothing in this plan cuts a live link; the fake `PairLink::cut` models it (a cut drops `peerConnected` on the backup and stops the sync, C-P5). 24-10 uses `setup-pair.sh cut` and `setup-pair.sh link`.

**3. [Override K6 = a, sync-exhausted path only] No second budget after the exhausted signal**
- `converge_backup` takes the plan's "K6 a" branch: on `PairWaitStep::Exhausted` the wait ends at once, the one resync is sent, the final evaluation runs and the run reports `sync_exhausted`. There is no `PairLink::exhaust(true)` + K6 b converge test variant (the `resync_clears` knob on the fake is kept and used by no test).
- Test `backup_sync_exhausted_ends_early` (budget 2 s per budget): reason `sync_exhausted` (ranked above `not_syncing`, which is also true), exactly 2 `backup_poll` events (the wait's one look and the final evaluation), `waited_s` < 2, one resync (`PairLink::resyncs()` 1, `FakeGateway::resyncs()` 1), one `backup_resync` event, record `applied`, no `restore_started` or `restored`, master files equal the render.
- The ordinary TIMEOUT path is unchanged per D-07: after the first budget times out the one resync is sent and the second budget still runs. `backup_resync_failure_is_recorded` asserts the events after `backup_resync` include `backup_poll` and `waited_s` covers both budgets (budget 1 s each, `waited_s >= 2`); `backup_resync_refused_is_not_resent` asserts the polls continue after the refused POST.

**4. [Override K5 = d] The exhausted signal is `BackupNode::sync_exhausted`**
- Read each tick through `GatewayApi::logs` (the log matcher); there is no providers read. The fake serves the spike's log line from the backup's wiremock `GET /data/api/v1/logs` (`PairLogs`) when `PairLink::exhaust` is set. The state was never reached on the live rig (C-P8), so this path is exercised by the fake only (D-18).

**5. [Override K2 = b] Already implemented in 24-05**
- `FILE_NORMALIZATION = [("resource.json", "")]`. Consequence for a test: `PairLink::tamper` appends a non-whitespace byte, since a trailing space on `resource.json` is no difference under K2 b (found while writing `backup_files_differ`; the first version passed unexpectedly, which is what the RED run showed).

### Plan departures

**6. [Test harness] The `Rig` backup deploy surface is `Option<Box<dyn DeployApi>>`**
- 24-05's `Rig.backup` was a `FakeDeploy`. A pair rig wraps the backup's deploy surface in `ForbidWrites` (a write panics), so the field became a boxed trait object; `with_backup` boxes its `FakeDeploy`. `Rig` also gained `backup_reader` (a `LocalFsTransport` over the backup's data dir, label `nodes.backup`), `pair` (the backup's wiremock server, data dir and link) and the helper `with_pair`.

**7. [Plan wording] `converge_backup` signature**
- The plan lists `converge_backup(ctx, opts, plan, &set, before, steps)`. The shipped signature is `converge_backup(ctx, backup, opts, &set, before, steps, &mut warnings)`: the caller already holds the `&BackupNode`, the plan is not needed (the declared render is in `ctx`), and the warnings vector is how an unwritable event log is reported without an `Err`.

**8. [Rule 3 - Blocking] The stderr progress observer in `ignition-cli`**
- `render.rs::describe_apply_event` matches `ApplyEvent` exhaustively, so the five new events broke the CLI build. It gained five progress lines (flags, counts and codes only). The human result block, `--backup-timeout`, exit 6 for the new outcome in the CLI and the README are 24-08's and are untouched.

**9. [Rule 1 - clippy] `DeployApplyResult.backup` is `Option<Box<BackupReport>>`**
- `ApplyPrepared::Noop(DeployApplyResult)` tripped `clippy::large_enum_variant` once the report was embedded. Boxing the field keeps the serialized JSON identical.

### Added beyond the plan

- `backup_sample_failure_aborts_before_any_write` (V5): an unreachable backup at the sample ends the run with its own error, with the collection unchanged, no lock, no scan and only `probe` on the host.
- `PairLink::{tamper, keep, refuse_resync, resyncs, timestamp}` and `FakeGateway::{with_pair, reload_disk, config_lock_held}`.

## Measured duration of the backup test cases

`cargo test -p ignition-core --test deploy_apply backup_` runs 24 tests in parallel (18 `backup_*` cases of this plan plus the 3 older `preflight_backup_*`/`no_backup_node` cases and the Task 3 cases) in 23 s wall clock (machine load average 30 to 50), 12.8 s to 17.4 s on earlier subsets. The longest single cases are the two resync-failure cases (they spend two 1 s and two 0.3 s budgets) and `backup_interrupt_leaves_master_applied` (about 0.15 s of wait before the drop). The whole `deploy_apply` binary (106 tests) runs in about 29 s.

## Verification

- `cargo test -p ignition-core --test deploy_apply backup_` 24 passed; `--test deploy_apply` as a whole 106 passed (every Phase 23 test unchanged).
- `cargo test -p ignition-core --lib error` 41 passed (`DEPLOY_SLUGS` pin still 12); `grep -c deploy_backup_unsynced crates/ignition-core/src/error.rs` prints 0.
- `grep -n redundancy_resync crates/ignition-core/src/deploy/apply/converge.rs` prints one line, `self.ctx.deploy.redundancy_resync()` (the plan's grep expects `ctx.deploy.redundancy_resync()`; the `self.` prefix is the same call).
- `cargo test -p ignition-core --test deploy_transport pair_module_never_writes` passes.
- `cargo test -p ignition-core --test live_deploy_apply --no-run` builds.
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- `cargo test --workspace --no-fail-fast`: every test binary passes (76 binaries ok) except `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`, the pre-existing load flake (machine load average 30 to 50 from other processes; the test asserts a Drop-path cleanup reached a wiremock server within 10 s and touches no Phase 24 code; see `deferred-items.md`). Not fixed here.

## Known Stubs

None.

## Threat Flags

None. T-24-19 (no branch of `converge_backup` calls `roll_back` or `restore_snapshot`; the unsynced tests assert record `applied`, no restore events and the master equal to the render), T-24-20 (one call site for the resync, the record set once, `ForbidWrites` on the backup, exactly one attempt counted in the failure and refusal tests), T-24-21 (content checks in the final evaluation; `backup_unsynced_content_differs`, `backup_files_differ`, `backup_timestamp_frozen`), T-24-22 (two bounded budgets, early exit on exhaustion, interrupt test) and T-24-23 (events carry flags, counts and codes) are covered by tests.

## Requirements

DPAIR-01 and DPAIR-02 are not marked complete: this plan ships the engine; the preflights (24-07), the CLI flag and README (24-08), verify (24-09) and the live gates (24-10) follow.

## Self-Check: PASSED

Created `crates/ignition-core/src/deploy/apply/converge.rs` and the seven commits (be7c47d, ced2730, e1173c2, 4216838, 35b8200, 6d93c6d) exist.
