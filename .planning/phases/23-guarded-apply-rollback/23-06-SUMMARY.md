---
phase: 23-guarded-apply-rollback
plan: 06
subsystem: deploy
tags: [apply-host, linux-ssh, posix-sh, sftp, stub, work-list, conformance, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-04 ApplyHost, WorkList and the local host; 23-19 the commit and restore rule rows; 23-02 checkpoint outcome (on-box layout, journal phases)
  - phase: 22-transport-plan-pull-verify
    provides: SftpTransport (read and Phase 22 write halves), RemoteShell, the local sftp-server seam
provides:
  - scripts/commit.sh, one POSIX sh script with the ops commit, restore, mirror, probe, fingerprint, finalize, discard and unstage
  - SCRIPT_SH, POSIX_STUB, script_sh_sha256 and the WorkList operations and records the script host needs
  - RemoteShell::exec_posix (one ssh call, bounded, one IGN-RESULT line) and the doc-hidden local_exec seam
  - impl ApplyHost for SftpTransport (Linux nodes), SftpTransport::with_apply and the doc-hidden with_home
  - Linux TreeMirror::commit as one script call; the per-file rename path is Windows-only until 23-20
  - apply_host_for_node builds the sftp host for a Linux ssh node
  - the shared conformance table run against the sh host on unix
affects: [23-08, 23-20, 23-05, 23-17, 23-12, 23-14, 23-16]

actuals:
  tokens: 40000
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "one fixed stub hashes the uploaded script and work list before it runs anything; the command line carries fixed text, the script path, the op, three roots and two hashes, never a path or a reason"
    - "the script answers with indices into the work list; the Rust side maps them back to paths, so the host never has to print one"
    - "an operation with no staging directory of its own uploads the script, the work list and its files into a throw-away control directory below the home, and the script removes that directory when it exits"
    - "the journal is read outside the lock; the script refuses when the journal on the box is not the one the work list says was seen"

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/scripts/commit.sh
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  modified:
    - crates/ignition-core/src/deploy/transport/batch.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/tests/deploy_transport.rs

key-decisions:
  - "the stub hashes the script as `$(cat script)` plus one newline, so the script must end in exactly one newline; a unit test pins that and the stub's shape (one line, no control character, no path)"
  - "operations that have no staging directory (probe, fingerprint, finalize, discard, restore, unstage and the mirror commit) upload to `<home>/.ign-ctl-<pid>-<seq>-<nanos>/` and the script removes it on exit; the runner removes it only when the script never ran"
  - "the Linux InPlace mirror commit uploads the staged copies with the script and moves them from the control directory, so on Linux InPlace is also a mv per file inside the one call"
  - "WorkList gained operations and records instead of a second format: T (retries), S (staged dir), J (journal seen), F (pre-rendered file and its hash), X (mirror delete); the version header stays `ign-work 1` because no host has shipped"
  - "the host reads the journal outside the lock (read_journal) and puts its hash in the work list; the script compares it before it overwrites or removes the journal, so a journal that changed in between is never clobbered"

patterns-established:
  - "a script host runs the same `scenarios()` rows as the local host through the local sftp-server and a local `sh -c` (the `local_exec` seam), with the account home and the data directory the same scratch directory"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-06]

duration: ~5h (most of it cargo builds under a load average above 400)
completed: 2026-10-16
---

# Phase 23 Plan 06: The Linux ssh batch, one POSIX script call under the lock Summary

**On a Linux ssh node the whole under-lock part of an apply, a restore and the Phase 22 mirror commit is one `ssh` call that runs an uploaded POSIX `sh` script behind a SHA-256-checking stub; the script and the local host agree on all 17 shared conformance rows.**

## The stub

`POSIX_STUB` in `crates/ignition-core/src/deploy/transport/batch.rs`, run as `sh -c <stub> ign-stub <script path> <op> <staging> <collection> <deploy> <script sha256> <work sha256>`, every argument single-quoted by `RemoteCommand::posix`:

```sh
set -eu; S=$1; O=$2; T=$3; C=$4; D=$5; SS=$6; WS=$7;
if command -v sha256sum >/dev/null 2>&1; then H=sha256sum;
elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256';
else echo 'ign: the host has no sha256 tool' >&2; exit 98; fi;
code=$(cat "$S") || exit 97;
a=$(printf '%s\n' "$code" | $H) || exit 97;
b=$($H <"$T/work.tsv") || exit 97;
[ "${a%% *}" = "$SS" ] && [ "${b%% *}" = "$WS" ] ||
{ echo 'ign: the staged script or work list does not match its hash' >&2; exit 97; };
exec sh -c "$code" ign-commit "$O" "$T" "$C" "$D"
```

(one line in the source). The stub reads the script into a shell variable, hashes that, and runs that same variable, so the bytes that were hashed are the bytes that run. Exit 97 is `STUB_MISMATCH_EXIT`; `exec_posix` turns it into `transport_failed` naming the op and saying nothing ran.

## The IGN-RESULT JSON shapes

One line, ASCII, `IGN-RESULT:{...}`. Entry results are indices into the work list; no path or file content is ever printed.

| Op | Success | Refusal (nothing written) |
|---|---|---|
| commit, restore | `{"ok":1,"phase":"snapshotting\|writing\|written\|restoring","stale":["W3","D1","L"],"moved":[i],"deleted":[i],"dirs":[[delete index, levels]],"fail":null\|"write 2: ..."}` | `{"ok":0,"err":"journal_present\|staged\|journal\|snapshot\|record\|no_snapshot\|journal_changed\|bad_work\|bad_roots\|unsafe","detail":"...","bad":["W2","F..","L"]}` |
| mirror | `{"ok":1,"moved":[i],"deleted":[i],"dirs":[[i,levels]],"fail":null\|"write 1: ..."}` | `err` of `symlink`, `not_regular`, `not_file`, `is_dir`, `resources`, `coll_not_dir` or `staged`, with `at` (`W3`, `X1`, `R`, `C`) or `bad` and `removed` |
| fingerprint | `{"ok":1,"hashes":["<sha>","-"]}` in `P` order, `-` absent | `err:"not_regular"` with `at` |
| probe | `{"ok":1,"problems":["coll_unwritable",...]}` | |
| verify, unstage, finalize, discard | `{"ok":1}` | as above |

`fail` is a fixed sentence with an index (`write 2: cannot move the file into place`). A pre-image mismatch is `stale` non-empty with phase `snapshotting`; the journal and the partial snapshot are already removed.

## Work list additions (still `ign-work 1`)

`O` now names one of `commit restore mirror probe fingerprint finalize discard unstage`. New records: `T <retries>` (at most 10), `S <abs dir>` (mirror's staged files), `J <sha or ->` (the journal seen; required for restore, finalize and discard), `F <name> <sha256>` (a pre-rendered journal or record file), `X <index> <path>` (a mirror delete). `R` is `-` for probe, fingerprint and mirror, and `G` is `-` for mirror. A `WorkList` is validated the same way on build and on parse, and the shell script re-validates every record before it acts on one.

## Operations and rules the script shares with the local host

commit (exclusive journal create, pre-image check of every touched path, snapshot with verification, record, journal `ready` then `writing`, moves, deletes and emptied directories deepest first and never `<module>/<type>`, ledger last, journal `written`), restore (journal `restoring` first, copy back and verify, delete created files, ledger last, already-back paths untouched so a second restore changes nothing), fingerprint (read-only, `!` for a non-regular file), finalize, discard (refuses past `ready` on the Rust side), probe. A self-move is refused with `-ef`; a symlink on any path is refused by a component walk with `[ -L ]`; a staged copy is hashed before any move; a moved file is `chmod 644` first and a created directory is 0755 (umask 022).

## Conformance

`tests/deploy_host_conformance.rs`: every one of the 17 shared rows has a `conformance_sh_<row>` test (unix), plus `every_row_runs_against_the_sh_host`, which fails when a row is not wired. Added rows: `stub_refuses_a_tampered_script`, `stub_refuses_a_tampered_work_list`, `sh_moved_files_are_0644`, `sh_failed_move_reports_index`. The host is `SftpTransport` over `RemoteShell::local_sftp_server(..).local_exec()` with the account home equal to the data directory, so the uploads are real sftp protocol and the script is run by a real local `sh`.

Mutation checks on `commit.sh` (each reverted): self-move guard off fails `conformance_sh_self_move_refused`; emptied-directory floor lowered to depth 2 fails `conformance_sh_delete_file_and_emptied_dirs` and `conformance_sh_add_one_file`; a blind stale check fails `conformance_sh_pre_image_mismatch_aborts_before_any_move`, `conformance_sh_directory_where_file_goes_is_stale` and `conformance_sh_symlinked_target_dir_is_refused`.

## Migrated tests

The Linux mirror tests before (13 in `tests/deploy_transport.rs` `mod sftp_mirror`, plus the POSIX segment of one Windows lib test) and after (14 plus one lib test). None deleted.

| Old name | New name | What changed |
|---|---|---|
| `sftp_mirror_separate_and_inplace` | same | same collection bytes and report as the local mirror; now also one exec and no control directory left |
| `sftp_mirror_never_suppresses_errors` | same | no batch line starts with `-`, `@-` or `!`; the vanished-file case now vanishes just before the script runs, so the script's report (not an sftp `rm`) is the failure, and the file it had already deleted is the only difference |
| `sftp_mirror_existence_first_mkdir` | `sftp_mirror_script_creates_only_missing_directories` | no sftp `mkdir` touches the collection; created directories are 0755 and files 0644 |
| `sftp_mirror_delete_set_refusals_write_nothing` | same | |
| `sftp_mirror_refuses_a_staging_root_inside_the_collection` | same | |
| `sftp_mirror_symlinks_are_refused_and_never_followed` | same | |
| `sftp_mirror_tampered_staged_copy_is_refused` | same | the script's SHA-256 check names the file; a wrong-size and a missing copy both end in the SHA-256 refusal |
| `sftp_mirror_same_size_tampered_staged_copy_is_refused` | same | staged copies are removed by the script and the report says so |
| `sftp_mirror_tamper_after_the_size_survey_is_refused` | `sftp_mirror_tamper_before_the_script_runs_is_refused` | the swap lands just before the script runs |
| `sftp_mirror_tamper_between_pre_verify_and_rename_is_caught_after_commit` | `sftp_mirror_staged_copy_replaced_by_a_symlink_is_refused` | in a one-call model no runner step sits between the script's own verification and its moves, so that scenario cannot be staged; the closest reachable one (a staged copy swapped for a symlink to a file outside staging) is pinned instead. The old scenario's guarantee, that a changed copy never reaches the collection, now rests on the script hashing and moving without a gap that the runner can enter |
| `sftp_mirror_oversize_swap_is_refused_before_any_rename` | same | |
| `sftp_mirror_inplace_verifies_after_the_write_and_stops_before_deletes` | `sftp_mirror_inplace_swap_of_the_uploaded_copy_is_refused_before_any_write` | InPlace now uploads the copies into a control directory and the script checks them before any write, a stronger guarantee than the old post-write check; an honest InPlace commit is still run |
| `sftp_mirror_hostile_but_legal_names_are_inert` | same | |
| (new) | `sftp_mirror_commit_is_one_script_call_and_renames_nothing` | D-01: exactly one exec, no sftp `rename`, no collection path on the command line |
| lib `windows_commit_retries_a_held_rename_a_bounded_number_of_times` (POSIX segment) | `linux_commit_never_renames_over_sftp_and_reports_a_failed_move` | the Windows segments stay; the POSIX one now asserts zero sftp renames, one exec and that a move that still fails is `transport_failed` naming the path and saying nothing was rolled back |
| (new) | `linux_mirror_commit_is_one_exec`, `apply_host_for_node_linux_ssh`, `exec_posix_failures_are_transport_failed`, `exec_posix_runs_ssh_with_the_quoted_command_and_parses_the_sentinel` | |

Count of Linux mirror integration tests: 13 before, 14 after.

## Task Commits

1. **Task 1 (tracer), RED:** `e35556f` (the add-one-file row and the two tamper rows fail on `not built yet`; `exec_posix`, `local_exec` and the `WorkList` records land)
2. **Task 1 (tracer), GREEN:** `16bba41` (`commit.sh` commit and restore, the stub, `prepare`, `commit`, `restore`)
3. **Task 2, RED:** `e2f826a` (sixteen `conformance_sh_*` tests, the modes and failed-move rows; six failed on the unbuilt methods)
4. **Task 2, GREEN:** `7337415` (probe, fingerprint, finalize, discard, read_record, fetch_snapshot)
5. **Task 3:** `4f7f6d3` (the mirror op, the Linux `TreeMirror::commit`, the factory arm, the migrated tests)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

Auto mode is not active (`workflow._auto_chain_active` and `workflow.auto_advance` are false), so the interactive rule asks for a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase`, the orchestrator directed a sequential run, and the slice has no human-verifiable surface (an ApplyHost over a local sftp-server and `sh`), so, as in 23-01, 23-02, 23-04, 23-05, 23-18 and 23-19, execution continued. The tracer's `<verify>` (`conformance_sh_add_one_file`, the two `stub_refuses_*` tests and the `remote` lib tests) passed before the tracer GREEN commit `16bba41` and again after Task 3.

## Test results

- `cargo test -p ignition-core --test deploy_host_conformance`: 46 passed (17 local rows, 17 sh rows plus 4 added, the 2 wiring guards, 3 fault rows, 3 FaultyHost rows)
- `cargo test -p ignition-core --test deploy_transport`: 63 passed
- `cargo test -p ignition-core --lib deploy::transport`: 127 passed
- `cargo test --workspace --no-fail-fast`: 2326 passed, 4 failed, 51 ignored. The 4 failures are `rig_up_with_module_json_reports_provisioned_modules`, `rig_up_after_undeclaring_reports_the_orphan_and_deletes_the_override`, `rig_reset_after_undeclaring_prints_the_removal_command` and `rig_up_after_undeclaring_prints_the_removal_command_for_humans` in `ignition-cli/tests/contract_rig.rs`; each fails with "Cannot connect to the Docker daemon at unix:///Users/pmannion/.orbstack/run/docker.sock", an environmental failure (OrbStack not running) in code this plan does not touch. Not fixed, recorded as unrun against a live Docker daemon.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
- `sh -n` and `dash -n commit.sh`: clean; `shellcheck -s sh commit.sh`: clean. `checkbashisms` is not installed here, so the plan's optional bashism check is replaced by running the suite with `dash` as `sh` (`PATH=<dir with sh -> dash>`) and with a PATH that has `shasum` and no `sha256sum`: the 22 sh rows pass under both.
- Acceptance: `grep -n 'pub const SCRIPT_SH' batch.rs` prints two lines, `SCRIPT_SH` and `SCRIPT_SH_FILE` (the file name constant shares the prefix); `grep -n 'pub const SCRIPT_SH:'` prints one.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] The runner could not know which journal the script would overwrite**
- **Found during:** Task 1 design (restore, finalize and discard overwrite or remove `journal.json`)
- **Issue:** the plan's work list had no way to say which journal the engine had looked at, so the script could clobber a journal another run created between the host's read and the script's run (DAPL-09).
- **Fix:** the `J` record (the journal's hash, or `-`), set by the host from `read_journal` and compared by the script before it touches the journal; a journal of another run is `deploy_journal_present` before the script is run.
- **Files modified:** `batch.rs`, `sftp/apply_ops.rs`, `scripts/commit.sh`
- **Commit:** 16bba41, 7337415

**2. [Rule 3 - Blocking] The mirror needed its own work list shape**
- **Found during:** Task 3
- **Issue:** the plan says to add op `mirror` to `commit.sh` and `WorkList`, but a mirror has no run, no gateway, no journal and its staged files are anywhere (`Separate` root) or only local (`InPlace`).
- **Fix:** `R` and `G` are `-` for it, `S` names the staged-files directory, `X` carries the delete set; `InPlace` uploads the local staged copies into a control directory with the script. See "Work list additions".
- **Committed in:** 4f7f6d3

**3. [Rule 1 - Bug] The control directory leaked once the script moved a staged copy**
- **Found during:** Task 3 (integration tests)
- **Issue:** after the script moved a file out of the control directory, the runner's per-file `rm` cleanup failed and the directory stayed.
- **Fix:** the script removes its own `.ign-ctl-*` directory on exit (`trap ... EXIT`); the runner cleans up only when the script never ran.
- **Committed in:** 4f7f6d3

**4. [Rule 3 - Blocking] `commit_inner` could not be reused for the Linux arm**
- **Found during:** Task 3
- **Issue:** `commit_inner` mixes the pre-I/O validation the plan says to keep with the per-file flow the plan says to remove.
- **Fix:** the validation became `mirror_precheck` (owner, staging root, Windows gate, mirror rules, batch-line paths, local staged copies re-hashed), used by both the script path and the Windows per-file path; `commit_inner` keeps the Windows flow unchanged until 23-20.
- **Committed in:** 4f7f6d3

---

**Total deviations:** 4 auto-fixed (1 missing critical, 2 blocking, 1 bug). **Impact:** no scope change.

**Other plan departures, not rule-driven:**
- The plan asks the tracer's `prepare` to confirm each upload's SHA-256 "with one fingerprint-style script call outside the lock". That call is the script's `verify` check, which runs in the same stub (hash of script and work list) and checks every staged copy, the pre-rendered files and the ledger against the work list. It is an internal check op on the commit work list, not a separate script op, so `verify` is not in the ops list of `WorkOp`.
- `read_record` and `fetch_snapshot` use the sftp read path (descend, listing, `get`, hashing here), not the script, as the plan says. The snapshot copies are set to 0600.
- The plan's `exec_posix(command)` takes an extra `op` argument so every error names the operation (the plan's own acceptance wording, "naming the op").
- `requirements-completed` is empty, as in 23-04, 23-05 and 23-19: DAPL-05 and DAPL-06 describe user-visible apply and rollback, and this plan lands one host of the seam. No `requirements mark-complete` was run.

## Auth Gates

None.

## Known Stubs

None. `grep` for `todo!`, `unimplemented!`, `TODO`, `FIXME` and `not built yet` over `commit.sh`, `sftp/apply_ops.rs` and `batch.rs` finds nothing. A Windows ssh node's `ApplyHost` calls return `transport_failed` ("apply over ssh to a Windows host is not supported by this build yet") and `apply_host_for_node` refuses a Windows node; that arm is 23-08 and 23-20 and is documented in the factory's doc comment.

## Threat Flags

None beyond the plan's register. T-23-24 (injection through paths or names): the remote command holds only fixed text, the script path, the op, three absolute roots checked by `safe_remote_path` and two hashes, all quoted by `RemoteCommand::posix`; paths travel only in `work.tsv` and `sftp_mirror_commit_is_one_script_call_and_renames_nothing` and `linux_mirror_commit_is_one_exec` assert no collection path appears on the command line. T-23-25 (script or work list altered in staging): `stub_refuses_a_tampered_script` and `stub_refuses_a_tampered_work_list`. T-23-26 (disclosure through errors): `exec_posix` never puts stdout in an error (the test plants `SECRETSTDOUT` in stdout and asserts it never appears) and script-supplied text is reduced to printable ASCII and capped at 200 characters. T-23-27 (gateway cannot read moved files): `chmod 644` before each move and 0755 directories, pinned by `sh_moved_files_are_0644`.

## Residuals recorded as unproven

- **FA-23-05, unproven against a live Linux gateway host.** No Linux gateway host was captured. File modes (0644, 0755), `mv` across file systems (the account home and the data directory may be different file systems on Linux, which makes the staged-file move a copy and delete instead of an atomic rename), and the sha tool are proven only on macOS through a local `sh`, with `dash` as `sh` and with `shasum` only. README text for 23-14 must say so (research Open Question 8).
- A shell cannot open a path relative to a descriptor, so every check in the script (`[ -L ]` walk, `-ef`, hash) is followed by the action a moment later. A writer with access to the collection can race that gap. The local host closes it with `O_NOFOLLOW` descriptors. This is documented at the top of `commit.sh`.
- `commit.sh` is not run under `busybox ash` or a real `ssh` login shell here. The remote command is passed through `ssh` as one argument to the account's login shell, which must be POSIX (`sh`, `bash`, `dash`); a csh or fish login shell is not handled and is not detected.
- The Windows build of the new Rust code (`sftp/apply_ops.rs` is not cfg-gated, only the sh rows are `#[cfg(unix)]`) was not compiled or run on Windows; the Windows host is `pending` (23-01).

## Next Phase Readiness

- 23-08 and 23-20 port the stub, the work list and the index-based result to PowerShell; `WorkList`, `exec_posix`'s sentinel parse and the conformance table are the contract.
- 23-05 and 23-17 can drive a Linux ssh node through `apply_host_for_node`; the engine's journal-present handling already fills `env`.
- 23-14 owns the README sentence for FA-23-05.

## Self-Check: PASSED

- Created files exist: `scripts/commit.sh`, `sftp/apply_ops.rs`, this SUMMARY.
- Commits `e35556f`, `16bba41`, `e2f826a`, `7337415`, `4f7f6d3` exist in `git log`.
- `grep -n 'pub const SCRIPT_SH:' crates/ignition-core/src/deploy/transport/batch.rs` prints one line; `grep -n 'pub async fn exec_posix' crates/ignition-core/src/deploy/transport/remote.rs` prints one line; `grep -n 'impl ApplyHost for SftpTransport' crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs` prints one line.
