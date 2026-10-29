---
phase: 23-guarded-apply-rollback
reviewed: 2026-10-18T14:20:21Z
depth: standard
files_reviewed: 57
files_reviewed_list:
  - README.md
  - crates/ignition-cli/src/cli.rs
  - crates/ignition-cli/src/main.rs
  - crates/ignition-cli/src/mcp.rs
  - crates/ignition-cli/src/render.rs
  - crates/ignition-cli/tests/common/mod.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - crates/ignition-cli/tests/contract_mcp.rs
  - crates/ignition-cli/tests/contract_stdout_purity.rs
  - crates/ignition-cli/tests/deploy_family_coverage.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/actions/deploy_apply.rs
  - crates/ignition-core/src/actions/mod.rs
  - crates/ignition-core/src/actions/workspace.rs
  - crates/ignition-core/src/client/deploy_api.rs
  - crates/ignition-core/src/client/restart.rs
  - crates/ignition-core/src/deploy/apply/fingerprint.rs
  - crates/ignition-core/src/deploy/apply/gates.rs
  - crates/ignition-core/src/deploy/apply/hold.rs
  - crates/ignition-core/src/deploy/apply/host.rs
  - crates/ignition-core/src/deploy/apply/journal.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/apply/preflight.rs
  - crates/ignition-core/src/deploy/apply/record.rs
  - crates/ignition-core/src/deploy/apply/rollback.rs
  - crates/ignition-core/src/deploy/apply/run_store.rs
  - crates/ignition-core/src/deploy/ledger.rs
  - crates/ignition-core/src/deploy/mod.rs
  - crates/ignition-core/src/deploy/target.rs
  - crates/ignition-core/src/deploy/transport/batch.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/local_fs.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/remote.rs
  - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
  - crates/ignition-core/src/deploy/transport/scripts/commit.sh
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/tests/common/fake_deploy.rs
  - crates/ignition-core/tests/common/faulty_host.rs
  - crates/ignition-core/tests/common/forbid_writes.rs
  - crates/ignition-core/tests/common/mod.rs
  - crates/ignition-core/tests/common/sftp_server.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/deploy_client_contract.rs
  - crates/ignition-core/tests/deploy_host_conformance.rs
  - crates/ignition-core/tests/deploy_pull.rs
  - crates/ignition-core/tests/deploy_transport.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-core/tests/fixtures/live-rig-deploy/capture23.sh
  - crates/ignition-core/tests/live_deploy_apply.rs
  - crates/ignition-core/tests/live_transport_wflwvm01.rs
  - crates/ignition-core/tests/restart_wait_contract.rs
  - crates/ignition-tui/src/routes.rs
findings:
  critical: 1
  warning: 5
  info: 5
  total: 11
status: issues_found
---

# Phase 23: Code Review Report

**Reviewed:** 2026-10-18T14:20:21Z
**Depth:** standard
**Files Reviewed:** 57
**Status:** issues_found

## Summary

The write path was read in full: `deploy/apply/*`, `actions/deploy_apply.rs`, both host scripts
(`commit.sh`, `commit.ps1`), the three `ApplyHost` implementations, `batch.rs`, `winps.rs`, the CLI
dispatch for `apply` and `rollback`, the render/MCP/TUI surface and the exit-code mapping in `error.rs`.
Tests were checked for skipped or vacuous cases only; the `#[ignore]` attributes found are all
live-rig gates with a stated environment requirement.

The host scripts and `local_fs.rs` are careful. Path components are validated before use, reparse points
and symlinks are checked on every walk, a staged file is re-hashed immediately before the move, secrets
stay out of events and error text, and the exit-code table agrees with the README. The weakness is in
the engine's control flow after the commit. One class of failure leaves the tree changed with neither a
restore nor a recovery pointer, which breaks the phase's central promise ("ends verified or restored").
The remaining findings are one rollback race, wrong recovery guidance, a boundary error in the hold
budget, and a missing deadline on the commit call.

Already accepted and not repeated: FA-23-05, the residual reparse race in `commit.ps1`, and the 0755
snapshot subdirectories.

## Critical Issues

### CR-01: Any error after the commit returns without a restore, a recovery hint or a rollback

**File:** `crates/ignition-core/src/deploy/apply/mod.rs:1004-1098`
**Issue:** `drive()` handles a failing `ApplyHost::commit` through `commit_failed` (journal check, recovery
command). Every later step uses a bare `?` instead. By then files have moved and the journal is on the host:

- `steps.emit(Committed)?` (1004), `steps.emit(LockVerdict)?` (1029), `steps.emit(ScanDone)?` (1045),
  `steps.emit(VerifyDone)?` (1069), `steps.emit(VerifyChecks)?` (1072): an `events.ndjson` append failure
  (disk full, run dir removed) aborts the run mid-apply.
- `lock.check(...).await?` (1027): one gateway REST error abandons the run.
- `scan_and_wait(...).await?` (1044): the file says Err means "transport or auth failure", so an auth or
  network failure after the write leaves the tree written but never scanned.
- `deploy_verify(...).await?` (1060-1068): `deploy_verify` returns `Err` for any REST refusal while
  reading back (`gather_read_back` returns on the first non-404 error), for a log read, or for a WebDev
  probe error. A single 5xx during verify therefore leaves a scanned, unverified tree.
- `ctx.host.fingerprint(...).await?` (1092), `ctx.host.finalize(&record).await?` (1097): a failure after a
  green verify returns an error for a deploy that is live and verified, with the record still `applying`.

In every case the result is an ordinary error envelope (for example `network_error`, exit 4, or
`gateway_client_error`, exit 2). The tree stays in the new state, nothing restores it, and the message
does not mention the journal or `ign deploy rollback --run <id>`. The next `plan` is refused with
`deploy_journal_present`, which is the first place the user learns of the situation. The README exit
contract lists only pre-write refusals and the `rolled_back` verdict as outcomes of a started apply, so
these exits are undocumented. No engine test covers any of them: `deploy_apply.rs` drives
`stall_scans` (a `NotComplete`, not an `Err`), a red verify and a failing restore, and the only
`fail("finalize")` is a `FaultyHost` self-test in `deploy_host_conformance.rs`.

**Fix:** Run everything between the commit and the final `Ok` inside one fallible block and route its
error through a single handler. For a verify or scan error, attempt `roll_back` with the matching cause
(its own `RollbackFailed` already covers a gateway that is down). For an event-log, fingerprint or
finalize error, convert to the interrupted-run error that `commit_failed` already builds.

```rust
// after `report.failure` handling
let post: Result<ApplyOutcome, CoreError> = async {
    // lock.check, scan_and_wait, deploy_verify, fingerprint, finalize ...
}.await;
match post {
    Ok(outcome) => Ok(outcome),
    Err(e @ CoreError::RollbackFailed(_)) => Err(e),
    Err(e) => {
        // Files moved and the journal is ours: try to put the snapshot back.
        match roll_back(ctx, opts, &scope, Some(&lock), RollbackCause::VerifyFailed,
                        steps, Vec::new()).await {
            Ok(outcome) => Ok(outcome),
            Err(_) => Err(commit_failed(ctx, run_id, plan, steps, e).await),
        }
    }
}
```

Treat a failed `finalize` after a green verify separately (report the deploy as applied with a warning,
or return the interrupted-run error); never as a plain transport error. Add engine tests that make
`resource_list`, `scan_status`, `fingerprint` and `finalize` fail once after the commit.

## Warnings

### WR-01: The rollback live-change guard is checked before the preflights and the lock, not under the lock

**File:** `crates/ignition-core/src/deploy/apply/rollback.rs:521-530` (check), `:621-637` (preflights),
`:723-745` (lock and restore)
**Issue:** `check_rollback` compares the touched files' live hashes with the record's post-run hashes
(DAPL-08) and then the run continues through the seven preflights, including `--designers wait` (up to
86400 s), then takes the lock and calls `ApplyHost::restore`. The `restore` operation in `commit.sh`,
`commit.ps1` and the local host verifies the snapshot copy and the result, but never that the live file
still equals the post-run hash. An edit or a newer apply that lands between the check and the restore is
overwritten without `--discard-live-changes`. An apply does not have this problem, because its script
checks the pre-image under the lock.
**Fix:** After `HeldLock::acquire`, before `restore_snapshot`, fingerprint the touched set again and
refuse (releasing the lock with a no-op scan) when any live hash differs from `record.post_map()` and
`discard_live_changes` is false. Better, carry the expected post hash in each `P` record so the script
refuses atomically with the move.

### WR-02: `rollback_failed` guidance names a snapshot copy that does not exist and omits the retry

**File:** `crates/ignition-core/src/error.rs:958-970` and `:1767-1771`; `apply/mod.rs:1104`, `:1518-1531`
**Issue:** The message and hint say to "restore the files from the .gwbk backup or from the run-directory
snapshot, then delete the journal ... by hand". The run directory's `snapshot/` is created only by
`fetch_snapshot`, which runs after a green apply (mod.rs:1104) or after a restore that has already
succeeded (mod.rs:1522). `rollback.rs:820` fetches into a scratch directory it deletes at once. So in the
exact case this error describes, a failed restore, `snapshot/` is absent. With `--skip-gwbk` (and for
every rollback, which takes no `.gwbk`) the text points at nothing. The only intact pre-images are the
on-box `ign-deploy/<gateway>/<mode>/snapshots/<run>/files/`, which the message never names.
The text also says to delete the journal unconditionally. Most causes in `restore_steps` (lock taken or
refused, network error, scan incomplete) leave the journal and snapshot intact, and
`ign deploy rollback --run <id> --env <env>` is a valid retry for exactly that journal. The README says
this error is the only case where the journal is deleted by hand (D-12), so an administrator following
the hint can remove the protection that makes `plan` and `apply` refuse over a half-restored tree.
**Fix:** Change the message and hint to: first retry `ign deploy rollback --run {run_id} --env <env>`;
if that reports a lost or damaged snapshot, restore from the `.gwbk` (when one was taken) or from the
on-box `snapshots/{run_id}/` directory (print its path), and only then delete the journal. Do not
mention a run-directory snapshot unless the code fetches one before it can fail.

### WR-03: The rollback `hold_budget` preflight counts the ledger, so a run the apply accepted can be impossible to roll back

**File:** `crates/ignition-core/src/deploy/apply/rollback.rs:629` against `apply/mod.rs:864`
**Issue:** Apply sizes the budget with `set.writes.len() + set.deletes.len()` (ledger excluded).
Rollback uses `record.touched.len()`, and `batch_spec` always adds the `ledger.json` entry. The hold test
pins 1527 files as the largest change that fits and 1528 as the first that does not, so an apply of exactly
1527 files passes while its manual rollback (1528) is refused with `hold_budget`. There is no override
flag by design, and the README and CLI help advertise "1,528 or more" for the apply only. The refusal
happens after the confirmation and run directory creation, and leaves no supported way to roll back.
**Fix:** Count the same quantity in both places, for example in rollback
`touched_files: record.touched.iter().filter(|e| e.path != LEDGER_KEY).count()`, or add the ledger to the
apply count and move the documented threshold to 1,527. Add a boundary test that applies and rolls back
at the limit.

### WR-04: The commit call has no deadline tied to the lock hold

**File:** `crates/ignition-core/src/deploy/target.rs:115-120`, `transport/remote.rs:1060-1092`,
`transport/scripts/commit.sh:679-689`, `commit.ps1:601-616`
**Issue:** The lock is held for `HOLD_S` = 176 s, sized as twice the p95 commit plus scan. The commit
runs as one `exec` bounded only by `ssh.op_timeout_s`, whose default is 1050 s (sized for the old
per-file commit). Nothing inside the scripts watches the clock: each held file costs up to `RETRIES`
one-second sleeps (`commit.sh` `move_one`, `commit.ps1` `MoveWithRetries`, and `LiveRetry` in the stale
check), and they are not summed against a budget. A slow or blocked host can keep moving files for
roughly 870 s after the lock lapsed. Meanwhile any actor's scan may load a partial tree, which is what
the lock exists to prevent. The engine only notices after the script returns (`LockVerdict`) and then
restores. A timeout then kills the local `ssh`, and the remote script may keep running (documented for
a dropped connection, but here it is the normal slow path).
**Fix:** Pass a deadline into the work list (for example a `B` record with the budget in seconds, derived
from `HOLD_S - LOCK_MARGIN_S`) and have both scripts stop moving after it and report a
`fail` (which the engine already turns into `write_failed` and a restore under a still-held lock). Also
cap the commit call's timeout at the hold plus a margin instead of reusing `op_timeout_s`.

### WR-05: `commit_failed` treats an unreadable journal as "nothing was written"

**File:** `crates/ignition-core/src/deploy/apply/mod.rs:794-821`
**Issue:** The doc comment says that when the journal cannot be read, "nothing is known to have been
written and the original error stands". An unreadable box is not evidence of an empty box, and the common
cause of an unreadable box here is the same dropped ssh connection that failed the commit. The user then
gets a plain `transport_failed` (exit 4) with no recovery pointer, even though the script may have
created the journal and moved files. The journal read error is also discarded (`_ => err`).
**Fix:** On `Err` from `journal_on_box`, return the original error with an appended sentence that this
run's journal may be on the host, naming `ign deploy rollback --run <id> --env <env>`, and emit
`commit_interrupted` with an `unknown` marker.

## Info

### IN-01: `deploy_journal_present` surfaces with a literal `<env>` placeholder on host-side paths

**File:** `crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs:563,583`;
`transport/local/apply_ops.rs:555`
**Issue:** The hosts build `DeployJournalPresent { env: "<env>" }`. `with_env` replaces it only for
`prepare` and `commit` (mod.rs:982, 1002). The same error from `restore`, `finalize` and
`discard_interrupted` (via `journal_of_run`) reaches `restore_steps` as `restore: ... --env <env>` inside
`rollback_failed`. The `CommitInterrupted` progress line (render.rs) also prints
`ign deploy rollback --run <run id>` without `--env`.
**Fix:** Apply `with_env` in `restore_steps` and the other host calls, or carry the env into the host
setup, and add `--env <env>` to the progress line.

### IN-02: POSIX stub hashes `work.tsv` but the script re-reads it from disk many times

**File:** `crates/ignition-core/src/deploy/transport/batch.rs:74-86`, `scripts/commit.sh:39,250,340,419,...`
**Issue:** `POSIX_STUB` checks the SHA-256 of `work.tsv` once, then `commit.sh` reads `$ST/work.tsv`
in each `while read ... done <"$WORK"` loop. The PowerShell stub hands the script the exact text it hashed,
so the "the list it acts on is the list that was checked" guarantee holds there and not here. The attacker
needs write access to the deploy account's staging directory, so this is hardening and not an exploit.
**Fix:** Have the stub load the text into a variable and give the script `WORKTEXT` (read from a here-doc
or `printf '%s'` into a temp inside the run's private control directory), or document the difference.

### IN-03: Staging left on the host when `prepare` or `commit` refuses

**File:** `crates/ignition-core/src/deploy/apply/mod.rs:978-1003`
**Issue:** Only the lock-refused branch and the stale branch call `discard_interrupted`. A `prepare`
failure part-way through the upload, a `commit` refusal that wrote no journal (journal of another run,
held file, `staged` mismatch), and an early return from `steps.emit(LockAcquired)?` all leave
`staging/<run_id>/` behind, and nothing prunes it (D-08). The content is rendered config only (embedded
secrets are refused earlier), so this is clutter, not a leak.
**Fix:** Best-effort `discard_interrupted` on every return between `prepare` and a successful commit.

### IN-04: The reserved-collection guard is case-sensitive

**File:** `crates/ignition-core/src/deploy/transport/mod.rs:552-561`
**Issue:** `mirror_mode_problem` refuses `mode == "core"` and `mode == "local"` byte-for-byte. On
Windows hosts and on a macOS `local` node the file system is case-insensitive, so `mode: Core` or `Local`
resolves to the reserved collection. The `mode_master` preflight (the gateway must report that same mode
string) makes this unlikely to be reachable, but the guard is the stated defence.
**Fix:** Compare with `eq_ignore_ascii_case`.

### IN-05: `TreeMirror` and the scripts' `mirror` operation have no production caller

**File:** `crates/ignition-core/src/deploy/transport/mod.rs:401` (`mirror_for_node`),
`sftp/apply_ops.rs:711-767`, `commit.sh` `op_mirror`, `commit.ps1` `OpMirror`
**Issue:** `grep` finds no use of `mirror_for_node` or `TreeMirror` in `actions/`, `ignition-cli` or
`ignition-tui`; only the transport tests and live tests call it. Roughly 200 lines of shell, 130 of
PowerShell and the Rust glue are shipped (and hashed into the stub) for a path the binary cannot reach,
and they must stay in sync with the apply path.
**Fix:** Keep it behind `#[cfg(test)]` or a feature, or record the intended consumer.

---

_Reviewed: 2026-10-18T14:20:21Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
