---
phase: 23-guarded-apply-rollback
plan: 19
subsystem: deploy
tags: [apply-host, conformance, fault-injection, faulty-host, restore, journal, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-04 ApplyHost, the local host and the seven method rows; 23-02 checkpoint outcome (journal phases, on-box layout)
provides:
  - ApplyFault (FailMoveAt, FailRestoreAt, HangAfterMove) and LocalFsTransport::with_apply_fault, a doc-hidden seam no env var, flag or CLI path reaches
  - ten commit and restore rule rows in the shared scenarios() table, each pinning one guard of the local host
  - FaultyHost, a call-granularity fail, hang and after-hook wrapper for any ApplyHost
affects: [23-05, 23-06, 23-12, 23-16, 23-17, 23-20]

actuals:
  tokens: 11600
  tasks: 3
  commits: 6

tech-stack:
  added: []
  patterns:
    - "a fault is injected through the real failure path: FailMoveAt refuses every attempt of the move so it takes the retry loop and ends in the real report; HangAfterMove stops the blocking commit after the k-th move and the async caller then waits at an await point"
    - "FaultyHost keeps its rules, counters and call log behind one Arc, so a clone boxed into the engine and a clone kept by the test see the same state"

key-files:
  created:
    - crates/ignition-core/tests/common/faulty_host.rs
  modified:
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/tests/common/mod.rs

key-decisions:
  - "the hang is raised by the async `commit` in local.rs, not inside the blocking thread: the blocking commit returns early after the k-th move with the journal at writing, and the async wrapper notifies (notify_one, so a permit is stored) and awaits pending(). An abort then leaves exactly what a kill -9 at that point leaves, and no blocking thread is left running"
  - "FaultyHost counts invocations from 1 per call name; a failed or hung call never reaches the inner host; hooks run after the inner call returns, also when it returned an error"
  - "FailRestoreAt(k) indexes the record's touched entries (the same index restore reports as `restore k`), not the order restore walks them (ledger last)"

patterns-established:
  - "a rule row names a guard, builds its state from the Ctx helpers, and asserts on the report, the journal and the files; the row never reads host internals, so a script host reruns it unchanged"

requirements-completed: []
requirements-advanced: [DAPL-06, DAPL-07, DAPL-09]

duration: ~1h45m (most of it cargo builds)
completed: 2026-10-16
---

# Phase 23 Plan 19: Commit and restore rule rows, the local fault seam and FaultyHost Summary

**Every commit and restore guard of the local host is now pinned by a row in the shared conformance table, and the engine can be driven into a mid-write failure, a mid-write kill and a failing restore or finalize without a test backdoor in the binary.**

## Conformance rows

`scenarios()` now holds 17 shared rows, each run against the local host as `conformance_local_<row>`.

From 23-04 (7): `add_one_file`, `fingerprint_is_read_only`, `finalize_removes_journal_and_staging_keeps_snapshot`, `discard_interrupted_touches_no_collection_file`, `fetch_snapshot_copies_with_hashes`, `read_record_absent_is_none`, `probe_reports_writable_without_touching_the_collection`.

Added here (10), with the guard each one pins:

| Row | Guard |
|---|---|
| `replace_file` | the staged copy is moved over the target; no temp, backup or dot-named entry in the collection; pre-image in the snapshot |
| `delete_file_and_emptied_dirs` | deletes remove each emptied directory deepest first and never `<module>/<type>`; restore recreates them |
| `ledger_swap_and_restore` | a prior ledger is snapshotted, swapped and put back |
| `pre_image_mismatch_aborts_before_any_move` | changed file or changed ledger: `stale`, nothing moved, journal and partial snapshot removed, collection and ledger untouched |
| `journal_present_refuses` | a second run id finds `journal.json` and fails `deploy_journal_present` before any write (edge DAPL-09 concurrency) |
| `self_move_refused` | a staged copy that is a hard link to its target is never renamed onto itself; the file survives |
| `gateway_owned_file_refused` | `config-mode.json`, a dot-named component and a two-component path are refused before any I/O by `prepare` and by `restore` (which wrote not even its journal) |
| `directory_where_file_goes_is_stale` | a directory where a file must go is a pre-image mismatch |
| `symlinked_target_dir_is_refused` | a directory swapped for a symlink after the plan is not followed; nothing is written through it |
| `restore_twice_is_idempotent` | two restores leave byte-identical collection, tree and ledger (edge DAPL-07 idempotency) |

`symlinked_target_dir_is_refused` is one row over the plan's nine (see Deviations).

Fault rows (local host only, beside the table): `fail_move_at_leaves_a_recoverable_partial_tree`, `fail_restore_at_reports_the_index`, `hang_after_move_can_be_aborted`.

FaultyHost rows: `faulty_host_fails_the_nth_call`, `faulty_host_hangs_the_nth_call`, `faulty_host_runs_a_hook_after_the_nth_call`.

## The fault seam

`ApplyFault` in `crates/ignition-core/src/deploy/transport/local.rs`, set by `with_apply_fault`; both `#[doc(hidden)]`.

- `FailMoveAt(k)`: write k (index into the byte-sorted writes) refuses every attempt, retried `RENAME_RETRIES` times, then ends in the real failure: the report names `write k`, phase `writing`, the journal stays at `writing`, the snapshot is complete.
- `FailRestoreAt(k)`: restore entry k fails; the report names `restore k` and the journal stays at `restoring`.
- `HangAfterMove(k, Arc<Notify>)`: after k writes have moved, `commit` signals and waits forever at an await point.

`grep -rn with_apply_fault crates/ignition-cli/src` prints nothing; no env var, flag or CLI path sets a fault.

## FaultyHost API

`crates/ignition-core/tests/common/faulty_host.rs`, declared in `common/mod.rs`. `FaultyHost<H: ApplyHost>` implements `ApplyHost` by delegating all nine methods; it is `Clone` and clones share the wrapped host, the rules and the call log. Invocations are counted from 1 per call name (`"probe"`, `"prepare"`, `"commit"`, `"restore"`, `"fingerprint"`, `"finalize"`, `"read_record"`, `"fetch_snapshot"`, `"discard_interrupted"`).

```rust
FaultyHost::new(inner)
    .fail(call: &'static str, nth: usize)                         // transport_failed, inner not called
    .hang(call, nth, notify: Arc<Notify>)                          // notify_one, then wait forever, inner not called
    .after(call, nth, || async { ... })                            // runs after the inner call returned, before the caller sees the result
host.calls() -> Vec<&'static str>                                  // the call sequence so far
```

For the engine plans: 23-17 uses `after("commit", 1, ..)` to tick the FakeGateway past the hold, `after("prepare", 1, ..)` to rewrite a mode file, and an `after` on `restore` to alter a restored file; 23-16 uses `after("commit", 1, ..)` to sleep past the hold; `fail("commit"|"restore"|"finalize", n)` drives the write-side failures.

## Task Commits

1. **Task 1 (tracer), RED:** `c22b680` (fault rows fail; `ApplyFault` and the builder exist but inject nothing)
2. **Task 1 (tracer), GREEN:** `88d2410` (`FailMoveAt` and `FailRestoreAt` through the real failure path)
3. **Task 2:** `558728f` (ten rule rows; every row passed against the 23-04 host)
4. **Task 3, RED:** `fb9de40` (`hang_after_move_can_be_aborted` fails; the variant exists but does not hang)
5. **Task 3, GREEN:** `11486a1` (`HangAfterMove`)
6. **Task 3, harness:** `bc45196` (`FaultyHost` and its three rows)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active (`workflow._auto_chain_active` and `workflow.auto_advance` are false), so the interactive rule asks for a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase`, the orchestrator directed a sequential run, and the slice has no human-verifiable surface (a test seam and conformance rows), so, as in 23-01, 23-02, 23-04 and 23-18, execution continued. The tracer's `<verify>` (`cargo test -p ignition-core --test deploy_host_conformance -- fail_move_at fail_restore_at`) passed before the tracer GREEN commit `88d2410` and again with the full file after Task 3.

## Host fixes forced by a row

None. All ten rule rows passed against the 23-04 host on first run. Because a row that passes at once proves nothing by itself, each guard was checked by mutation (a temporary edit to `apply_ops.rs`, reverted before commit):

| Mutation | Rows that failed |
|---|---|
| self-move guard replaced by `if false` | `self_move_refused` |
| `FsError::Changed` read as `Live::Absent` | `directory_where_file_goes_is_stale`, `symlinked_target_dir_is_refused`, `fingerprint_is_read_only` |
| `abort_before_move` made a no-op | `pre_image_mismatch_aborts_before_any_move`, `directory_where_file_goes_is_stale`, `symlinked_target_dir_is_refused` |
| emptied-directory removal allowed down to depth 1 | none at first; `delete_file_and_emptied_dirs` was then changed so `<module>/<type>` becomes empty (the untouched sibling moved to another module), and it failed |

## Test results

- `cargo test -p ignition-core --test deploy_host_conformance`: 24 passed (17 shared rows run locally, the wiring guard, 3 fault rows, 3 FaultyHost rows)
- `cargo test --workspace`: all green, no failures
- `cargo clippy --workspace --all-targets -- -D warnings`: clean

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] A symlinked target directory had no row**
- **Found during:** Task 2 (the orchestrator's note that 23-04 implemented the guard without a row, and the project's "refuse symlinked components" discipline)
- **Issue:** the plan names nine rule rows; the symlink guard (T-23-75, a guard lost without a failing test) was not among them.
- **Fix:** added `symlinked_target_dir_is_refused` as a tenth rule row. The acceptance count of 16 shared rows therefore reads 17; the 16 named in the plan are all present and passing.
- **Files modified:** `crates/ignition-core/tests/deploy_host_conformance.rs`
- **Commit:** `558728f`

**2. [Rule 3 - Blocking] `HangAfterMove` cannot hang inside the blocking thread**
- **Found during:** Task 3
- **Issue:** the plan says the filesystem calls stay inline in the async fn so the hang is an await point. 23-04 runs every host operation in `spawn_blocking`, and aborting a task does not stop a blocking thread, so a hang inside it would leave a running thread and a test that cannot observe a settled state.
- **Fix:** the blocking commit stops after the k-th move (journal at `writing`) and reports it through a flag; the async `commit` then notifies and awaits `pending()`. The observable contract is the one the plan states: a hang is an await point a test can abort, the disk state is what a kill -9 there leaves.
- **Files modified:** `local.rs`, `local/apply_ops.rs`
- **Commit:** `11486a1`

## Known Stubs

None.

## Residuals recorded as unrun

- `self_move_refused` and `symlinked_target_dir_is_refused` are `#[cfg(unix)]` bodies (no-ops elsewhere). The non-unix `Dir::ident` returns `None`, so the inode self-move guard does not exist on the path-based Windows local fallback; the Windows ssh host is 23-08 and 23-20 and is `pending` (23-01). Not run: any Windows build of these rows.

## Threat Flags

None. `ApplyFault` lives in the library the binary links (T-23-18); it is `#[doc(hidden)]`, has one setter, and `grep -rn with_apply_fault crates/ignition-cli/src` prints nothing.

## Self-Check: PASSED

- Created: `crates/ignition-core/tests/common/faulty_host.rs` exists.
- Commits `c22b680`, `88d2410`, `558728f`, `fb9de40`, `11486a1`, `bc45196` exist in `git log`.
- `grep -n 'pub enum ApplyFault' crates/ignition-core/src/deploy/transport/local.rs` prints one line.
