---
phase: 25-guarded-project-deploy
plan: 13
subsystem: deploy
tags: [deploy-projects, release-record, sftp, ssh, windows, atomic-replace, live-capture]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-11 ReleaseRecordWriter trait and release_writer_for_node (Ssh arm refusing); 25-08 SftpTransport ProjectHost, account_home, windows_native_path, the live host test harness; 25-09 ReleaseRecord::parse"
provides:
  - "impl ReleaseRecordWriter for SftpTransport on POSIX and Windows nodes: names and bytes held to the read's rules, every level listed (symlink, non-directory, odd record, leftover temp refuse transport_failed before any byte), missing levels made with sftp mkdir, bytes put to release.json.tmp-<run_id>, replaced in one step, temp removed on failure"
  - "The Ssh arm of release_writer_for_node builds the sftp writer; deploy projects can now record on an ssh node"
  - "RemoteShell::exec_posix_status: an exit-status-only exec for a program that prints nothing (mv)"
  - "live_record_write_lnxvm and live_record_write_winvm (ignored, quiet skip), both run green; ### Record write (25-13) holds both rows"
  - "README: the release record is written only in the deploy account's home, atomically, with no write right on the data directory needed for it"
affects: [25-14, 25-15, 25-17, 25-18, 25-19]

actuals:
  tokens: 15252   # chars/4 over `git diff 659cdbd~1..HEAD -- crates README.md` (61008 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "an ssh write is: list every level, mkdir the missing ones, put to a temp name beside the target, one fixed replace command, rm the temp on any failure"
    - "the Windows replace is a fixed WinScript with native paths as base64 JSON parameters, never in the script text"
    - "live tests run setup and cleanup as the deploy account over ssh (powershell -EncodedCommand on Windows), with no az vm run-command"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/transport/sftp/project_ops.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/src/deploy/transport/projects.rs
    - crates/ignition-core/tests/live_project_host.rs
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md
    - README.md

key-decisions:
  - "The temporary suffix is the record's own run_id, validated by ReleaseRecord::parse (RunId::parse), so no getrandom fallback exists."
  - "The Windows replace is Move-Item -Force through WinScript::move_replace, as the plan names it."

patterns-established:
  - "exec_posix_status for fixed commands whose success is the exit code"

requirements-completed: []  # DPRJ-03 stays open: see Requirement status

duration: "tracer by the first executor, then a continuation of about 25 minutes of work (Windows branch, tests, two live runs, docs)"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 13: Release record writer over ssh Summary

**`deploy projects` can now record its release on Linux and Windows gateway hosts: the sftp `ReleaseRecordWriter` lists every level, makes the missing ones, puts the bytes to `release.json.tmp-<run_id>` in the deploy account's home and replaces the record with `mv -f --` (Linux) or a fixed `Move-Item -Force` script (Windows), removing the temp file on failure; both families wrote, read back (equal sha256) and cleaned up live.**

## Accomplishments

- **Task 1, tracer (`659cdbd`, accepted at the human-verify gate: "Accept, continue (Recommended)").**
  - POSIX `ReleaseRecordWriter for SftpTransport`, the Ssh arm of `release_writer_for_node`, `RemoteShell::exec_posix_status`, the `sftp_record_write_posix`, `sftp_record_write_refusals` and `sftp_record_write_failure_cleans_temp` unit tests, `live_record_write_lnxvm` and the Linux row of `### Record write (25-13)`. The Windows branch refused with `transport_failed` until Task 2.
- **Task 2 (`c85943f`).**
  - `replace_record` Windows branch: `WinScript::move_replace(windows_native_path(temp), windows_native_path(final))` through `exec_powershell`; the sftp spelling `/C:/Users/<account>/...` from `account_home()` becomes `C:\Users\<account>\...` and travels as base64 JSON parameters. The answer must carry `ok: 1`. A failed move takes the existing path: `rm` of the temp file, then `transport_failed` naming `~/ign-deploy/<gateway>/projects/<project>/release.json`.
  - `sftp_record_write_windows` (unit, in the sftp.rs tests): the write order (four `mkdir`, one `put`, one exec), the script text equal to the fixed `MOVE_REPLACE` and free of any path, the exact native `src` and `dst` parameters, read-back equal, no temp left, nothing under the data directory; an overwrite makes no directory; a failed move leaves put then rm and the previous record byte for byte; a link on the way refuses before any write; device names, trailing dot, stream colon and `COM1` refuse before any session or exec.
  - `live_record_write_winvm` (ignored, quiet skip). `ssh_exec` is now shared by `deploy_sh` and the new `deploy_ps`.
  - Windows row and notes in `### Record write (25-13)`; one README sentence in the project account-scope paragraph.

## Live results (ign-win-836, deploy account `svc-ignition-deploy`)

| Step | Result | Wall time |
| --- | --- | --- |
| read before any write | `None` | not timed |
| write record 1 (four `mkdir`, one `put`, one `Move-Item -Force`) | ok | 8975 ms (re-run 8457 ms) |
| read back record 1 | sha256 written `2029e57e...ee6de`, read back equal | not timed |
| overwrite with record 2 | ok | 12371 ms (re-run 12347 ms) |
| read back record 2 | sha256 written `ecf36ae9...d7ab5f`, read back equal | not timed |
| directory after the overwrite | `release.json` only, `size=257` | |
| `<data_dir>\ign-deploy\cap25-gw` | does not exist | |
| cleanup | test guard printed `cap25 entries left: 0; base-absent`; a separate ssh session counted `*cap25*` and `*ign-deploy*` entries under the home (depth 3): `left=0`, `base-absent` | |

Runs: 2026-10-23T16:17:36Z (52.89 s) and 2026-10-23T16:33:12Z (59.04 s, after a clippy-only edit). The Linux run from Task 1 (2026-10-23T14:39:05Z, 9342 ms and 15525 ms) is in the same section. No `az vm run-command`, no `cmd /c del`, only `CAP25-*` and `cap25-*` names, and neither VM was deallocated.

## Test results

Build environment: `CARGO_TARGET_DIR` in the session scratchpad.

- `cargo test -p ignition-core --lib sftp_record_write`: 4 passed (`posix`, `refusals`, `failure_cleans_temp`, `windows`).
- `cargo test --workspace`: 2847 passed, 0 failed, 77 ignored. This covers the plan's verify commands: `deploy::transport` lib tests, `deploy_transport` (`write_half_unreachable`, `release_writer_is_confined`), `live_project_host --no-run` (compiles), and `deploy_family_coverage` (9 passed).
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Without `IGNITION_LIVE_LNXVM` or `IGNITION_LIVE_WINVM`, `cargo test -p ignition-core --test live_project_host -- --ignored` passes 4 tests as quiet skips.
- Mutation check for Task 2: swapping `src` and `dst` in the Windows replace made `sftp_record_write_windows` fail; the original was restored before the commit.

## Deviations from Plan

1. **[Rule 3 - Blocking] Added `RemoteShell::exec_posix_status` in remote.rs.** `exec_posix` needs an `IGN-RESULT:` line and `mv` prints none. Commit `659cdbd`.
2. **Temp suffix is the record's `run_id`.** It is validated by `RunId::parse` inside `ReleaseRecord::parse`, so the plan's `getrandom` fallback was not written. Commit `659cdbd`.
3. **Windows was refused between Task 1 and Task 2.** The tracer's `replace_record` returned `transport_failed` on a Windows node until `c85943f`; no release contained that state.
4. **The unit tests are in the sftp.rs tests, not project_ops.rs,** because `FakeSftp` is private there.
5. **Tests were written alongside the code, not strictly first.** Mutation runs (Task 1 and the src/dst swap in Task 2) showed they fail when the code is broken.
6. **The failure detail mentions a possible leftover temp file when the `rm` fails** after a failed replace (`the temporary file <name> may still be there`).
7. **Cost.** About 9 s for a first write and 12 to 15 s for an overwrite, from one ssh connection per step (listing, mkdir batch, put, replace). Not optimised; the user saw the Linux figures and accepted them.
8. **`IGN_LNXVM_ADMIN_KEY` is not in the env file,** and the record test does not use it.
9. **`deploy::transport::process::tests::timeout_kills_child`** fails when run alone in this sandbox (seen by the first executor) and passes in the full run; its file is untouched. Logged in `.planning/phases/25-guarded-project-deploy/deferred-items.md` (created by this plan).
10. **Live test setup on Windows uses the deploy account over ssh,** not the `az vm run-command` route the `.git` probe test uses, as the continuation brief required. `ssh_exec` was extracted from `deploy_sh` so both families share it (a refactor of test code only).

No auth gates, package installs or checkpoints beyond the tracer gate.

## Open caveat: Windows replace atomicity is not proven

The Linux replace is `mv -f --` (rename(2) on one filesystem, atomic). On Windows the plan names `Move-Item -Force`. The live run shows it replaces an existing file and leaves no temp file, and the unit tests pin the order and the failure cleanup, but nothing in this plan provoked a concurrent reader on a real Windows volume, and I did not establish whether `Move-Item -Force` over an existing file is a single `MoveFileEx(REPLACE_EXISTING)` or a delete then move. The README sentence and the module doc say "in one step" for both families, as the plan's truth does. If an atomic Windows guarantee must be defended, a follow-up could swap the script body for `[IO.File]::Replace` or `File.Move(..., true)`, and 25-19 could add a concurrent-reader probe. The readers fail closed on an unparseable or missing record (25-09), so the worst case of a non-atomic window is a refused read, not a wrong decision.

## Requirement status

`requirements-completed` is empty on purpose and DPRJ-03 was not marked complete in REQUIREMENTS.md. DPRJ-03 is "each project is exported before it is overwritten and verified after import by canonical hash". This plan delivers the record-keeping half on ssh nodes (so a deploy to an ssh node no longer stops at the writer), but later plans still own parts of the requirement as the roadmap lists them:

- **25-14** adds the Designer-session and wrong-target preflights (frontmatter `requirements: [DPRJ-03]`).
- **25-15** adds the children sweep (`[DPRJ-02, DPRJ-03]`).
- **25-17** adds the secret canary and stdout purity for the verb (`[DPRJ-03, DPRJ-04, DPRJ-05]`).
- **25-19** proves the export-before and verify-after contract on a real 8.3.6 rig (`[DPRJ-01 ... DPRJ-05]`).

A requirement is marked only by the plan that finishes it; none of those has run.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-25-40 (torn record) is mitigated by the temp-and-replace order and temp cleanup on failure, with the caveat above for Windows; T-25-41 (symlink redirect) by listing every level and refusing links, non-directories and a non-regular record before any byte (`sftp_record_write_refusals`, and a link case in the Windows test); T-25-42 (injection) by the fixed `mv -f --` argv and the fixed Windows script with parameters (the test asserts no path appears in the script text); T-25-43 (writes outside the home) by the single target below `account_home()` (the live runs show `data-dir-untouched` on both hosts).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/deploy/transport/sftp/project_ops.rs`, `crates/ignition-core/src/deploy/transport/sftp.rs`, `crates/ignition-core/src/deploy/transport/remote.rs`, `crates/ignition-core/src/deploy/transport/projects.rs`, `crates/ignition-core/tests/live_project_host.rs`, `.planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md`, `.planning/phases/25-guarded-project-deploy/deferred-items.md`, `README.md`: found.
- Commits exist: `659cdbd`, `c85943f`: found in `git log`.
- No `todo!`, `unimplemented!` or skipped test added; the two live tests are `#[ignore]` with a quiet skip, as the plan requires.
- Zero `cap25` entries remain on ign-win-836 or ign-lnx-836 (checked by an independent ssh session as the deploy account). No background work is left running.
- `.planning/config.json` and `.planning/research/.cache/` were not staged.
