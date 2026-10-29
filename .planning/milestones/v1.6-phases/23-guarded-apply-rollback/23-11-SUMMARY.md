---
phase: 23-guarded-apply-rollback
plan: 11
subsystem: deploy
tags: [rollback, restore-snapshot, on-box-record, live-changed, discard-live-changes, run-directory, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-17 restore_snapshot, HeldLock identity; 23-03 RunId, RunDir, EventLog, RunRecord, fingerprint; 23-04 ApplyHost (read_record, fingerprint, restore, finalize, fetch_snapshot); 23-07 run_preflights; 23-09 restart-pending read; 23-02 vocabulary (option-a) and the typed errors
provides:
  - run_rollback, check_rollback, RollbackContext, RollbackOptions, RollbackOutcome, RollbackKind, RollbackChecked, restore_set (deploy/apply/rollback.rs, re-exported from deploy::apply)
  - deploy_rollback_prepare, deploy_rollback_execute, RollbackPrepared, RollbackPrepareOptions, DeployRollbackResult (actions/deploy_apply.rs)
  - RestoreContext: restore_snapshot no longer needs a declared repo
  - ApplyEvent::LiveChangesDiscarded; ApplyEvent::RestoreStarted.cause is now optional
affects: [23-12, 23-13, 23-14, 23-15, 23-16, 23-21]

actuals:
  tokens: 15300
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "one check function (check_rollback) answers the early prepare and runs again inside run_rollback, so a journal, a live change or a second rollback that appears between the two is still refused"
    - "a Fresh test harness builds host, reader, fake gateway and REST server from the target file alone, so every rollback test is a fresh process"

key-files:
  created:
    - crates/ignition-core/src/deploy/apply/rollback.rs
  modified:
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-core/tests/deploy_apply.rs

key-decisions:
  - "RestoreInput.cause and ApplyEvent::RestoreStarted.cause became Option<RollbackCause>; None is a manual rollback. No new RollbackCause variant, so the closed data.cause vocabulary (lock_expired, write_failed, scan_incomplete, verify_failed) is unchanged and an apply can never report a manual cause. The event line reads {\"cause\": null}; the rollback run's run.json says kind: rollback."
  - "Manual rollback carries restart_pending and restart_pending_new. The list is read before the lock and again after the restore (restart_report, now taking the DeployApi), read-only, same warning restart_pending_unavailable. restore_snapshot itself still does not read it; run_rollback does, as run_apply does."
  - "restore_snapshot takes a RestoreContext {master node, host, deploy, clock, hold, poll} instead of an ApplyContext and ApplyOptions: a rollback has no Declared. ApplyContext::restore_context builds it for the apply paths. grep -c 'fn restore_snapshot' still prints 1."
  - "run_rollback(ctx, opts) re-runs check_rollback itself instead of taking the prepared value (the plan's run_rollback(ctx, prepared, opts)). RollbackOptions.run_id is the run to roll back; the rollback's own id is generated. deploy_rollback_execute only checks that the prepared record is for that run."
  - "The scan-lock is taken in run_rollback before restore_snapshot and passed as held, so another actor's hold is the typed scan_lock_held before any write; inside restore_snapshot a refused acquire would have been wrapped as rollback_failed."
  - "A damaged record that cannot name its own run is reported under the requested run id (the record parser says unknown)."
  - "No .gwbk is taken for a rollback (D-13 is an apply step; the plan does not ask for one)."

patterns-established:
  - "a record is trusted only after its fingerprints agree with its touched set (pre always, post for an applied record), not just after it parses"

requirements-completed: []
requirements-advanced: [DAPL-08, DAPL-10]

duration: ~40min (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 11: Manual rollback from the on-box record Summary

**`run_rollback` undoes a past run from a fresh process with only the target file and the run id: it validates the id before any host contact, reads and cross-checks the on-box record, refuses when live state has moved on unless told to discard it, and restores through the same `restore_snapshot` the apply engine uses.**

## What was built

- `deploy/apply/rollback.rs` (new, re-exported from `deploy::apply`): `check_rollback` is the one gate function. Order: journal on the box (any journal is `deploy_journal_present` naming its run), `ApplyHost::read_record` (no record, or a damaged one, is `deploy_run_invalid`), `validate_record`, the local run-directory cross-check, then the live hashes of the touched set against the record's `post_map`.
- `validate_record`: run id, gateway and mode must match the target; state `applying` (no journal) and state `rolled_back` ("already rolled back by <id>") are `deploy_run_invalid`; `pre_fingerprint` must equal the fingerprint of the touched set's pre hashes, and an applied record's `post_fingerprint` must equal that of its post hashes (a forged or edited record is `deploy_run_invalid`, not a silent preference).
- `cross_check_local`: when `.ign/deploy/<gateway>/<run id>/` exists, `run.json` (run_id, gateway, mode, plan_sha256, render_sha256) and `plan.json` (the last four) must agree with the record; the message names the file and the field. Links are not followed (`symlink_metadata`), files are capped at 8 MiB, and a `run.json` with `kind: rollback` is refused (it is a rollback's directory, not a deployment's).
- Staleness: touched paths whose live hash is not the record's post hash. Without `--discard-live-changes` that is `deploy_live_changed` (first ten paths, the count). With it, the paths go into the rollback's `run.json` (`discarded_live_changes`), a `live_changes_discarded` event (first ten, count) and `result.json`. Files outside the touched set never count.
- `run_rollback`: re-checks, creates the rollback's own run directory (git-ignore guard first), `run.json` (`kind: rollback`, `target_run_id`, hold, designers, preflight verdicts), `plan.json` (the restore and delete sets and what happens to the ledger), the seven preflights (`mode_master`, `mode_backup`, `wrong_target`, `write_rest`, `write_host`, `designers`, `hold_budget`, sized by the record's touched set), the pending-restart read, the scan-lock, `restore_snapshot`, the restart read again, `result.json` once through one exit point. Outcome `restored`; the on-box record ends `rolled_back` with `rolled_back_by` this run's id.
- `actions/deploy_apply.rs`: `deploy_rollback_prepare(repo, env, run_id_text, reader, host, &RollbackPrepareOptions)` (id first, then `load_target`, then `check_rollback`, then byte-stable prose: run, gateway, mode, counts, `plan_sha256`, the changed paths), `deploy_rollback_execute(prepared, ctx, opts)`, `DeployRollbackResult`.

## Carry-overs resolved

| Carry-over | Decision |
|---|---|
| `RollbackCause` has no manual value | Made the field `Option<RollbackCause>` (RestoreInput and the `RestoreStarted` event); `None` is manual. The closed apply vocabulary is not widened. One existing test line (`fault_write_failed_mid_commit_restores`) changed to `Some(RollbackCause::WriteFailed)`. |
| `restore_snapshot` does not read the restart-pending list | The manual rollback result carries `restart_pending` and `restart_pending_new`, read by `run_rollback` around the restore (read-only, SF-3, unreadable is the warning `restart_pending_unavailable`). `restore_snapshot` is unchanged in this respect. |
| Slugs | None added. `deploy_run_invalid`, `deploy_live_changed`, `deploy_journal_present`, `rollback_failed` and the existing `scan_lock_held` are used as the option-a vocabulary has them; the four-place rule was not triggered. |
| Validate the run id before any host contact; stay inside `ign-deploy/<g>/<m>/` | `RunId::parse` runs first in `deploy_rollback_prepare` (`rollback_bad_run_id_is_invalid_input` asserts zero host calls, zero gateway requests, no `.ign`); record paths are checked by `RunRecord::parse` (`check_touched`) and the hosts' own `mirror_path_problem`. The local run-directory read does not follow links. |

## Tests

| Behaviour | Test |
|---|---|
| apply, delete `.ign`, roll back from a new host, reader and fake gateway; collection, ledger, scan, record, run directory | `rollback_fresh_process_from_record_only` |
| `../x`, empty, 129 characters: `invalid_input`, zero contact | `rollback_bad_run_id_is_invalid_input` |
| edited touched file refuses `deploy_live_changed`, host unchanged | `rollback_live_changed_refuses` |
| same with `discard_live_changes`: restores, `run.json` and events list the path | `rollback_discard_live_changes_proceeds` |
| file outside the touched set does not block and is left alone | `rollback_unrelated_edit_does_not_block` |
| apply A then B on the same file: rollback of A refuses; B still rolls back | `rollback_older_run_behind_newer_refuses` |
| no record, corrupt, other mode, other gateway, `applying` with no journal, self-inconsistent | `rollback_record_problems` |
| local `run.json` plan_sha256, `plan.json` render_sha256, `kind: rollback` | `rollback_run_dir_mismatch` |
| second rollback: "already rolled back by <id>", zero writes (edge DAPL-08 idempotency) | `rollback_twice_refuses` |
| another run's journal, before and between prepare and run; token B holds the lock: `scan_lock_held` before any write (edge DAPL-08 concurrency) | `rollback_other_journal_refuses`, `rollback_lock_held_refuses` |

TDD sequence: RED `f4074b8`, GREEN `d8b77ae` (the tracer's code path: journal check, record read, restore, run directory), RED `63928c6` (six of the nine Task 2 tests failed against it, as they should), GREEN `ab8a3fa`. Three Task 2 tests (`rollback_unrelated_edit_does_not_block`, `rollback_other_journal_refuses`, `rollback_lock_held_refuses`) already passed at the Task 2 RED because the tracer's code covers them; they are real tests of those behaviours, just not RED. Both RED commits were written before the code they test; the tracer RED does not compile at its own commit (the types it names land in `d8b77ae`), the same shape as 23-02 and 23-09.

## Verification

- `cargo test -p ignition-core --test deploy_apply`: 66 passed (55 before this plan, 11 new `rollback_` rows).
- `cargo test --workspace --no-fail-fast`: 2401 passed, 0 failed, 51 ignored (live and Windows-host gates). The Docker daemon was up and the 25 `contract_rig.rs` rows ran and passed.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.

## Deviations from Plan

### Plan departures

1. **`run_rollback(ctx, opts)`, not `run_rollback(ctx, prepared, opts)`.** `RollbackPrepared` lives in the action layer, which depends on the engine. The engine takes the run id in `RollbackOptions.run_id` (as the plan's own options struct has it) and re-runs `check_rollback`, so the early refusal and the run cannot disagree. `deploy_rollback_execute` still takes the prepared value and checks it is for that run.
2. **A new file, `deploy/apply/rollback.rs`**, rather than growing `mod.rs` (1500 lines). The names are re-exported from `deploy::apply`, so they sit where the artifact table says.
3. **`RollbackContext` has an `env` field** (the journal refusal and `DeployRollbackResult.env` need it) beyond the plan's list.
4. **`RollbackPrepareOptions { discard_live_changes }`** is the `opts` of `deploy_rollback_prepare`; the plan did not name its type.
5. **[Rule 3 - Blocking] `restore_snapshot` signature.** It needed an `ApplyContext` and so a `Declared`, which a rollback does not have. It now takes `RestoreContext` and no `ApplyOptions`; `restore_context` builds one from an `ApplyContext`. The 23-17 behaviour and tests are unchanged (all 23-17 `fault_`/`auto_rollback_` rows pass).
6. **[Rule 2 - Missing critical functionality] The record's fingerprints are checked** and the local run-directory gateway name is checked as a plain component before it becomes a path; neither is in the plan's truths, both follow from T-23-47 (a forged or edited record steering the restore).
7. **[Rule 1 - Bug, found by the test] A damaged record was reported under run id `unknown`**, which breaks the truth that each problem names the run id. `check_rollback` now reports it under the requested id.

No auth gates.

## Residuals worth knowing

- **Interim loop with a journal of the same run.** Any journal refuses with `deploy_journal_present`, including the target run's own, and that error's message tells the user to run `ign deploy rollback --run <id>`, which gives the same refusal. 23-12 adds the same-run recovery path; until it lands a user with their own run's journal is bounced between the two.
- **A window between the check and the lock.** `run_rollback` re-fingerprints just before the preflights, not under the lock; `restore_snapshot` does not compare live hashes. A foreign edit in the few hundred milliseconds between that read and the restore would be overwritten without being named. The apply path closes the same gap with the under-lock pre-image check (`plan_stale`); a restore has no equivalent host op. The scan-lock does not stop file writers, only scans (C2 in `23-LIVE-CAPTURES.md` is the open question for Designer saves).
- **No `.gwbk` for a rollback.** The apply's `.gwbk` is the restore point for the gateway; a rollback restores from the on-box snapshot and writes the snapshot copy into its run directory only.
- **Ssh hosts.** Only the local host's `read_record`/`restore`/`fingerprint`/`finalize` are exercised here; the ssh hosts are held to the same `ApplyHost` conformance table by 23-06 and 23-20.

## Tracer feedback gate and human-verify checkpoints

Auto mode is not active and `workflow.human_verify_mode` is `end-of-phase`; the orchestrator directed skipping mid-plan human-verify stops. The tracer's `<verify>` (`cargo test -p ignition-core --test deploy_apply -- rollback_fresh_process_from_record_only rollback_bad_run_id_is_invalid_input`) passed before commit `d8b77ae` and again in the full run after Task 2. Skipped for the end-of-phase list: the tracer checkpoint only. Nothing human-verifiable exists yet at this level (no verb until 23-14); a real-gateway rollback is 23-16.

## Requirements

`requirements-completed` is empty. DAPL-08 is "User can run `ign deploy rollback --run <id>` from a fresh process" and this plan delivers the library engine only; the verb is 23-14, the Windows and Linux ssh hosts' own rows are 23-06 and 23-20, the live gate is 23-16. 23-18 already ticked DAPL-08 in `REQUIREMENTS.md`; I did not unmark it, but it is not yet accurate in the sense of the requirement's own wording: there is no `ign deploy rollback` command until 23-14. It becomes true when 23-14 lands.

## Known Stubs

None. `grep` for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and skipped tests over `rollback.rs` and `actions/deploy_apply.rs` finds nothing.

## Threat Flags

None beyond the plan's register. T-23-45 (`--run` traversal): `RunId::parse` before any path is built, `rollback_bad_run_id_is_invalid_input`. T-23-46 (overwriting newer live work): `rollback_live_changed_refuses`, `rollback_older_run_behind_newer_refuses`, and `--discard-live-changes` recorded in `run.json`, events and `result.json`. T-23-47 (forged or foreign record): `RunRecord::parse` with gateway and mode, the state and fingerprint checks, the local cross-check; `rollback_record_problems`, `rollback_run_dir_mismatch`. T-23-48 (who rolled back and what was discarded): the rollback run directory, `rolled_back_by` in the on-box record, discarded paths in `run.json` and events.

## Next Phase Readiness

- **23-12:** `check_rollback` refuses any journal; the same-run recovery path (D-09..D-12) replaces that refusal for the run's own journal. `RollbackKind` gains `Cleared`.
- **23-13/23-14 (verbs):** `ApplyEvent` has two changes to render in the stderr observer: `live_changes_discarded` and a `restore_started` whose `cause` can be null. `DeployRollbackResult` carries `discarded_live_changes`, `restart_pending` and `restart_pending_new` for the human output. The verb builds `RollbackContext` from `load_target`, `for_node` and `apply_host_for_node`, and `RollbackPrepared.target` gives it the target the file named.
- **23-15:** the rollback `run.json` and events carry paths of changed files (names, never content); the canary scan should treat them as the apply's paths are treated.

## Self-Check: PASSED

- Created and modified files exist: `crates/ignition-core/src/deploy/apply/rollback.rs`, `crates/ignition-core/src/deploy/apply/mod.rs`, `crates/ignition-core/src/actions/deploy_apply.rs`, `crates/ignition-core/tests/deploy_apply.rs`, this SUMMARY.
- Commits `f4074b8`, `d8b77ae`, `63928c6`, `ab8a3fa`, `2e8ebb2` exist in `git log`.
- `grep -c 'fn restore_snapshot' crates/ignition-core/src/deploy/apply/mod.rs` prints 1.
