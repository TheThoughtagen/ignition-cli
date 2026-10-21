---
phase: 23-guarded-apply-rollback
fixed_at: 2026-10-18T19:10:00Z
review_path: .planning/phases/23-guarded-apply-rollback/23-REVIEW.md
iteration: 1
findings_in_scope: 6
fixed: 6
skipped: 0
status: all_fixed
---

# Phase 23: Code Review Fix Report

**Fixed at:** 2026-10-18T19:10:00Z
**Source review:** .planning/phases/23-guarded-apply-rollback/23-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 6 (CR-01, WR-01 to WR-05)
- Fixed: 6
- Skipped: 0

Work was done sequentially on the main checkout (no worktree), one commit per finding. No new
error slug, cause or exit code was added: the four-place rule was not triggered.

**Verification (run in the main checkout, `/Users/pmannion/whiskeyhouse/ignition-cli`):**
- `cargo test --workspace --no-fail-fast`: 2487 passed, 0 failed, 62 ignored (live and Windows-host gates).
  `deploy_apply` alone: 91 passed. No known flake fired.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
- Live Windows host (the 23-16 capture host, as `svc-ignition-deploy`), after the `commit.ps1` change:
  `wflwvm01_apply_script_run` PASS (335 s), `wflwvm01_script_acl_and_reparse` PASS (128 s), and a new
  gate `wflwvm01_ps_commit_time_budget` PASS (143 s). The reports now show `deadline: false` read
  back from Windows PowerShell 5.1, and a commit over a held target with a 1 s budget reports
  `deadline: true` with no path in the reason. The scratch tree was cleaned by each test. No Keychain
  prompt occurred and no `ign` binary was reinstalled (nothing ran through the CLI).

## Fixed Issues

### CR-01: Any error after the commit returns without a restore, a recovery hint or a rollback

**Files modified:** `crates/ignition-core/src/deploy/apply/mod.rs`,
`crates/ignition-core/tests/common/faulty_deploy.rs` (new), `crates/ignition-core/tests/common/mod.rs`,
`crates/ignition-core/tests/deploy_apply.rs`
**Commit:** a35342a
**Status:** fixed: requires human verification (control flow, not syntax)
**Applied fix:** everything between the commit and the final `Ok` now has one of three exits.
- Before the verify verdict (`confirm()`): an `Err` from the lock read, the scan or the verify, or from
  appending their events, is restored through the same `roll_back` / `restore_snapshot` path as a red
  verify, with the closest option-a cause: lock read is `lock_expired`, scan is `scan_incomplete`,
  verify and its events are `verify_failed`. The step and the error's code (never its text) are kept
  as the warning `post_commit_error (<step>: <code>)`. If the restore itself cannot run (a dead
  gateway), the result is `rollback_failed` with the journal location.
- After a green verify: an `Err` from `fingerprint` or `finalize`, or from the `verify_done` event, is
  the interrupted run: `deploy_journal_present` with `ign deploy rollback --run <id> --env <real env>`,
  the files left as written, nothing restored on a guess, and a `commit_interrupted` event. This
  matches the existing A9 recovery (a kill past a green verify recovers by rollback). A `finalize`
  whose answer was lost but whose journal is gone (the host installs the record before it removes the
  journal) is `applied` with the warning `finalize_reported_error (<code>)`. A failed `finalized`
  event is only the warning `events_log_unwritable`.
- An `Err` from the `committed` event goes through the existing `commit_failed` journal check.
- No new slug, cause or exit code. `deploy_rolled_back` stays in `data` only.
- Tests (new fault wrapper `FaultyDeploy`, armed from a `FaultyHost` hook after the commit):
  `post_commit_lock_check_error_restores_as_lock_expired`,
  `post_commit_scan_error_restores_as_scan_incomplete`,
  `post_commit_verify_error_restores_as_verify_failed`,
  `post_commit_error_with_the_gateway_gone_is_rollback_failed_with_the_journal`,
  `green_verify_then_fingerprint_error_is_the_interrupted_run`,
  `green_verify_then_finalize_error_is_the_interrupted_run` (both then recover through `rollback --run`),
  `green_verify_then_a_finalize_that_ran_but_reported_an_error_is_applied`. All seven failed before
  the fix.
- Not covered by a test: an event-log append failure (`steps.emit` Err). The log is a plain file append
  and has no injection point; the paths are by inspection.

### WR-05: `commit_failed` treats an unreadable journal as "nothing was written"

**Files modified:** `crates/ignition-core/src/deploy/apply/mod.rs`,
`crates/ignition-core/tests/deploy_apply.rs`
**Commit:** 6ba4618
**Status:** fixed: requires human verification
**Applied fix:** `commit_failed` now distinguishes the box states (own journal, another run's journal,
no journal, unreadable). When the journal cannot be read or parsed, the original error keeps its slug
and exit code (a `transport_failed` stays exit 4) and gains
`; this run's journal may be on the host: recover it with: ign deploy rollback --run <id> --env <env>`.
Any other error variant becomes `deploy_journal_present` for this run. A `commit_interrupted` event is
emitted with the code `<code>; journal state unknown`. Test:
`journal_commit_error_with_an_unreadable_box_names_the_recovery` (failed before the fix). A readable box
with no journal still returns the original error.

### WR-03: The rollback `hold_budget` preflight counts the ledger

**Files modified:** `crates/ignition-core/src/deploy/apply/rollback.rs`,
`crates/ignition-core/src/deploy/apply/preflight.rs`, `README.md`
**Commit:** ed77b23
**Applied fix:** the rollback preflight now counts `restore_set` (paths put back plus paths removed, the
ledger excluded), the quantity the apply is judged by. Boundary test
`rollback_budget_counts_what_the_apply_counted`: a record of 1527 files plus the ledger passes
`hold_budget`, 1528 does not. `hold_budget_finding` became `pub(super)` for the test. The README
rollback step 5 says how the check is sized.

### WR-01: The rollback live-change guard is checked before the preflights and the lock

**Files modified:** `crates/ignition-core/src/deploy/apply/rollback.rs`,
`crates/ignition-core/tests/deploy_apply.rs`, `README.md`
**Commit:** 9707225
**Status:** fixed: requires human verification
**Applied fix:** after `HeldLock::acquire` and before `restore_snapshot`, the live hashes are read again
(`live_changes`, the same function `check_rollback` uses). A new difference without
`--discard-live-changes` is `deploy_live_changed`, after one no-op scan releases the lock (D12); with the
flag it is listed (event `live_changes_discarded`, and in `discarded_live_changes`). A recovery of an
interrupted run is unchanged (D-11, no post-run state). Tests:
`rollback_live_edit_after_the_check_is_refused_under_the_lock` and
`rollback_live_edit_after_the_check_is_named_when_discarding` (a `FaultyHost` hook edits `DB_CONFIG`
after the run's own check). Both failed before the fix.
**Residual:** the window between this re-check and the host's `restore` is narrow but not closed: the
scripts still do not compare the live hash with the post-run hash atomically with the move (the
reviewer's "better" option, carrying the expected hash in each `P` record). Not done: it changes the
work-list format for restore on both scripts.

### WR-02: `rollback_failed` guidance names a snapshot copy that does not exist and omits the retry

**Files modified:** `crates/ignition-core/src/error.rs`, `crates/ignition-core/src/deploy/apply/mod.rs`,
`crates/ignition-core/src/deploy/apply/rollback.rs`, `crates/ignition-core/tests/deploy_apply.rs`,
`README.md`
**Commit:** 55e1b26
**Applied fix:** `RollbackFailure` gained `env` (carried by a new `RestoreContext.env`) and
`snapshot_dir()`. The message now reads: retry with `ign deploy rollback --run <id> --env <env>` (the
journal and snapshot are still on the host); if the retry reports a lost or damaged snapshot, restore
from the `.gwbk` (when this run took one) or from the on-box `snapshots/<run>/` directory (printed,
next to the journal), then delete the journal by hand. The hint says to run the retry first. The
run-directory `snapshot/` is no longer offered. README (rollback recovery bullet) matches; the
`deploy_family_coverage` agreement test and the exit-table tests pass. Along the way a host's journal
refusal inside the restore now names the real env (the `<env>` placeholder, IN-01, for that path only).
Tests: `rollback_failed_message_names_run_dir_journal_and_the_guidance` (retry first, real env, on-box
snapshot dir, no "run-directory snapshot") and `journal_snapshot_lost_is_rollback_failed`.

### WR-04: The commit call has no deadline tied to the lock hold

**Files modified:** `crates/ignition-core/src/deploy/transport/batch.rs`,
`crates/ignition-core/src/deploy/apply/host.rs`, `crates/ignition-core/src/deploy/apply/mod.rs`,
`crates/ignition-core/src/deploy/transport/scripts/commit.sh`,
`crates/ignition-core/src/deploy/transport/scripts/commit.ps1`,
`crates/ignition-core/src/deploy/transport/local.rs`,
`crates/ignition-core/src/deploy/transport/local/apply_ops.rs`,
`crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs`,
`crates/ignition-core/src/deploy/transport/remote.rs`, `crates/ignition-core/tests/deploy_apply.rs`,
`crates/ignition-core/tests/deploy_host_conformance.rs`,
`crates/ignition-core/tests/live_transport_wflwvm01.rs`, `README.md`
**Commit:** 3cec932
**Status:** fixed: requires human verification
**Applied fix:**
- Work list: a new commit-only `B<TAB><seconds>` record (1 to 7200), parsed and validated in Rust and in
  both scripts; `BatchSpec.commit_budget_s`; `BatchReport.deadline`.
- Engine: the budget is `hold_s - LOCK_MARGIN_S` (174 s of the shipped 176 s), at least 1. A report with
  `failure` and `deadline` ends as a restore with the cause `lock_expired` (not `write_failed`), under
  the lock if it is still held.
- `commit.sh` and `commit.ps1`: the clock starts with the script; no move, delete or retry sleep starts
  after the budget (the retry sleeps are therefore bounded by it, in the stale-check `LiveRetry` too);
  the script stops and the report carries `deadline` and a value-free reason.
- Local host: the same check with an `Instant`; test fault `ApplyFault::SlowMoves`.
- Commit call: killed at budget + 60 s (`COMMIT_EXEC_SLACK_S`) instead of `ssh.op_timeout_s` (1050 s by
  default), through new `exec_posix_within` / `exec_powershell_within`. If that backstop fires the run
  is the interrupted run (`deploy_journal_present`), as for a dropped connection.
- Tests: `a_commit_that_outruns_the_hold_stops_and_restores_as_lock_expired` (engine, local host, hold
  3 s), `sh_commit_stops_when_its_time_budget_runs_out` (real `commit.sh` under local `sh`),
  `the_time_budget_round_trips_and_is_for_a_commit_in_range`,
  `the_commit_budget_is_the_hold_less_its_margin_and_never_zero`,
  `the_within_forms_kill_at_their_own_limit_not_the_op_timeout`, and the live
  `wflwvm01_ps_commit_time_budget` (see Verification).
**Residual:** the budget counts from the script's start while the hold counts from the grant, so the ssh
connect and start-up are absorbed by the 2 s margin and the post-commit lock check (a late finish still
ends as `lock_expired`). Linux ssh remains unproven against a live host (FA-23-05); `commit.sh` was
exercised under local `sh` only.

## Skipped Issues

None. Info findings IN-01 to IN-05 were out of scope (`fix_scope: critical_warning`); IN-01 was touched
only for the restore path of WR-02, as noted.

---

_Fixed: 2026-10-18T19:10:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
