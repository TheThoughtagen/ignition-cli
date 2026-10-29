---
phase: 23-guarded-apply-rollback
plan: 23
subsystem: deploy-transport
tags: [sftp, windows, 8dot3, short-names, survey, live-gate, conformance]
gap_closure: true
gap_ids: [G2]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: the Windows ApplyHost (commit.ps1), the PowerShell conformance rows (23-20), the marker-led listings (23-22)
provides:
  - SftpTransport::survey_root, the survey base of put_tree and stage_inner (the account home when the root lies below it)
  - a mkdir-specific class text for an sftp Failure
  - wflwvm01_short_name_home_prepare, a live gate for an 8.3-spelled account home
  - the G2 root-cause record and the windows-latest rehearsal on ign-win-836
affects: [23-24, phase 23 verification, windows-latest CI]

actuals:
  tokens: 60000
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "a staging root below the account home is surveyed from the home the server named, never from the drive root"
    - "the FakeSftp 8.3 alias knob: a short prefix that resolves to a long one while listings show only long names"

key-files:
  created:
    - .planning/phases/23-guarded-apply-rollback/23-23-SUMMARY.md
  modified:
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/remote.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md

key-decisions:
  - "engine fix, not a test-only change: an 8.3-spelled home fails every Windows prepare, the drive-root survey lists every ancestor of the home (a hardened host may deny that), and canonicalising the conformance temp dir would hide the case on the one runner that exercises it"
  - "a failed sftp mkdir keeps slug transport_failed; only the class text changes, so no error slug, exit code or README exit-table row moves"
  - "no tolerance for a refused mkdir was added: the fix removes the mkdir at or above the home"

requirements-completed: []

duration: ~2h45m
completed: 2026-10-19
---

# Phase 23 Plan 23: 8.3 short-name home fix (G2) Summary

**`prepare` surveys a staging root below the account home from the home, so a home spelled `C:\Users\RUNNER~1` no longer fails with a `mkdir` the server refuses; the failure is reproduced on ign-win-836, fixed in the engine, and the 47 conformance tests pass there under an 8.3 TEMP.**

## Status: COMPLETE

Every must-have is met except one live gate that the execution instruction excluded (`lnxvm_apply_script_run` on ign-lnx-836; see Deviations). `requirements-completed` is left empty (human_verify_mode end-of-phase). The windows-latest run itself is plan 23-24's.

## Root cause (evidence in 23-LIVE-CAPTURES.md `## G2: 8.3 short names (windows-latest)`)

- `put_tree` and `stage_inner` surveyed the staging root from `/<first component>` (`/C:`) and listed every ancestor of the account home. The listing of `/C:/Users` shows only the long name of the home directory; the survey therefore took the short-spelled component as missing and `make_dirs` issued `mkdir "/C:/Users/<SHORT>"`, which the server refuses with `Failure`. `require_success` labelled every sftp `Failure` "a held or non-empty target".
- On ign-win-836 (2026-10-19, UTC): the home has a short name (`for %I in ("%USERPROFILE%") do @echo %~sI` gives `C:\Users\SVC-IG~1`; `dir /x C:\Users` shows `SVC-IG~1  svc-ignition-deploy`); the sftp listing of `/C:/Users` names `svc-ignition-deploy` and no `SVC-IG~1`; `mkdir "/C:/Users/SVC-IG~1"` answers `remote mkdir "/C:/Users/SVC-IG~1": Failure`.
- Engine level, before the fix: `wflwvm01_short_name_home_prepare` failed in `prepare` with `exit code 1 (refused with no reason: a held or non-empty target): remote mkdir "/C:/Users/SVC-IG~1": Failure`, the CI text with `SVC-IG~1` for `RUNNER~1`. Offline, the new fake test reproduced the exact CI string before the fix.
- No SYSTEM fallback (`az vm run-command`) was needed; the home already had a short name.

## What changed

- `crates/ignition-core/src/deploy/transport/sftp.rs`
  - `SftpTransport::survey_root(&self, root) -> Result<(String, Vec<String>), String>`: reads only the cached home (opens no session). A root below the home is surveyed from the home (the first thing listed), with the components after it; any other root keeps `/<first component>`. It also refuses a root with no components (the old code indexed `parts[0]`).
  - `stage_inner` (the phase-22 mirror staging) uses it.
  - FakeSftp: `Knobs.alias: Option<(short, long)>` and `FakeSftp::resolve`. A path under the short prefix is the long directory for `ls`, `get` and every mutating line; listings show only stored long names; `mkdir` of an existing resolved path answers `Failure`, with the path as the client typed it.
  - Five tests (all in `sftp::tests`): `prepare_below_a_short_name_home_makes_nothing_at_or_above_it`, `prepare_lists_nothing_above_the_home_posix`, `a_staging_root_outside_the_home_keeps_the_root_survey`, `a_symlink_below_the_home_is_still_refused`, `mirror_stage_below_the_home_surveys_from_the_home`. RED before the fix: four failed (the short-name test with the CI string; the other three on the listing of `/` or `/home`); the outside-the-home test passes either way and guards the old behaviour.
- `crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs`: `put_tree` uses `survey_root`; `put_tree` and `staging_abs` are `pub(super)` so the unit tests drive the exact code `prepare` runs (`apply_roots` / `staging_abs` / `put_tree`).
- `crates/ignition-core/src/deploy/transport/remote.rs`: `failing_command_is_mkdir`; in `require_success`, a `Failure` whose stderr line starts `remote mkdir ` reads "(refused with no reason: the directory may already exist, for example under an 8.3 short spelling, or cannot be created there)". Every other command keeps "a held or non-empty target". Test `a_mkdir_failure_is_not_called_a_held_target` (RED first, then green); `require_success_names_the_sftp_failure_class_for_exit_1_only` is unchanged and passes. Slug `transport_failed`, no exit-table change (`git diff 14d0b01 -- crates/ignition-core/src/error.rs README.md` is empty).
- `crates/ignition-core/tests/live_transport_wflwvm01.rs`: `wflwvm01_short_name_home_prepare` and `WinScratch::short_home_host` (a second host over the same scratch gateway whose home and data dir are spelled with the short name; the scratch tree is built by `WinScratch::new`, not copied). The short form comes from `Scripting.FileSystemObject` over `exec_powershell`, or `IGN_WFLWVM01_SHORT_HOME`. It runs `prepare`, `commit` and `restore` of one new file and asserts the file arrived and the restore returned the pre-run listing.
- No existing unit test pinned an ancestor listing of the home, so none needed an update. `deploy_host_conformance.rs` was not changed: it keeps the runner's own temp path, so windows-latest still covers an 8.3-spelled home.

## Commits

- `1a81e0f` test(23-23): live gate for prepare below an 8.3-spelled account home, and the G2 root cause record
- `f0dced9` fix(23-23): survey a staging root below the account home from the home, never from the drive root
- `3a47279` fix(23-23): a failed sftp mkdir is not called a held or non-empty target
- the final docs commit (this summary, the LIVE-CAPTURES rehearsal record, STATE and ROADMAP)

## Live results (ign-win-836, 2026-10-19)

| Gate | Result |
|---|---|
| `wflwvm01_short_name_home_prepare`, before the fix | FAIL, the CI text (`remote mkdir "/C:/Users/SVC-IG~1": Failure`) |
| `wflwvm01_short_name_home_prepare`, after the fix | PASS (127.59s) |
| `wflwvm01_write_half_scratch` (mirror stage, `stage_inner`) | PASS (124.77s) |
| `wflwvm01_apply_script_run` (long home) | PASS (425.83s) |
| xwin-built `deploy_host_conformance`, TEMP and TMP below `C:\Users\SVC-IG~1` | 47 passed, 0 failed |
| xwin-built `deploy_host_conformance`, default TEMP | 47 passed, 0 failed |
| `lnxvm_apply_script_run` on ign-lnx-836 | not run (instruction: leave that VM alone) |
| `wflwvm01_cli_apply_rollback` (optional) | not run (the `azure-win` profile reads the macOS Keychain, which this execution must not touch) |

## Regression gates (macOS, this tree)

- `cargo test --workspace --no-fail-fast`: 2507 passed, 0 failed, 69 ignored (23-22 ended at 2501; the difference is this plan's six new tests: five survey tests and the mkdir text test).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all --check`: clean. `cargo build -p ignition-cli --no-default-features`: ok.
- `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings`: clean.
- `git diff 14d0b01 -- crates/ignition-core/src/error.rs README.md`: empty (no slug, exit code or exit-table row moved).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The xwin conformance binary needs a static CRT**
- **Found during:** Task 2 (rehearsal)
- **Issue:** the first `cargo xwin` build (dynamic CRT) exits silently on ign-win-836: the VM has no `vcruntime140.dll`. `cargo xwin` also picked Homebrew's `rustc` (no Windows target) until rustup's toolchain was put first on PATH.
- **Fix:** built with `RUSTFLAGS="-C target-feature=+crt-static"`, `CARGO_TARGET_DIR=$HOME/.cache/ign-xwin-target-static`, `PATH=$HOME/.cargo/bin:$PATH`. Recorded in 23-LIVE-CAPTURES.md.
- **Files modified:** none (build invocation only)

**2. [Instruction] ign-lnx-836 was not touched**
- The plan lists `lnxvm_apply_script_run` as a Task 2 regression gate; the execution instruction said the Linux VM is not needed and must be left alone. The Linux path is covered offline by `prepare_lists_nothing_above_the_home_posix` and `mirror_stage_below_the_home_surveys_from_the_home`, and by the 23-22 gates that ran on the previous tree. Plan 23-24 or the verifier may want one Linux rerun.

### Authentication gates

None. ign-win-836 was deallocated by its 0300 UTC auto-shutdown (and had a new public IP, 52.154.75.231, the value in `~/.cache/ign-wflwvm01/host` and the pinned `known_hosts`); started with `az vm start` at 03:07Z, running at 03:09Z, left running.

## Deferred / noticed, not fixed

- `~/.config/ign-azure-win/env` is stale: its `IGN_WINVM_HOST` (40.114.71.134) and its key differ from the cached ones (`~/.cache/ign-wflwvm01/host`, `svc-ignition-deploy_ed25519`). The live gates were run with the cached values. The module doc of `live_transport_wflwvm01.rs` still tells the operator to source the config env file.
- `SftpTransport::shown` renders a path outside `data_dir` as ".", so a symlink refusal on the staging root reads "`.` is a symlink; symlinks are refused". `shown_paths_respect_the_data_dir_component_boundary` pins that. Pre-existing; a message-quality fix for a later plan.
- Two timing-sensitive tests failed repeatedly while the Mac was under heavy unrelated load (load average 30 to 100: a WebKit content process, BTLEServer and others), and passed once the load dropped, with no code change in between: `deploy_host_conformance::sh_commit_stops_when_its_time_budget_runs_out` (assertion `took < 2.9s` only; every behaviour assertion before it passed; took 3.7s to 19.7s) and `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` (polls the mock for 10s). Both passed on isolated builds of 14d0b01 and of HEAD, and both passed in the final `cargo test --workspace`. Not related to this change.
- A first workspace rerun after I removed `ignition_core` fingerprints from the shared cargo target dir (to rule out stale artifacts, per the repo's CLAUDE.md) failed six targets with "never executed" and "extern location does not exist": my own `rm` was still deleting while cargo built. Not a code failure; the clean rerun above is the evidence.

## Known Stubs

None.

## Threat Flags

None. `survey_root` removes the listing of the home's ancestors; every component below the home is still refused when it is a symlink, a special file, or a file where a directory is needed (T-23-88, `a_symlink_below_the_home_is_still_refused`). No `mkdir` tolerance was added (T-23-89). The mkdir text is specific (T-23-90). The SYSTEM fallback was not used (T-23-91). No secret was printed (T-23-92); the gates wrote only below the account home and removed their scratch (T-23-93).



## Self-Check: PASSED

Commits 1a81e0f, f0dced9, 3a47279 exist; the three source files, the G2 section (1) and `survey_root` in sftp.rs and apply_ops.rs are present; the named tests ran green.
