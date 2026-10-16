---
phase: 23-guarded-apply-rollback
plan: 05
subsystem: deploy
tags: [apply-engine, scan-lock, hold, held-lock, noop, fake-gateway, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-04 ApplyHost and the local host; 23-19 FaultyHost and ApplyFault; 23-02 gates and checkpoint outcome; 23-03 run store, journal, record, fingerprint
  - phase: 22-transport-plan-pull-verify
    provides: deploy_plan, deploy_verify, TreeReader, Ledger, the OQ5-2 formula
provides:
  - run_apply, the apply engine's green path and its empty-plan no-op, over any ApplyHost
  - write_set, the whole-unit write set with mode-collection pre-images
  - deploy_apply_prepare and deploy_apply_execute, the thin verbs, and DeployApplyResult with failure_exit_code
  - HeldLock (two-clock lock judgement), Clock, SystemClock, ManualClock, HoldModel
  - D-02's hold rule as code: HOLD_FLOOR_S 10, HOLD_CEILING_S 300, unclamped hold_for, separate hold_clamped, fits
  - FakeGateway::with_data_dir, scan_count and lock_acquires
affects: [23-17, 23-07, 23-09, 23-10, 23-11, 23-12, 23-13, 23-15, 23-16]

actuals:
  tokens: 20600
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "one exit point: once the run directory exists, run_apply wraps an inner drive() and writes result.json exactly once, whatever drive returned"
    - "events are value-free (ids, counts, flags); one ApplyEvent value feeds both events.ndjson and the observer"
    - "the fake gateway is coupled to a data directory and reloads the mode collection only at a config scan trigger (C2c, C2d and C2e: a file dropped without a scan is never loaded, lock or no lock)"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/hold.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/deploy_apply.rs

key-decisions:
  - "A scan runs once (C11 in 23-LIVE-CAPTURES Decisions: a tag provider and its definitions load in one scan), so there is no second-scan branch"
  - "A host refusal that wrote nothing (Err from prepare or commit) is returned as its own typed error, with the engine filling the env of deploy_journal_present; only a stale pre-image and a commit that failed after moving files are Internal. The plan's 'a failed commit' read as the second"
  - "No lock release and no host discard in any failing branch of this plan: a scan over a partial tree would load it, and 23-17 owns the aborts"
  - "A snapshot download failure and an unwritable result.json are warnings, never a rollback"
  - "run.json records the --allow-drift reason text (private 0600 run directory, 500 characters at most) so the audit trail of a drift override survives"

patterns-established:
  - "an engine test builds a Rig: tracer repo over a local data directory, a data-dir-coupled fake gateway, wiremock for the log source, and a local host behind FaultyHost"

requirements-completed: []
requirements-advanced: [DAPL-01, DAPL-03, DAPL-06]

duration: ~2h50m (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 05: The apply engine's green path, the no-op and D-02's hold layer Summary

**At library level over a local node a declared change now goes from plan to a verified, finalized deploy under a two-clock-judged scan-lock, an empty plan touches nothing, and the hold layer carries D-02's formula, floor and ceiling with no measured value.**

## `run_apply` step order as implemented

`crates/ignition-core/src/deploy/apply/mod.rs`, `run_apply` then `drive`:

0. empty plan (`summary.has_changes` false): return `noop` before anything is touched.
1. `RunDir::create_fresh(repo, gateway)` (the git-ignore guard is inside it; `not_a_repo` is allowed and recorded).
2. `run.json`, `plan.json` written, `events.ndjson` opened.
3. `write_set` (whole units of `added` and `changed`, the delete set, mode-collection pre-images), then `RunStarted`.
4. the old ledger through `TreeReader::read_ledger`, the new ledger (old minus deletes plus written, byte-sorted, this run's `render_sha256` and `run_id`) validated by `Ledger::parse(to_bytes())`.
5. `BatchSpec` (journal and record built to describe exactly the batch) checked with `BatchSpec::problem()`.
6. `ApplyHost::prepare` outside the lock, then `Staged`.
7. `HeldLock::acquire(Config, hold_s)`, then `LockAcquired`.
8. exactly one `ApplyHost::commit`, then `Committed`.
9. `HeldLock::check(LOCK_MARGIN_S)`, must be `Held`, then `LockVerdict`.
10. `scan_and_wait(Config)` once, then `ScanDone`.
11. `deploy_verify`, must be green, then `VerifyDone`.
12. `ApplyHost::fingerprint` over the touched paths, post fingerprint, record to `applied`, `ApplyHost::finalize`, then `Finalized`.
13. `ApplyHost::fetch_snapshot` into `<run dir>/snapshot/` (a failure is the warning `snapshot_download_failed (<code>)`).
14. the single exit point writes `result.json` once (`{"ok": false, "error": {code, message}}` for an error); an unwritable `result.json` on a success becomes the warning `result_json_unwritable`.

## `ApplyEvent` names

`run_started` (`run_id`, `writes`, `deletes`), `staged` (`files`), `lock_acquired` (`hold_s`), `committed` (`moved`, `deleted`), `lock_verdict` (`held`), `scan_done`, `verify_done` (`ok`), `finalized`, `restore_started` (`cause`), `restored`, `failed` (`code`). `restore_started` and `restored` are for 23-17 and are not emitted yet. `failed` is emitted by the exit point on any error.

## Branches left as `CoreError::Internal` for 23-17 to replace

Each message names its cause in parentheses; none is `applied`.

| Branch | Message cause | 23-17 replaces it with |
|---|---|---|
| `BatchReport.stale` (pre-image mismatch under the lock, nothing moved) | `stale_pre_image` | the no-op scan to release the lock (D12), then `plan_stale` |
| `BatchReport.failure` (commit stopped after moving files) | `write_failed` | `restore_snapshot`, outcome `rolled_back` |
| lock not `Held` with the margin after the commit | `lock_expired` | `restore_snapshot`, outcome `rolled_back` |
| `ScanOutcome::NotComplete` | `scan_incomplete` | `restore_snapshot`, outcome `rolled_back` |
| red `deploy_verify` | `verify_failed` | `restore_snapshot`, outcome `rolled_back` |

Also left to 23-17, not an `Internal` but an unhandled cleanup: a refused lock acquire (for example `scan_lock_held`) returns its own typed error and leaves `staging/<run>` on the host (no `discard_interrupted` call); a typed host refusal from `commit` leaves the lock to lapse. Both are deliberate: a scan or a discard here could load or remove another run's partial state.

## Accomplishments

- `write_set`: pre-image is the plan's `live_sha256` only where `live_in` contains `mode`; a `core`-only copy is "absent in the mode collection". A delete without such a hash, or a rendered file whose bytes differ from the plan's hash, is `Internal`.
- `HeldLock` takes its `Instant` before the acquire request, judges by the gateway's `remaining` (`lock_verdict`) and by the local count (rounded to err short); the local clock wins when it is shorter. It never reads `timestamp` or `lastScanTimestamp`.
- `deploy_apply_prepare` validates the flags before any host contact, re-plans (`deploy_plan`, host read only) and gates; `ApplyPrepared::Noop` is a final `DeployApplyResult` with outcome `noop`.
- `DeployApplyResult::failure_exit_code` is `Some(6)` exactly when the outcome is `rolled_back` (not reachable until 23-17).
- The 23-04 carry-over is done: `DeployJournalPresent` raised by the host gets the real `--env` from the engine (test `a_journal_of_another_run_names_the_real_env`).
- D-02 in `hold.rs`: `hold_for` is unclamped, `hold_clamped` is separate, 505 s and 699 s appear only as formula regression values in unit tests, and `grep -vE '^[[:space:]]*//' hold.rs | grep -cE 'const (HOLD_S|MEASURED|P95_[A-Z0-9_]+):'` prints 0.

## Task Commits

1. **Task 1 (tracer), RED:** `11344c6` (the fake's `with_data_dir`, type and stub skeletons, the engine rows; nine failed on `Internal("... not built")`)
2. **Task 1 (tracer), GREEN:** `e1b025d` (`run_apply`, `write_set`, `HeldLock`, the verbs)
3. **Task 2, RED:** `dff4c68` (hold formula, fit and `ManualClock` stubs; five rows failed)
4. **Task 2, GREEN:** `0a21905` (the formula, clamp, fit and `ManualClock`)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active (`workflow._auto_chain_active` and `workflow.auto_advance` are false), so the interactive rule asks for a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase`, the run is a sequential executor under `mode: yolo`, and the slice has no human-verifiable surface (a library engine over a fake gateway), so, as in 23-01, 23-02, 23-04, 23-18 and 23-19, execution continued. The tracer's `<verify>` (`cargo test -p ignition-core --test deploy_apply -- apply_happy_path_local empty_plan_is_a_noop`) passed before the tracer GREEN commit `e1b025d` and again after Task 2 (the full file, 14 passed).

## Test results

- `cargo test -p ignition-core --test deploy_apply`: 14 passed (the 5 gate tests, `apply_happy_path_local`, `empty_plan_is_a_noop`, `stale_hash_stops_before_any_host_write`, `write_set_pre_images_follow_where_the_live_copy_is`, `apply_deletes_what_the_ledger_lists_and_rewrites_the_ledger`, `a_commit_that_fails_after_moving_is_never_applied`, `a_lock_that_lapsed_before_the_scan_is_never_applied`, `a_red_verify_is_never_applied`, `a_journal_of_another_run_names_the_real_env`)
- `cargo test -p ignition-core --lib deploy::apply::hold`: 11 passed (`hold_formula_regression`, `hold_floor`, `hold_fit_boundary`, `held_lock_local_clock_expiry`, `held_lock_gateway_expiring` and six more)
- `cargo test --workspace --no-fail-fast`: 2300 passed, 0 failed
- `cargo clippy --workspace --all-targets -- -D warnings`: clean; `cargo fmt --all -- --check`: clean
- Acceptance greps: `pub fn failure_exit_code` prints one line in `actions/deploy_apply.rs`; the no-measured-constant grep prints 0.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] The engine fills the env of `deploy_journal_present`**
- **Found during:** Task 1 (the 23-04 carry-over in the task brief)
- **Issue:** a host cannot know `--env`, so its `DeployJournalPresent` carries the placeholder `"<env>"` and the recovery command in the message would be wrong.
- **Fix:** `with_env` replaces the field on any host error from `prepare` and `commit`.
- **Files modified:** `crates/ignition-core/src/deploy/apply/mod.rs`
- **Commit:** `e1b025d`

**2. [Rule 2 - Missing critical functionality] Four tests beyond the plan's two for Task 1**
- **Found during:** Task 1 design (the truths list branches that "end in `CoreError::Internal`" and a write set with deletes, none of which the two named rows touch)
- **Fix:** rows for the write set's pre-images, the delete set and ledger arithmetic, the three post-lock Internal branches (failed commit, lapsed lock, red verify), the stale-hash early stop and the journal env. The plan's `empty_plan_is_a_noop` also executes the no-op to prove it stays inert.
- **Files modified:** `crates/ignition-core/tests/deploy_apply.rs`
- **Commit:** `11344c6`

### Plan departures, not rule-driven

- The plan says "a failed commit" ends in `Internal`. A host `Err` (a refusal that wrote nothing: journal present, an invalid spec) is returned as its own typed error rather than wrapped, since wrapping would lose `deploy_journal_present` and its recovery command. Only `BatchReport.stale` and `BatchReport.failure` are `Internal`.
- `ApplyOptions` carries `base` and `gates` as the plan specifies; `run_apply` does not re-plan, so it only records the gate flags in `run.json`. The plan the gates passed is the one applied; the host's pre-image check under the lock is what catches a change since.
- `DeployApplyResult` also carries `summary` (the plan's counts) next to the plan's field list.
- `requirements-completed` is empty. DAPL-01, DAPL-03 and DAPL-06 describe user-visible apply behaviour; this plan lands the library engine only, the abort and rollback paths are 23-17 and the verb is 23-13. `requirements mark-complete` was not run, as in 23-01, 23-02, 23-04 and 23-19.

## Auth Gates

None.

## Known Stubs

None. `grep` for `todo!`, `unimplemented!`, `TODO`, `FIXME`, `not built` and ignored tests over the changed files finds nothing. The `Internal` branches above are deliberate refusals to report `applied`, not stubs; they are pinned by tests.

## Threat Flags

None beyond the plan's register. T-23-23 is mitigated and tested (`held_lock_local_clock_expiry`, `held_lock_gateway_expiring`, `held_lock_never_reads_the_gateway_timestamp`). T-23-69 is mitigated: `hold_for` is unclamped, `hold_clamped` is separate, and the grep gate shows `hold.rs` holds no measured constant. T-23-70 is mitigated and tested (`a_commit_that_fails_after_moving_is_never_applied`, `a_lock_that_lapsed_before_the_scan_is_never_applied`, `a_red_verify_is_never_applied`). New surface worth noting: `run.json` stores the `--allow-drift` reason text in the 0600 run directory.

## Live-rig note

No live step was needed (the plan is library level over a local node and a fake gateway). The local rig (compose project `ign-deploy-rig`, port 18290) was not touched and no trial reset was run.

## Next Phase Readiness

- 23-17 replaces the table above, adds `restore_snapshot`, `stall_scans` and the abort cleanups, and flips `OutcomeKind::RolledBack` and `RollbackCause` on.
- 23-07 builds `hold_budget` on `fits` / `HoldModel::fits`; 23-09 adds `.gwbk` and restart-pending to `drive`; 23-12 adds the journal refusal before the plan.
- 23-10 Task 2 adds the measured constants and `HoldModel::MEASURED` to `hold.rs`; 23-10 Task 3 option-a is the only path that changes `HOLD_CEILING_S`, and it updates `hold_formula_regression`'s clamp assertions in the same commit.
- 23-13 names `HoldModel::MEASURED`, so the verb cannot compile before 23-10.

## Self-Check: PASSED

- Created files exist: `crates/ignition-core/src/deploy/apply/hold.rs`, `crates/ignition-core/src/actions/deploy_apply.rs`, this SUMMARY.
- Commits `11344c6`, `e1b025d`, `dff4c68`, `0a21905` exist in `git log`.
- `grep -n 'pub fn failure_exit_code' crates/ignition-core/src/actions/deploy_apply.rs` prints one line.
