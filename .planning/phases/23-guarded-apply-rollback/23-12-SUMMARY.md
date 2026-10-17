---
phase: 23-guarded-apply-rollback
plan: 12
subsystem: deploy
tags: [journal, recovery, rollback, interrupted-run, scan-lock, fault-injection, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-11 run_rollback, check_rollback, RollbackContext; 23-17 restore_snapshot and the commit-Err carry-over; 23-03 Journal, RunRecord, fingerprint; 23-04 ApplyHost (fingerprint, restore, finalize, read_record, fetch_snapshot, discard_interrupted); 23-18 TreeReader::read_journal; 23-19 ApplyFault::HangAfterMove, FaultyHost; 23-02 option-a vocabulary
provides:
  - journal refusal in deploy_plan (so also in deploy_apply_prepare) before the collections are read
  - recovery of an interrupted run through rollback --run <journal's run id>: restore (D-11, no staleness check, no flag) or cleared (D-10)
  - RecoveryAction, recovery_action, journal_location (re-export), journal_on_box, journal_present (deploy/apply/journal.rs)
  - RollbackKind::Cleared, RollbackChecked.interrupted / .record (now optional), InterruptedRun, RollbackPrepared.run_id / .interrupted
  - ApplyEvent::InterruptedRun, ApplyEvent::Cleared, ApplyEvent::CommitInterrupted
  - FaultyHost::fail_after (test harness)
affects: [23-13, 23-14, 23-15, 23-16, 23-20, 23-21]

actuals:
  tokens: 36000
  tasks: 2
  commits: 6

tech-stack:
  added: []
  patterns:
    - "one journal read (journal_on_box) answers plan, apply and rollback; the refusal is built in one place (journal_present)"
    - "a recovery reads and hashes the snapshot into a scratch directory below its own run directory before it restores anything, so a lost snapshot is rollback_failed with nothing half restored"
    - "recovery never depends on the record's post part: a lost record is rebuilt from the journal's touched list and the snapshot, and the journal's pre_fingerprint is the integrity check"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/apply/journal.rs
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/common/faulty_host.rs

key-decisions:
  - "The run's own journal is not a refusal in check_rollback: it selects the recovery. Any other run's journal, and any journal that cannot be read or parsed, still refuses (deploy_journal_present / deploy_run_invalid). Nothing else recovers or discards a journal (D-09, D-12)."
  - "D-10: clear only when the phase is snapshotting or ready AND the live fingerprint of the journal's touched list equals pre_fingerprint; every other journal restores, including a before-the-first-move journal whose touched set a foreign edit changed (that edit is overwritten by the snapshot, as D-10 says)."
  - "The snapshot is read before the restore (fetch into <run dir>/inspect, hashed, removed): with a record each listed pre-image must be present with its hash; without one the snapshot must hash to the journal's pre_fingerprint. A failure is rollback_failed (journal kept)."
  - "A commit Err is reported as deploy_journal_present when this run's own journal is on the box (see Carry-overs); no slug, outcome or exit code was added."

patterns-established:
  - "recovery refuses to guess: no restore on an ambiguous commit error, only on a journal read back from the box"

requirements-completed: []
requirements-advanced: [DAPL-09]

duration: ~2h (most of it cargo builds and the workspace test run)
completed: 2026-10-16
---

# Phase 23 Plan 12: Interrupted-run recovery Summary

**A leftover journal now blocks `deploy plan`, `apply` and `rollback --run <other>` with the exact recovery command, and `rollback --run <the journal's run>` recovers it: it restores the on-box snapshot when anything may have moved, or removes the run as `cleared` when it provably died before touching the tree.**

## What was built

- **`journal.rs`**: `journal_on_box` (read and strictly parse the journal, `deploy_run_invalid` naming the location when it cannot be read or parsed), `journal_present` (the `deploy_journal_present` refusal with run, env and location), `RecoveryAction` and `recovery_action` (D-10), `JournalPhase::as_str`, `journal_location` re-exported (it already lived in `deploy/ledger.rs`, where 23-18 put it; the plan's artifact list names it here).
- **`deploy_plan`** reads the journal after the `--base` check (a bad base still costs no transport work, 22-04) and before the collections are read. `deploy_apply_prepare` inherits it, so the journal is checked before the plan, as 23-02 item 7 orders.
- **`check_rollback`**: a journal of another run refuses; the run's own journal returns a `RollbackChecked` with `interrupted: Some(InterruptedRun { journal, live, action })`. The post-apply staleness check is skipped (D-11), `--discard-live-changes` is not needed and is ignored. When the record is present it must agree with the journal (touched list and pre fingerprint) and the local run directory; it may be at any state, or absent.
- **`run_rollback`** for a recovery: `interrupted_run` event first (phase, action, live hash of every touched path, D-11), then:
  - `clear`: `run.json` and `plan.json`, `ApplyHost::discard_interrupted`, `cleared` event, outcome `cleared`. No lock, no preflights, no restore, no scan.
  - `restore`: the snapshot is fetched into `<run dir>/inspect`, hashed and removed (`snapshot_record`); then the usual preflights, scan-lock, `restore_snapshot`, which finalizes (journal and the run's staging removed, record `rolled_back`).
- **`deploy_rollback_prepare`** gives the recovery its own confirmation prose (three variants: clear, restore with a record, restore with the record lost); `RollbackPrepared` carries `run_id`, an optional `record` and `interrupted`.

## Carry-overs resolved

| Carry-over | Decision |
|---|---|
| 23-11: the own-run journal bounced a user between the error message and the refusal | Closed: `rollback --run <id>` of the journal's run is the recovery (`journal_kill_mid_write_then_recover`). |
| 23-17 / ssh: `ApplyHost::commit` `Err` treated as "nothing written", lock left to lapse | Assessed: yes, the journal stays on the box in that case (a host only removes it on a clean abort before the first move), so `rollback --run <id>` recovers it. The engine now looks for its own run's journal after an `Err` (`commit_failed`): when it is there the run is `deploy_journal_present` (existing slug, exit 6, the message carries the exact recovery command) and a `commit_interrupted` event keeps the original error code. It restores nothing on the guess, because over ssh the remote script may still be running; the recovery path reads the snapshot before it writes. With no journal of this run (or an unreadable box) the original error stands. Tests: `journal_commit_error_after_moves_reports_the_interrupted_run` (a `FaultyHost` that runs the inner commit, which moved one file then failed, and then errs; recovery then restores) and `journal_commit_error_with_nothing_on_the_box_stays_the_original_error`. No new slug, outcome or exit code was needed, so nothing is recorded under "Open decision". One thing the user may want to confirm: the apply then reports `deploy_journal_present` where it used to report the transport error; the transport error's code is only in `events.ndjson` (`commit_interrupted`), not in the envelope. |
| Option-a vocabulary | `restored`, `cleared`; slugs `deploy_journal_present`, `deploy_run_invalid`, `rollback_failed`. No slug was added, the four-place rule was not triggered. |
| No path traversal or symlink following | Journal and record reads go through the existing hardened `read_journal` / `read_record` (no link followed); the run id is parsed before any path is built (23-11); the scratch directory is a fixed name below the recovery's own run directory; the local run-directory cross-check reads with `symlink_metadata` (23-11). |

## Tests

| Behaviour | Test |
|---|---|
| a real apply aborted after its first move; plan, apply and a rollback of another run refuse with the exact command and write nothing; `rollback --run` restores, scans, removes journal and staging; the plan hash is the pre-apply one; the torn hashes are in the events | `journal_kill_mid_write_then_recover` |
| garbage journal blocks plan, apply and rollback with `deploy_run_invalid` | `journal_corrupt_blocks` |
| D-10: `ready` and `snapshotting` with an untouched tree are `cleared`; no restore, finalize, lock or scan | `journal_died_before_move_is_cleared` |
| D-10 negative: before the first move but the tree differs: restores | `journal_before_move_but_changed_restores` |
| D-12: a snapshot file lost: `rollback_failed` naming `.gwbk`, the run-directory snapshot, the journal path and the file; journal kept; no restore call | `journal_snapshot_lost_is_rollback_failed` |
| Pitfall 14: journal at `written`, record `applying` (finalize hung and killed): files and ledger restored | `journal_without_record_post_part_recovers` |
| Pitfall 14: record lost: rebuilt from journal and snapshot; an incomplete snapshot with no record is `rollback_failed` | `journal_without_record_recovers` |
| A9: killed between a green verify and finalize: restores | `journal_after_green_verify_restores` |
| edge DAPL-09 idempotency: second recovery is `deploy_run_invalid` ("already rolled back by", or no record after a clear), zero writes | `journal_recover_twice_refuses` |
| `recovery_action` for every phase, live equal and different | `recovery_action_clears_only_an_untouched_run` (lib) |
| carry-over: commit `Err` over a leftover journal; commit `Err` over a clean box | the two `journal_commit_error_*` tests |

TDD sequence: RED `fc9467c` then GREEN `baae5c3` (Task 1, tracer); RED `3811f80` then GREEN `1ba9f35` (Task 2); RED `6623e61` then GREEN `ca4e22b` (the 23-17 carry-over). Task 1's RED compiled and failed at runtime (plan did not refuse). Task 2's RED did not compile at its own commit (it names `RollbackKind::Cleared` and `recovery_action`, which land in `1ba9f35`), the same shape as 23-02, 23-09 and 23-11. The carry-over RED compiled and failed at runtime.

## Verification

- `cargo test -p ignition-core --test deploy_apply`: 77 passed (68 before this plan; 9 `journal_` and 2 `journal_commit_` rows new, one 23-05 test adjusted, see Deviations).
- `cargo test -p ignition-core --lib deploy::apply::journal`: 23 passed.
- `cargo test --workspace --no-fail-fast`: 2414 passed, 0 failed, 51 ignored (live and Windows-host gates); 2401 before this plan. The Docker daemon was up and the 25 `contract_rig.rs` rows ran and passed.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.

## Deviations from Plan

### Plan departures

1. **[Rule 1 - Bug exposed by the change] `a_journal_of_another_run_names_the_real_env` (23-05)** prepared the apply over a journal and expected the host to refuse at commit. Now the plan refuses first (which is this plan's truth), so the test writes its journal between `prepare` and `run`; the engine path it protects (the engine fills the env the host cannot know) is still exercised.
2. **`deploy_plan` reads the journal after the `--base` check**, not first: `bad_base_costs_no_transport_work` (22-04) pins that a bad base is refused before any transport work, and the plan says only "before reading the collections".
3. **`RollbackChecked.record` and `RollbackPrepared.record` are now `Option<RunRecord>`** (a recovery with a lost record has none); `RollbackPrepared` gained `run_id` and `interrupted`. Two lines of a 23-11 test changed to match.
4. **[Rule 2 - Missing critical functionality] The snapshot is read and hashed before a recovery restores.** The plan's D-12 truth says a missing, incomplete or hash-failing snapshot is `rollback_failed`; `host.restore` alone would have restored the files before the broken one and then failed, leaving a half restore. The pre-read costs one extra `fetch_snapshot` of a recovery (the same call the restore makes at its end).
5. **A lost record is rebuilt with all-zero `plan_sha256` and `render_sha256`** in the `rolled_back` record the recovery installs (the real values are unknown). The record is only read by `check_rollback`, which refuses a `rolled_back` record before looking at those hashes.
6. **`journal_location` was not added**, it already existed in `deploy/ledger.rs` (23-18); `journal.rs` re-exports it.
7. **The 23-17 carry-over added two engine events and an engine branch** (`commit_failed`, `commit_interrupted`) beyond the plan's two tasks; it is its own RED/GREEN pair.

No auth gates.

## Residuals worth knowing

- **Clear races a still-running remote script.** If an ssh drop leaves a remote commit script running past `ready` while a runner clears the journal (phase `ready`, tree untouched at read time), `discard_interrupted` re-checks the phase on the box and refuses once the journal is at `writing`, but the window between the host's check and its removal is not closed by anything on the runner. The scan-lock hold the dead apply took lapses within its hold; recovery right after a dropped connection should wait for the hold, which the message does not say. Worth a line in the 23-14 README rollback section.
- **A foreign edit before the first move is overwritten.** D-10 restores when the touched set differs from `pre_fingerprint`, including a difference caused by someone else's edit. That follows the decision as written; it is not asked for confirmation.
- **A `rolled_back` record with a leftover journal** (crash between the record install and the journal removal in `finalize`) is recovered by restoring again; the restore is idempotent and `rolled_back_by` becomes the recovery run.
- **Ssh hosts.** Only the local host's `read_journal`, `fingerprint`, `restore`, `discard_interrupted`, `fetch_snapshot` and `finalize` are exercised here; the `sh` and PowerShell hosts are held to the same trait by 23-06 and 23-20.
- **Stderr observer (23-13/23-14).** Three new events to render: `interrupted_run` (phase, action, torn hashes), `cleared`, `commit_interrupted` (original code).

## Tracer feedback gate and human-verify checkpoints

Auto mode is not active and `human_verify_mode` is `end-of-phase`; the orchestrator directed skipping mid-plan human-verify stops. The tracer's `<verify>` (`cargo test -p ignition-core --test deploy_apply journal_` and `--test deploy_plan`) passed before commit `baae5c3`. Skipped for the end-of-phase list: the tracer checkpoint only. A real-gateway kill-and-recover is `live_kill_mid_write_then_recover` in 23-16.

## Requirements

`requirements-completed` is empty. DAPL-09 is "An interrupted apply leaves an on-box journal, and plan, apply and rollback refuse while it is present until recovery runs". The library now does exactly that, but there is no `ign deploy apply` or `ign deploy rollback` command until 23-13 and 23-14, and the ssh hosts' own rows are 23-06 and 23-20. `REQUIREMENTS.md` already shows DAPL-09 as ticked (23-18 ticked it, as it did DAPL-08); I did not unmark it, but it becomes accurate when 23-14 lands.

## Known Stubs

None. `grep` for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and skipped tests over the changed files finds nothing.

## Threat Flags

None beyond the plan's register. T-23-49 (torn tree after a kill): `journal_kill_mid_write_then_recover`. T-23-50 (discarding a journal over a partial tree): `recovery_action` unit rows and `journal_before_move_but_changed_restores`; no discard flag exists. T-23-51 (corrupt journal treated as absent): `journal_corrupt_blocks`. T-23-52 (no record of the torn state): the `interrupted_run` event with every touched hash, asserted in `journal_kill_mid_write_then_recover`.

## Next Phase Readiness

- **23-13/23-14 (verbs):** `deploy_rollback_prepare` returns `RollbackPrepared { run_id, record: Option<_>, interrupted, prose, live_changed, .. }`; the prose already says "Recover run ..." or "Clear run ...". `DeployRollbackResult.outcome` can now be `cleared` (its JSON string), with empty `restored`, `deleted`, `snapshot` work and no scan.
- **23-15:** the events carry paths and hashes (the `interrupted_run` event lists every touched path with its live hash); the canary scan treats them like the apply's paths.
- **23-16:** `live_kill_mid_write_then_recover` can reuse `HangAfterMove` on the live rig; the recovery needs no flag.

## Self-Check: PASSED

- Modified files exist: `crates/ignition-core/src/deploy/apply/journal.rs`, `rollback.rs`, `mod.rs`, `crates/ignition-core/src/actions/deploy.rs`, `deploy_apply.rs`, `crates/ignition-core/tests/deploy_apply.rs`, `tests/common/faulty_host.rs`, this SUMMARY.
- Commits `fc9467c`, `baae5c3`, `3811f80`, `1ba9f35`, `6623e61`, `ca4e22b` exist in `git log`.
