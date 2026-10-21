---
phase: 23-guarded-apply-rollback
plan: 17
subsystem: deploy
tags: [apply-engine, restore, rollback, scan-lock, held-lock, fault-injection, fake-gateway, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-05 run_apply, HeldLock, HoldModel, the single exit point; 23-04 ApplyHost; 23-19 ApplyFault and FaultyHost; 23-03 RunRecord, journal_location; 23-02 vocabulary (option-a)
provides:
  - restore_snapshot, RestoreInput and RestoreReport, the one restore path for abort, auto-rollback and (from 23-11) manual rollback
  - every post-lock failure branch of run_apply as a restore or a precise refusal; no CoreError::Internal is left after the lock is taken
  - HeldLock::check_gateway, HeldLock::wait_for_lapse and lock identity (another actor's hold reads as not held)
  - FakeGateway::stall_scans
  - ApplyEvent::VerifyChecks and a public Steps with a constructor
affects: [23-11, 23-12, 23-13, 23-15, 23-16, 23-21]

actuals:
  tokens: 16500
  tasks: 3
  commits: 6

tech-stack:
  added: []
  patterns:
    - "a scenario table: each fault is an async fn returning a Ran (rig, state before, scan counts at restore, result), so a per-fault test and the never_applied_when_red sweep run the same code"
    - "restore reads in a fixed order: lock, one restore call, one scan, then the hash check against the record's pre-images; the verify verb is never used to judge a restore"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/apply/hold.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/deploy_apply.rs

key-decisions:
  - "HeldLock remembers which hold is ours (timestamp and actor from one read right after the grant) so that a lock another actor took after ours lapsed reads as NotHeld; without it the post-commit check and the restore both mistook a foreign hold for ours (DAPL-07 concurrency)"
  - "restore_snapshot reuses the held lock on the gateway's word alone: the local clock can only read shorter than the gateway, and the plan's local-clock test requires the reuse"
  - "an Err from ApplyHost::commit (a refusal that wrote nothing) stays as it was in 23-05: its own typed error, the lock left to lapse; a scan there could load another run's state"
  - "plan_stale carries the plan hash re-read after the lock was released; if the re-plan fails the field is the word unavailable"
  - "a rolled-back run records rolled_back_by as its own run id; 23-11 passes the rollback run's id through RestoreInput"

patterns-established:
  - "an abort drops this run's staging with discard_interrupted, best effort, and never lets a failed cleanup hide the reason for the refusal"

requirements-completed: []
requirements-advanced: [DAPL-01, DAPL-05, DAPL-06, DAPL-07]

duration: ~1h (most of it cargo builds on a machine at load average 900)
completed: 2026-10-16
---

# Phase 23 Plan 17: The one restore path and every abort and auto-rollback branch Summary

**At library level an apply now ends verified or restored and scanned through a single `restore_snapshot`; a changed pre-image or a competing lock refuses with nothing moved; a restore that cannot finish is `rollback_failed` with the journal kept.**

## `restore_snapshot` step order as implemented

`crates/ignition-core/src/deploy/apply/mod.rs`, `restore_snapshot` then `restore_steps`. Any failure of any step is `CoreError::RollbackFailed` with the run id, the run directory, the on-box journal location (`journal_location`) and a value-free cause; the journal stays on the box.

1. event `restore_started` (cause).
2. the lock. With a held lock: `HeldLock::check_gateway(need)` where `need` is `ceil(estimate_commit_s(touched)) + LOCK_MARGIN_S`. `Held` reuses it (no second acquire, C1d). `Expiring` (ours, too short) waits with `wait_for_lapse`, then takes a fresh hold. `NotHeld` (free, or another actor's) takes a fresh hold at once, event `lock_acquired`. Without a held lock a fresh hold is taken. A refused acquire (for example `scan_lock_held`) is the failure cause.
3. exactly one `ApplyHost::restore`; a report with `failure` or `stale` is a failure.
4. exactly one `scan_and_wait` on the config scan, only now (Pitfall 1); `NotComplete` is a failure; event `scan_done`.
5. the restore check (Pitfall 9, never `deploy_verify`): `ApplyHost::fingerprint` of the touched set equals `RunRecord::pre_map()` (a mismatch names up to ten paths), and `deployment_mode()` equals the record's mode.
6. the record to `rolled_back` (`rolled_back_by` is the restoring run's id, `post_fingerprint` the pre fingerprint), `ApplyHost::finalize` (installs it, removes the journal and the run's staging), event `restored`, then `fetch_snapshot` into the run directory (a failure is the warning `snapshot_download_failed (<code>)`).

## Each abort and the test that proves it

| Abort | Ends as | Test |
|---|---|---|
| commit fails after moving files | `rolled_back`, `write_failed`, lock reused, one scan | `fault_write_failed_mid_commit_restores`; `fault_rerun_after_restore_plans_the_same` (edge DAPL-06 idempotency) |
| lock lapsed by the gateway | `rolled_back`, `lock_expired`, scan count 0 when `restore` returns, fresh hold | `fault_lock_expired_before_scan_restores_then_scans` |
| local clock past the hold, gateway large | `rolled_back`, `lock_expired`, one acquire only | `fault_local_clock_expired_restores_under_held_lock` |
| local clock past the hold, gateway short | waits for the lapse, fresh hold | `fault_short_remaining_waits_for_the_lapse_then_restores_under_a_fresh_lock` (not in the plan) |
| scan does not complete | `rolled_back`, `scan_incomplete`, two scans (stalled, restore's own) | `fault_scan_incomplete_restores` |
| pre-image changed under the lock | `plan_stale` (exit 6), nothing moved, one no-op scan, `result.json` holds the error | `fault_pre_image_changed_is_plan_stale` (edges DAPL-01 and DAPL-05 concurrency) |
| another actor holds the lock | `scan_lock_held` (exit 6), no collection write, only this run's staging removed, no scan | `fault_competing_lock_is_scan_lock_held` (edge DAPL-06 concurrency) |
| master verify red | `rolled_back`, `verify_failed`, checks in the log by id and status | `auto_rollback_verify_red_restores` |
| restore call fails | `rollback_failed` (exit 6), journal stays, files untouched | `auto_rollback_restore_failure_is_rollback_failed` |
| another actor takes the lock after ours lapsed | `rollback_failed` naming `scan_lock_held`, the restore never runs, no scan, journal stays | `auto_rollback_lock_taken_is_rollback_failed` (edge DAPL-07 concurrency) |
| restored files differ from the pre-images | `rollback_failed` naming the path, journal stays | `auto_rollback_restore_check_mismatch_is_rollback_failed` |
| any of the eleven scenarios | never `applied`, never `ok`, exit 6 | `never_applied_when_red` (prohibition, T-23-21) |

No `CoreError::Internal` branch from 23-05 remains on an abort path. The two `Internal` left in `mod.rs` are pre-lock defects (a write set that does not match the plan, a batch the engine built invalid).

## Task Commits

1. **Task 1 (tracer), RED:** `a43cd11`
2. **Task 1 (tracer), GREEN:** `2d55a64` (`restore_snapshot`, `HeldLock` identity, the `write_failed` branch)
3. **Task 2, RED:** `b881106` (`stall_scans`, six fault rows)
4. **Task 2, GREEN:** `307ad53` (lock expiry, scan incomplete, stale pre-image, refused lock)
5. **Task 3, RED:** `7a0555b` (scenario table, four auto-rollback rows, `never_applied_when_red`)
6. **Task 3, GREEN:** `4798fc3` (red verify branch, `verify_checks` event, module doc)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active (`workflow._auto_chain_active` and `workflow.auto_advance` are false), so the interactive rule asks for a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase`, the orchestrator directed execution to skip mid-plan human-verify checkpoints, and the slice is a library engine over a fake gateway with no human-verifiable surface. Execution continued. The tracer's `<verify>` (`cargo test -p ignition-core --test deploy_apply -- fault_write_failed_mid_commit_restores fault_rerun_after_restore_plans_the_same`) passed before `2d55a64` and again after Task 3. Skipped checkpoint recorded for the end-of-phase list: none beyond this one.

## Test results

- `cargo test -p ignition-core --test deploy_apply`: 24 passed (5 gate rows, the 23-05 engine rows, 12 `fault_`/`auto_rollback_`/`never_applied_when_red` rows).
- `cargo test -p ignition-core --lib deploy::apply::hold`: 15 passed (four new: another actor's hold, an unseen grant, `check_gateway` ignores the local clock, `wait_for_lapse`).
- `cargo test --workspace --no-fail-fast`: 2344 passed, 0 failed. The Docker daemon was up, so the four `contract_rig.rs` rows ran and passed.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] `HeldLock` identifies its own hold**
- **Found during:** Task 1 design, against the plan's edge DAPL-07 test and its local-clock test
- **Issue:** the two tests are indistinguishable by the gateway's `remaining` and the local clock alone: a foreign hold with a large `remaining` after ours lapsed looks the same as our own hold with the local clock ahead. With 23-05's `check` the engine would also have scanned and reported `applied` when another actor held the lock after ours lapsed.
- **Fix:** `acquire` reads the lock once right after the grant and keeps its `timestamp` and `actor` (C1c and D9: the timestamp is stable across reads of one hold and differs for each new hold). A later reading with another identity is `NotHeld`. The identity is only compared with an earlier reading of the same gateway, never with a clock (Pitfall 15 holds). New `check_gateway` (gateway side only) and `wait_for_lapse`; `check` is built on them. `acquire` now costs one extra GET.
- **Files modified:** `crates/ignition-core/src/deploy/apply/hold.rs`
- **Commit:** `2d55a64`

**2. [Rule 3 - Blocking] `restore_snapshot` takes a `RestoreInput`, and `Steps` is public**
- **Issue:** the plan's positional `(ctx, run_id, record, held, opts)` plus the cause and the event sink is eight parameters, over clippy's limit under `-D warnings`; `Steps` was private but `restore_snapshot` is public.
- **Fix:** `RestoreInput { run_id, run_dir, record, held, cause }`; `Steps` is public with `Steps::new` and `emit`.
- **Commit:** `2d55a64`

**3. [Rule 2 - Missing critical functionality] A `verify_checks` event**
- **Issue:** the plan asks for the red verdict's checks in `events.ndjson`, ids and statuses only, and `VerifyDone { ok }` cannot carry them without breaking its pattern in the 23-05 tests.
- **Fix:** a new `ApplyEvent::VerifyChecks`, emitted only on a red verdict. The green event list is unchanged.
- **Commit:** `4798fc3`

### Plan departures, not rule-driven

- The plan names seven `fault_` tests; there are eight (`fault_short_remaining_waits_for_the_lapse_then_restores_under_a_fresh_lock` covers the wait-for-lapse branch of `restore_snapshot`, which the seven did not reach). `never_applied_when_red` sweeps eleven scenarios (the seven fault scenarios, the rerun being no failure of its own, plus the verify, restore-failure, lock-taken and check-mismatch cases).
- The 23-05 rows `a_commit_that_fails_after_moving_is_never_applied`, `a_lock_that_lapsed_before_the_scan_is_never_applied` and `a_red_verify_is_never_applied` are replaced by their new counterparts; they asserted the `Internal` the plan removes.
- `FakeGateway::stall_scans(n)` is "accepted and never advancing" (the POST counts and releases the lock, nothing loads, `lastScanTimestamp` stays), not "the status stays active": a permanently active scan would also block the restore's own scan at its first phase.
- `requirements-completed` is empty. DAPL-01, DAPL-05, DAPL-06 and DAPL-07 are user-visible apply behaviour; this plan lands the library engine over a local node and a fake gateway only. The verb is 23-13, the Windows and Linux hosts' own rows are 23-06 and 23-20, the live gates are 23-16. `requirements mark-complete` was not run.

## Auth Gates

None.

## Known Stubs

None. `grep` for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and skipped tests over the changed files finds nothing.

## Threat Flags

None beyond the plan's register. T-23-19 (partial tree loaded after a lapsed lock) is mitigated and tested by the scan-count-at-restore assertions; T-23-20 (foreign edit between plan and write) by `fault_pre_image_changed_is_plan_stale`; T-23-21 (red reported as success) by `never_applied_when_red`; T-23-22 (wrong oracle) by `auto_rollback_restore_check_mismatch_is_rollback_failed`.

## Open points for later plans

- **23-11 (manual rollback):** `RestoreInput.cause` is a `RollbackCause`, which has no manual value, and `ApplyEvent::RestoreStarted` carries it. 23-11 either adds a variant (and the README vocabulary, four-place rule if it becomes an exit-visible slug) or makes the field optional.
- **23-06 and 23-20 (ssh hosts):** the engine treats an `Err` from `ApplyHost::commit` as "wrote nothing" and leaves the lock to lapse. Over ssh a dropped connection mid-commit is also an `Err`, with the state unknown. The on-box journal covers it for 23-12's recovery, but the engine does not roll back on that `Err`; worth a decision before the ssh hosts are exposed by the verb.
- **23-12:** a refused-lock abort and a stale abort call `discard_interrupted` best effort; a failure to clean the staging is silent (the staging is the documented manual cleanup, D-08).
- The hold used by the restore's fresh lock is `opts.hold.hold_s`, the same as the apply's; 23-10 supplies the measured model.

## Live-rig note

No live step was needed (library level, local node, fake gateway). The local rig was not touched and no trial reset was run.

## Self-Check: PASSED

- Modified files exist: `crates/ignition-core/src/deploy/apply/mod.rs`, `crates/ignition-core/src/deploy/apply/hold.rs`, `crates/ignition-core/tests/common/fake_deploy.rs`, `crates/ignition-core/tests/deploy_apply.rs`, this SUMMARY.
- Commits `a43cd11`, `2d55a64`, `b881106`, `307ad53`, `7a0555b`, `4798fc3` exist in `git log`.
- `grep -c 'fn restore_snapshot' crates/ignition-core/src/deploy/apply/mod.rs` prints 1.
