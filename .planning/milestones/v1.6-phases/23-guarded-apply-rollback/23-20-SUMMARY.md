---
phase: 23-guarded-apply-rollback
plan: 20
subsystem: deploy
tags: [apply-host, windows-ssh, powershell-5.1, commit-ps1, mirror, conformance, factory-arm, live-run, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-08 commit.ps1 (commit, restore, verify, unstage), the hash-checking stub, the ACL reset and the 23-08 reparse hardening; 23-06 commit.sh (the op set and order translated here) and RemoteShell::local_exec; 23-19 the 16-row shared table and the fault seam
provides:
  - commit.ps1 ops probe, fingerprint, finalize, discard and mirror (the same nine ops and dispatch order as commit.sh), on \\?\ paths, with index-based ASCII results; mirror moves through MoveOne so each moved file gets the ACL reset
  - the Windows ApplyHost arms probe, fingerprint, finalize, read_record, fetch_snapshot and discard_interrupted (no more "not supported by this build yet")
  - TreeMirror::commit on a Windows node as ONE mirror-op script call; the per-file rename loop, its sftp read-backs and the rename retry in run_batch are gone
  - RemoteShell::local_exec for exec_powershell (cmd /C on Windows), the PowerShell row source of deploy_host_conformance.rs (Ctx::ps) and the rows where PowerShell differs from sh
  - apply_host_for_node hands a Windows ssh node the sftp apply host (the last code commit of the plan)
  - live test wflwvm01_ps_ops and a "23-20" entry in 23-LIVE-CAPTURES.md
affects: [23-10, 23-13, 23-21, 23-14, 23-15, 23-16]

actuals:
  tokens: 34000   # chars/4 over the realized diff (136,864 characters of crates/ changes, 1,897 lines added, 800 removed)
  tasks: 2
  commits: 3      # three code commits; the docs commit follows

tech-stack:
  added: []
  patterns:
    - "a probe that must not write where the gateway watches reads the directory's access rules against the account's own token (explicit deny wins, inherit-only rules skipped) instead of creating a file"
    - "a Windows test binary is built on the Mac with cargo xwin, copied to the capture VM and run there; this ran the whole conformance table on a real Windows PowerShell 5.1 host before any CI leg"
    - "an internal ApplyHostKind enum lets a factory test tell the concrete host without a network call"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
    - crates/ignition-core/src/deploy/transport/batch.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/tests/common/sftp_server.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md

key-decisions:
  - "probe never writes into the collection: CanAddFile evaluates the directory's access list for the create-file right against the account's token. The ign-deploy side is written (a probe file made and removed), as in sh. An explicit CreateFiles deny on production was reported as 'the mode collection is not writable'. Limit: only create-file is checked."
  - "fingerprint of a file another process holds without sharing is the refusal 'held' (after the bounded retries, naming the path), never None and never a stale reading; sh has no such case."
  - "mirror needs the work list's S record (the staged-files directory) and takes it in native Windows form (C:\\...); WorkList accepts either shape and each script accepts only its own."
  - "A staged copy that is a hard link to the live file is not refused (no inode to compare); the move onto it reports success and the file keeps its bytes. sh refuses it. The row is a PowerShell-only test, not a shared row."
  - "The old Windows rename-retry in run_batch is deleted with the per-file commit: nothing renames over sftp any more, and a held target is the script's bounded retry."

patterns-established:
  - "a conformance row that differs per host is a host-specific test beside the shared table, never a cfg branch inside a shared row (that would change the local host's row on the same OS)"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-06]

duration: ~1h45 (most of it cargo builds under a load average of 100-200 and live runs over the ssh link)
completed: 2026-10-17
---

# Phase 23 Plan 20: The rest of the PowerShell host, mirror on the script, and the Windows factory arm Summary

**A Windows ssh node now runs every on-box operation (probe, commit, restore, fingerprint, finalize, discard, mirror) as one hash-checked `commit.ps1` call, the shared conformance table passes against it on a real Windows PowerShell 5.1 host, the Windows mirror commit no longer renames file by file, and `apply_host_for_node` hands out the Windows host only now.**

## What was built

- `commit.ps1` (1,236 lines, ASCII, PowerShell 5.1): `probe`, `fingerprint`, `finalize`, `discard` and `mirror`, translated from `commit.sh` in its dispatch order. `CheckWork` learned the `S` (staged-files directory) and `X` (mirror delete) records and the per-op run/gateway rules; `CheckRoots` skips the `ign-deploy` shape for a mirror (its deploy root is the account home). `commit`, `restore`, `verify` and `unstage` from 23-08 are unchanged.
- 23-08's hardening holds in every new op: `mirror` reaches the collection only through `MoveOne` (so `MoveWithRetries` re-classifies the target chain before every attempt and `ResetAcl` checks for a link before and after the descriptor change), deletes classify immediately before acting, emptied directories go through the same `RmdirUp`, trees go with .NET `Directory.Delete`. A unit test (`the_shipped_script_runs_every_operation_commit_sh_has_and_keeps_the_hardening`) pins this: one `Move-Item` in the whole script (inside `MoveReplace`), no `Remove-Item`, `OpMirror` calls `Moves`, `MoveOne` calls `MoveWithRetries` and `ResetAcl`, and the dispatch order equals `commit.sh`'s.
- Windows `ApplyHost` arms: `apply_setup_any` was folded into `apply_setup` (the refusing variant is gone). `read_record` and `fetch_snapshot` read over sftp outside the lock exactly as on Linux; `fetch_snapshot` is bound by `WINDOWS_SNAPSHOT_PATH_MAX = 259`, tested live at 259 (works) and 260 (`probe_paths` refuses). `stale_path` now also names a `P` index so a held fingerprint names its path.
- `TreeMirror::commit` is one `mirror` op call on either family (`mirror_commit_script`); `commit_inner`, `remove_emptied`, `verify_remote_copies`, `discard_staged`, `RemoteCopy`, `chain_of` and the rename retry in `run_batch` are deleted. The Windows source directory travels native (`C:\...`) in the `S` record.
- `RemoteShell::local_exec` now also runs `exec_powershell` through the local shell (`cmd /C <command text>` on Windows); `Ctx::ps()` and `ps_host` in `deploy_host_conformance.rs` build the PowerShell host over the runner's `sftp-server.exe`.
- `apply_host_for_node`: the Windows `ssh` arm returns the sftp host. The concrete host is resolved by a private `apply_host_kind` so the factory test (`apply_host_for_node_windows_ssh`) can tell the Windows sftp host from the Linux and local hosts without a network call.

## Tracer gate

Task 1 is a tracer. Auto mode is not active and the project runs `human_verify_mode = end-of-phase`; as in 23-01 through 23-19 the run continued after the tracer commit instead of stopping for a `checkpoint:human-verify`. The tracer's `<verify>` (`winps` unit tests, the conformance test, clippy) passed before the expansion step, and the working slice (every op on the live host and the whole conformance table on a Windows host) was run before the factory arm changed. For the end-of-phase list: the tracer is `wflwvm01_ps_ops` plus the PowerShell conformance rows.

## Migrated tests (D-01, T-23-78)

| Before (23-20 start) | After |
|---|---|
| `windows_commit_replaces_by_rename_and_never_renames_onto_itself` | `windows_commit_replaces_through_the_script_and_never_renames_over_sftp` |
| `windows_commit_retries_a_held_rename_a_bounded_number_of_times` (three segments: held twice then success; held for good; permission denied) | `windows_commit_holds_retry_in_the_script_and_reports_a_failed_move` (the bound travels as the work list's `T 3`; the script's failure comes back as `transport_failed` naming the path and "nothing was rolled back"; the transport calls once and never renames) |
| `windows_apply_half_refuses_the_calls_23_20_builds` | `windows_apply_calls_run_the_script_or_read_over_sftp` |
| `windows_probe_paths_...` (its last assertion pinned "not supported yet") | same name; the last assertion now expects the script probe's answer |

Windows mirror tests in `sftp.rs`: 4 before (the two above, `windows_staging_inside_the_collection_is_refused_whatever_its_case`, `windows_staged_and_deleted_paths_pass_the_windows_gate_and_write_nothing`), the same 4 after, plus the new `windows_mirror_commit_is_one_exec` (both staging modes: exactly one exec, no sftp rename, no path on the command line, the decoded parameters, the native `S` record, the `T 3` and `X` records, the report mapped back to paths). The count of Windows mirror tests before and after is equal.

The one Phase 23 commit that changed the Windows arm of `apply_host_for_node` from the refusal to the sftp host is `76107ed` (`git log -L:apply_host_for_node:...` lists `f56bce0` and `76107ed` only: 23-04 `f56bce0` created the function with the Windows refusal, 23-06 `4f7f6d3` changed the Linux arm and the refusal's wording, `76107ed` replaced the refusal). It is this plan's last code commit.

## Tests

- `cargo test --workspace --no-fail-fast` (macOS): 2,431 passed, 0 failed, 55 ignored (the known flake `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` passed this time). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean; `cargo xwin clippy -p ignition-core --all-targets --target x86_64-pc-windows-msvc -- -D warnings` clean, so the `cfg(windows)` code compiles and lints.
- New offline tests: `windows_mirror_commit_is_one_exec`, the two migrated tests, `windows_apply_calls_run_the_script_or_read_over_sftp`, `apply_host_for_node_windows_ssh`, `the_shipped_script_runs_every_operation_commit_sh_has_and_keeps_the_hardening`, a native-Windows-source case in `the_other_operations_round_trip_and_refuse_foreign_records`, `exec_powershell_with_local_exec_uses_the_local_shell_not_ssh`, `every_row_runs_against_the_powershell_host` (checks the wiring on every OS).
- Windows, run for real on the capture VM (Windows Server 2022, PowerShell 5.1.20348, Win32-OpenSSH, account `svc-ignition-deploy`) from test binaries built on the Mac with `cargo xwin --target x86_64-pc-windows-msvc` and copied there: the whole `deploy_host_conformance` binary, 47 passed, 0 failed (the 16 shared rows against the local host and against the PowerShell host, `ps_stub_refuses_a_tampered_script_or_work_list`, `ps_held_target_reports_index`, `ps_moved_file_has_only_inherited_entries`, `ps_self_move_through_a_hard_link_never_loses_the_file`, `ps_junction_target_dir_is_refused`, the fault rows); the `ignition-core` library tests, 906 passed, 13 failed, all 13 `run_store` tests that start `git`, which the VM does not have (`program not found`). `winps::tests::stub_refuses_hash_mismatch`, the 23-08 carry-over that needs a Windows runner, passed there. **This is not the windows-latest CI leg**; see "Human check owed".
- Live over ssh (`IGNITION_LIVE_WFLWVM01=1`, scratch gateway below the account home, never the real data directory): `wflwvm01_ps_ops` passed (2026-10-17T21:47Z, 582 s); after the last code change `wflwvm01_apply_script_run`, `wflwvm01_script_acl_and_reparse` (the 23-08 reparse and ACL checks) and `wflwvm01_write_half_scratch` (the Windows mirror through the script, `Separate` and `InPlace`) passed again (22:43Z, 3 passed, 511 s). The VM was left clean (no `ign-live-scratch`, `ign-deploy`, `ign-conf-run`).

## Live evidence (`## 23-20 PowerShell host ops`, `### 23-20` in `23-LIVE-CAPTURES.md`)

One script call each, under a load average of 100-200 on the runner side, single runs and not p95s: probe 5.7 s, mirror commit 5.6 s (separate staging, one file), mirror commit stopped by a held target 9.3 s (3 retries), fingerprint of a held file 7.6 s, finalize 13.8 s, `read_record` (sftp) 15.3 s, `fetch_snapshot` (sftp, 3 files) 24.7 s. A mirror-moved file ends with `protected False` and only inherited entries. A target held with `FileShare.Read` stops the mirror with `write 0: cannot move the file into place`, nothing rolled back, the file unchanged; the same commit works once it is free.

## Deviations from Plan

### Auto-fixed issues

**1. [Rule 1 - Bug] Three defects in the new conformance harness, found by running it on the Windows VM**
- **Found during:** Task 1, first run of the Windows test binary on the VM (20 of 23 passed).
- **Issues:** `tree_names` (used by the shared `delete_file_and_emptied_dirs` row) printed `\` on Windows, so the row's `/` comparison failed (this also hit the local host on windows-latest); `mklink /J` read a `/` in the path as a switch; the ACL test compared a moved file with a control file made by the test process, which gets explicit entries from the ssh session's default access list (captured, `### 23-20`).
- **Fix:** `tree_names` normalises to `/`; native separators for `mklink`; the ACL test asserts that every entry of the moved file is inherited. 47 of 47 then passed.
- **Commit:** cd89c59

**2. [Rule 2 - Missing critical functionality] `fingerprint` and a held file**
- **Found during:** Task 1 (writing the op).
- **Issue:** `commit.sh` reports a live hash and has no unreadable case; on Windows a file held without sharing cannot be hashed, and reporting it as `None` or as a hash of something else would let a stale plan through.
- **Fix:** `LiveRetry`, then the refusal `held` naming the path (`stale_path` learned `P` indices); live-tested.
- **Commit:** aa05fe6

### Plan departures, not rule-driven

- **The mirror routing landed in the first code commit, not the second.** The plan puts the Windows `TreeMirror::commit` and the migrated tests in Task 2; they share `sftp.rs` with the Task 1 arms and the live mirror test needed the script path to exist, so they are one commit (`aa05fe6`). Task 2's code commit is the factory arm alone (`76107ed`), which is what the "last commit, nowhere else" rule needs.
- **`apply_setup_any` was removed rather than kept beside `apply_setup`.** Once every arm is allowed on both families the refusing variant has no caller.
- **Deletions beyond the plan's list.** The per-file commit's helpers (`verify_remote_copies`, `discard_staged`, `remove_emptied`, `RemoteCopy`, `chain_of`) and the Windows rename retry in `run_batch` were dead after the migration (clippy and the compiler said so) and are removed; the plan says "the per-file rename path is gone".
- **The self-move and junction cases are PowerShell-only tests, not edits to the shared rows.** `self_move_refused` and `symlinked_target_dir_is_refused` are no-ops on non-unix; giving them a Windows branch would also run it against the local host on windows-latest, which is unproven. The PowerShell tests `ps_self_move_through_a_hard_link_never_loses_the_file` and `ps_junction_target_dir_is_refused` carry the Windows cases.
- **`probe` does not create a file in the collection.** The plan does not say how; `sh` uses `[ -w ]`, which writes nothing. Creating a probe file in a directory the gateway watches could be loaded as a resource, so the Windows probe reads the access list (decision above).
- **TDD.** The unit tests and the live test were committed together with the implementation (the live test needed the script to exist); there is no separate RED commit. The first live and Windows-binary runs were the red.
- **Tracer gate:** see above.
- **Trial:** not reset. This plan never touched the gateway REST; `ign --profile azure-win --json license status` at 22:43Z shows the trial expired (last reset 20:29Z). The next plan that uses the gateway (23-10) must reset it first with `ign --profile azure-win --json api call --method POST --path /data/api/v1/trial`.

## Human check owed (Task 2)

Push the branch when you choose and open the CI run for `76107ed`. **windows-latest has never run `deploy_host_conformance` with a PowerShell row source**; what is unproven there, and cannot be proven from the Mac or the VM, is whether the runner has `C:\Windows\System32\OpenSSH\sftp-server.exe` (the harness panics with the install hint and never skips if it does not) and that `sftp.exe -D` works against it (it does on the VM: `Remote working directory: /C:/Users/...`). If the runner lacks the server, the fix is to add `Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0` to the Windows leg of `.github/workflows/ci.yml`, not to loosen the harness. The run id belongs in this file once it exists: windows-latest run id: **pending (not pushed)**.

## Carry-overs

**23-10 (hold model, OQ5b):** single-run figures above, taken outside any gateway scan lock. Only `commit` and `restore` are under the lock; the post-write `fingerprint` and `finalize` (5.7-13.8 s each on this link, ssh handshake dominated) should be checked against where the engine takes them relative to the hold. The 152-file commit figure (6.92 s) is 23-08's.

**23-13 / 23-21 (verbs, tests):** a held `fingerprint` surfaces as `transport_failed` ("<path> could not be read because another process holds it open; nothing was changed"); no new slug, so the four-place rule was not triggered. A Windows `deploy apply` through `apply_host_for_node` is now reachable: nothing else in the engine refuses a Windows ssh node. `WinScratch::host()` in `live_transport_wflwvm01.rs` still builds the host directly (its comment says the factory arm is 23-20's); 23-16's `wflwvm01_cli_apply_rollback` can use the factory.

**23-14 (rollback verb, README):** `read_record`, `fetch_snapshot` and `discard_interrupted` now work on a Windows node, so `ign deploy rollback` can read and restore a Windows run. README notes (Windows): the deploy account needs create-file on the mode collection (the probe reports "the mode collection is not writable" otherwise; only that right is checked), a file another process holds open without sharing stops the apply before any move (`held`), a share that denies delete stops it after three one-second retries with a restorable partial write, and a snapshot path over 259 characters is refused by the preflight (23-08).

**23-15 / 23-16:** the PowerShell conformance rows exist (`conformance_ps_<row>` for all 17 rows of `scenarios()` plus five PowerShell-only tests); `every_row_runs_against_the_powershell_host` fails on any OS when a new row is added without one. `wflwvm01_ps_ops` is a live gate that needs the capture host and writes only the scratch gateway.

**CI:** `.github/workflows/ci.yml` is unchanged. `winps::tests::stub_refuses_hash_mismatch` and the PowerShell rows run there for the first time on the push above.

## Known Stubs

None. No `TODO`, `FIXME`, `todo!`, `unimplemented!` or skipped test in the changed files; the only `#[ignore]` tests are the live ones, which carry their reason.

## Threat Flags

None new. Within the plan's threat model: T-23-37 (one sentinel line and ASCII results; every new op prints indices, fixed words and counts, never a path or content), T-23-76 (the new ops take the op name and the three roots through the stub's base64 JSON; the only new path-bearing record, `S`, travels in the hash-checked `work.tsv` and is checked by `PlainAbs`, the overlap check with the collection, and the Rust-side `windows_native_dir_ok`), T-23-77 (the factory arm changed only in `76107ed`, after every Windows `ApplyHost` call, the mirror and the conformance rows), T-23-78 (the two pinned tests are migrated by name, counts equal) are each implemented and tested.

## Self-Check: PASSED

- `crates/ignition-core/src/deploy/transport/scripts/commit.ps1` has the nine ops: FOUND (`grep -c "'probe'\|'fingerprint'\|'finalize'\|'discard'\|'mirror'"` = 9, at least 5 as the acceptance criterion needs)
- `windows_mirror_commit_is_one_exec` in `sftp.rs`, `apply_host_for_node_windows_ssh` in `transport/mod.rs`: FOUND
- commits `aa05fe6`, `cd89c59`, `76107ed`: FOUND
- `23-LIVE-CAPTURES.md` has `## 23-20 PowerShell host ops` and `### 23-20`: FOUND
