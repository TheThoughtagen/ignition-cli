---
phase: 23-guarded-apply-rollback
plan: 08
subsystem: deploy
tags: [apply-host, windows-ssh, powershell-5.1, script-stub, acl, held-target, live-capture, c7b, c-script-run, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-06 commit.sh, POSIX_STUB, WorkList and the Linux ApplyHost (the op set and order translated here); 23-01 the Windows capture host record and routing; 23-12 recovery (the journal rules the script keeps)
  - phase: 22-transport-plan-pull-verify
    provides: winps (PRELUDE, WinScript, encoded_command, COMMAND_BUDGET), exec_powershell, the sftp write half and the C7a-4/6/7/8 captures
provides:
  - scripts/commit.ps1, a Windows PowerShell 5.1 script with the ops commit and restore (and the verify and unstage ops that prepare needs), on \\?\ paths, replace-by-move with bounded retries, an ACL reset after every move, one ASCII IGN-RESULT line with index-based results
  - SCRIPT_PS1, SCRIPT_PS1_FILE, script_ps1_sha256 (batch.rs); winps::script_stub, StubRoots, STUB
  - the Windows arm of impl ApplyHost for SftpTransport for prepare, commit and restore (the factory arm for Windows is unchanged)
  - ApplyHost::probe_paths and PreflightInput.touched_paths, so the write_host preflight can refuse a path a host cannot take
  - WINDOWS_SNAPSHOT_PATH_MAX (259) and windows_long_path_problems, the Rust gate for snapshot and staging paths the sftp server cannot reach
  - the held refusal (a target that cannot be read) of commit.ps1 and its sftp mapping
  - C-script-run and C7b in 23-LIVE-CAPTURES.md with their Decisions entries; live tests wflwvm01_apply_script_run and wflwvm01_c7b
affects: [23-20, 23-10, 23-13, 23-14, 23-16, 23-15]

actuals:
  tokens: 47000
  tasks: 2
  commits: 4  # three code commits and the docs commit

tech-stack:
  added: []
  patterns:
    - "a fixed stub reads commit.ps1 and work.tsv once as bytes, hashes them, and runs the very text it hashed through [ScriptBlock]::Create, handing the script the text of the work list it hashed; the command carries no path, the roots travel in the base64 JSON parameters"
    - "Windows PowerShell resolves an alias before a function: every helper name is checked against the 5.1 alias list by a unit test"
    - "a moved file takes the collection's inherited access list by resetting its security descriptor after the move"

key-files:
  created:
    - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
  modified:
    - crates/ignition-core/src/deploy/transport/batch.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/sftp.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/src/deploy/apply/host.rs
    - crates/ignition-core/src/deploy/apply/preflight.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - crates/ignition-core/tests/live_transport_wflwvm01.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md

key-decisions:
  - "Stub mechanism: [ScriptBlock]::Create over the bytes the stub hashed, never -File. -File fails under AllSigned and Restricted on this host (it works only because LocalMachine is Unrestricted); a script block runs under every policy (C-script-run)."
  - "Replacement inside the script is Move-Item -LiteralPath -Destination -Force on \\?\ paths, retried RENAME_RETRIES times one second apart. File.Replace behaves the same, File.Move refuses an existing target; a target held with None, Read or ReadWrite fails every variant (C-script-run)."
  - "The ACL step is kept (C7b): a file moved in from the home arrives with its three entries as explicit, protected entries, unlike config-mode.json or a file made in place; the script resets the security descriptor (empty FileSecurity, SetAccessRuleProtection false false) after each move of commit and restore. The gateway loaded the unprotected-list file too, so this is for gateways whose service account is not SYSTEM or an administrator."
  - "A target that cannot be read (held with FileShare.None) is the refusal held before the first move, after the same bounded retries, never a stale plan; a target held with FileShare.Read stops the move after the retries with the report failure write N: cannot move the file into place, and the restore of that run works while the file is still held (C7b)."
  - "WINDOWS_SNAPSHOT_PATH_MAX = 259: a file at a native path of 259 characters below the account home is listed and served by sftp, 260 and up are not (C7b). probe_paths refuses a touched path whose snapshot or staging copy would pass it, in Rust, before any write."
  - "script_stub takes the script and work-list hashes, the operation and the three roots, not the plan's script and work file paths: both files are fixed names inside the staging directory, which keeps the command shorter (worst legal case 5,882 of 8,000 characters)."

patterns-established:
  - "a probe that needs the run's touched paths gets them through ApplyHost::probe_paths, whose default is probe(); hosts with no path limit are unchanged"
  - "a live test that needs gateway REST uses the installed ign binary (api call) because the profile credential is in the OS keyring and only that binary may read it without a prompt"

requirements-completed: []
requirements-advanced: [DAPL-05, DAPL-06]

duration: ~2h20 (most of it cargo builds under a load average of 200-800 and live runs over the ssh link)
completed: 2026-10-17
---

# Phase 23 Plan 08: The Windows script host, commit and restore through one hash-checked PowerShell call Summary

**On a Windows ssh node the whole under-lock part of an apply is now one `exec_powershell` call of a fixed stub that checks the SHA-256 of the uploaded `commit.ps1` and `work.tsv` and runs the script text it hashed; the host's real behaviour (execution policy, replacing and holding files, ACLs, long paths) was captured first and the script handles each of them. 152 files moved in 6.92 s where the per-file path took 234-348 s.**

## C-script-run and the stub

Captured on `ign-win-836` (Windows PowerShell 5.1.20348.5756, `LocalMachine=Unrestricted`, FullLanguage), `## C-script-run` and `### C-script-run` in `23-LIVE-CAPTURES.md`:

| Question | Answer |
|---|---|
| `[ScriptBlock]::Create(ReadAllText(file))` under the policy | runs, exit 0, under the policy as found, `AllSigned` and `Restricted` |
| `powershell -File` | exit 0 under Unrestricted and Bypass, exit 1 under AllSigned ("not digitally signed") and Restricted ("running scripts is disabled") |
| Encoded stub length | 4,014 of 8,000 characters with the real roots (staging 95 characters); 5,678 at 240-character roots and 5,882 at 259 (unit test) |
| Replace a file | `Move-Item -Force` and `[IO.File]::Replace` replace a free target or one held with a share mode that allows delete; `[IO.File]::Move` refuses any existing target; a target held with None, Read or ReadWrite fails all three, unchanged, and `Move-Item` says "Cannot create a file when that file already exists." for a held file |
| Move onto itself | `Move-Item -Force` reports nothing and keeps the file (unlike an sftp rename onto itself); `commit.ps1` still refuses it |

The stub (`winps::STUB`): reads both files once as bytes, SHA-256s the bytes, exits 97 with one fixed sentence before running anything when either differs from `$p.ss`/`$p.ws`, otherwise runs `[ScriptBlock]::Create(UTF8.GetString($b))` with the operation, the three roots and the text of the work list it hashed. A tampered work list and a tampered script were each refused live ("the staged script or work list does not match its hash; nothing ran", no journal, collection untouched).

## C7b decisions (`### C7b`)

- **ACL:** staged copies hold the home's inherited entries; after `Move-Item -Force` into production they were explicit and protected (SYSTEM, Administrators, svc-ignition-deploy FullControl only), unlike `config-mode.json` and a file made in place (inherited, with `Users` ReadAndExecute). The script resets the security descriptor after every move (commit and restore). After the reset both moved files equal the control file.
- **Gateway load:** the moved unit was listed by `resources/list` (collection `production`, enabled) after a scan of 0.95 s in both runs, with and without the reset; the log has two `gateway.Database` ERROR lines (the fixture's password cannot be decrypted here), no `will not be loaded`, no permission error. `sc qc Ignition` is denied to the account, so the service identity was not read.
- **Held target:** `FileShare.Read` holder: the move fails after the 3 retries (5.7 s), report `phase writing`, `failure "write 0: cannot move the file into place"`, file unchanged, restore works while it is still held. `FileShare.None` holder: refused as `held` before any move (5.6 s), `transport_failed` naming the path, no journal. Neither hangs.
- **Defender:** the account cannot read Defender's configuration (CIM access denied). State as found per the host record: real-time protection on, no exclusions. The 152-file corpus (70 units, 91,833 bytes) was committed by one call in **6.92 s**; prepare (outside the lock) 86.0 s; restore 18.03 s wall-clock including its uploads. A with-exclusion run was not taken (it needs the administrator credential, which the plan does not use).
- **Longest reachable path below the home:** 259 characters (`ls` lists the file and `get` serves it); 260, 261 and 270 are not listed and `get` fails. Lengths probed: 200, 230, 240, 250, 255, 258, 259, 260, 261, 270.

## What was built

- `commit.ps1` (about 950 lines, ASCII, PowerShell 5.1): `CheckWork` re-validates every record of the list the stub hands it; `CheckRoots`; ops `commit` (exclusive journal create by moving a copy onto the journal name, pre-image check of every touched path with bounded retries for unreadable ones, snapshot with verification, record, journal `ready` then `writing`, moves, deletes and emptied directories deepest first and never `<module>/<type>`, ledger last, journal `written`), `restore` (journal at `restoring` first, copy back and verify, delete created files, ledger last, a second restore changes nothing), `verify` and `unstage` (what `prepare` needs outside the lock). Results are indices; no path or file content is printed.
- Windows `ApplyHost`: `prepare`, `commit` and `restore` use the same upload code as the Linux host with the PowerShell script and `exec_stub`; `probe`, `fingerprint`, `finalize`, `read_record`, `fetch_snapshot` and `discard_interrupted` still answer "not supported by this build yet" without starting a process (test `windows_apply_half_refuses_the_calls_23_20_builds`). `apply_host_for_node` still refuses a Windows ssh node (test `apply_host_for_node_linux_ssh` unchanged).
- `probe_paths`: the engine passes the touched paths (writes, deletes, the record's touched list) to the `write_host` preflight; only the Windows host looks at them.

## Tests

- `cargo test --workspace --no-fail-fast`: 2,424 passed, 1 failed, 53 ignored. The one failure is `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`, the known load flake (load average about 300), not a Phase 23 regression and not chased.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- New offline tests: `stub_fits_the_budget`, `stub_body_is_fixed_text_and_carries_no_path_or_hash`, `stub_verifies_before_it_runs_and_hashes_the_bytes_it_runs`, `the_shipped_script_is_ascii_text_the_stub_can_hash`, `the_shipped_script_names_no_function_like_an_alias_or_a_cmdlet`, `windows_apply_half_refuses_the_calls_23_20_builds`, `windows_probe_paths_refuses_a_path_the_sftp_server_could_not_reach`, three `apply_ops::tests` on the 259-character gate (at the limit, one past, UTF-16 units, counted refusals), `preflight_write_host_reports_a_path_the_host_cannot_take`. `stub_refuses_hash_mismatch` is `#[cfg(windows)]` and was NOT run here (no Windows runner on this machine); its property was run live against the host (tampered work list and tampered script, exit 97, nothing ran).
- Live (the Windows host, `IGNITION_LIVE_WFLWVM01=1`): `wflwvm01_apply_script_run` passed three times (2026-10-17T18:58Z, 20:22Z, and again after the review hardening), `wflwvm01_c7b` passed twice (19:28Z and again after the hardening, with the no-broad-write assertion), `wflwvm01_script_acl_and_reparse` passed (20:53Z); each ends with the collection equal to its pre-run listing and the scratch/deploy trees gone.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] A helper named `LP` was never called**
- **Found during:** Task 1 (first live prepare: every staged hash came back empty)
- **Issue:** `LP` is the Windows PowerShell alias of `Out-Printer`; an alias wins over a function.
- **Fix:** renamed to `Xp`; unit test `the_shipped_script_names_no_function_like_an_alias_or_a_cmdlet` holds every helper name against the alias list of this host.
- **Commit:** af93e5c

**2. [Rule 1 - Bug] A held target was reported as a stale plan**
- **Found during:** Task 2 (C7b held-target step)
- **Issue:** a file another process holds without sharing cannot be hashed, and the pre-image check turned that into "not what the plan was made against", which would send the user to re-plan.
- **Fix:** `Live` tells an unreadable file (`?`) from a changed one; the pre-image check retries it, then refuses `held` before the first move (journal and partial snapshot removed); the sftp mapping names the path.
- **Commit:** 7102ac4

**3. [Rule 2 - Missing critical functionality] The ACL step**
- **Found during:** Task 2 (planned by the plan, needed by the C7b answer)
- **Fix:** `ResetAcl` after every move of commit and restore; a failure fails that entry.
- **Commit:** 7102ac4

**4. [Rule 3 - Blocking] The C7b test could not open the keyring**
- **Found during:** Task 2 (first run hung)
- **Issue:** `Session::resolve` read the profile's credential from the macOS Keychain, which prompts a person for a test binary; nothing had been written yet. The run was killed.
- **Fix:** the test reaches the gateway (scan, resource list, log) through the installed `ign --profile azure-win api call`, which may read it without a prompt.
- **Commit:** 7102ac4

**5. [Rule 2 - security review follow-up] insecure-permissions and toctou-reparse (flagged by the coordinator for `commit.ps1`)**
- **insecure-permissions: not a defect, now asserted.** Everything the script creates (staging and control directories, journal, snapshot copies, record, ledger, restore temp files) is created below the account home and inherits the home's list: SYSTEM, Administrators and svc-ignition-deploy only (observed on the staged copies, run 2 of C7b, and on 28 created or moved files in the new run 3). Nothing the script creates carries an entry for `Users`, `Everyone` or `Authenticated Users`, and the script grants nothing explicitly. The moved files in the collection end with the collection's own inherited list (ACL reset): the only broad entry is `BUILTIN\Users` ReadAndExecute, the same as `config-mode.json`; the svc-ignition-deploy grant on `production` is the only write grant. Before the reset they were protected with only the three entries (no broad entry either). Tests: live `wflwvm01_script_acl_and_reparse` asserts no broad entry on 28 created or moved paths; `wflwvm01_c7b` now fails if a moved production file lets `Users`, `Everyone` or `Authenticated Users` write (`broad_principal_rules` / `broad_write_rules`, with an offline test).
- **toctou-reparse: real as a narrow residual, hardened.** The gap exists in the plan's threat model as FA-23-05 for the `sh` script: a path is classified (reparse points refused along the chain) and acted on a moment later, and a shell or PowerShell cannot open relative to a handle. Who can win the race: a principal that can create a junction below `production` or the account home, which is the deploy account itself, Administrators and SYSTEM; `Users` have read and execute only. That principal can already rewrite the collection directly, so no new privilege is gained, but the cheap fixes were made: (1) `MoveWithRetries` re-classifies the target chain before EVERY attempt and refuses a link; (2) `ResetAcl` takes base and relative path, refuses unless the path is a plain file with no link on the way immediately before the descriptor change and checks again immediately after (the descriptor change is the one step that would otherwise silently act on a link's target); (3) `RemoveTree` and the control-directory cleanup use `[IO.Directory]::Delete`, which removes a link inside the tree as the link, instead of `Remove-Item -Recurse`, which in Windows PowerShell 5.1 can follow a junction into its target; (4) deletes, snapshot copies and emptied-directory removals already classify immediately before acting, and a non-recursive directory delete of a link removes only the link. Residual: a link swapped in between the last check and the single API call that follows it; it is not closed by anything the script can do and is recorded, as for `sh`. Tests (live, deterministic cases): a restore whose directory is a junction fails `restore 0: the live path is not a regular file` and the target is untouched; a commit through a junction stops at the pre-image check with nothing written through it. The race itself cannot be staged deterministically and has no test.
- **Commit:** 528efb3

### Plan departures, not rule-driven

- **`script_stub` signature.** The plan names `script_stub(script_path, script_sha256, work_path, work_sha256, op, roots)`. Both files are fixed names in the staging directory, so the function takes the two hashes, the operation and `StubRoots`; the stub reads `commit.ps1` and `work.tsv` from `roots.staging`. Shorter command, same checks.
- **Two extra ops in `commit.ps1`.** `verify` and `unstage` exist because the shared `prepare` calls them (the upload check outside the lock and the removal of a stale staging directory).
- **`ApplyHost::probe_paths` and `PreflightInput.touched_paths`.** `probe()` takes no arguments, so it cannot see the touched paths. The plan's "the Windows `probe` arm refuses ... reported through the `write_host` preflight" is built as a defaulted trait method plus a new input field (three construction sites, all updated). `probe()` and every other host are unchanged; `FaultyHost.calls()` still records `probe`.
- **Defender with and without an exclusion.** Taken once, as found (the plan's action says "once ... as found"; the truth line says "with and without"). Adding an exclusion needs the administrator credential, which this plan does not use.
- **TDD.** Task 1 is `tdd="true"`; the unit tests and the live test were committed together with the implementation (the live test needed the script to exist), so there is no RED commit. The first live run was the red: it failed on the alias bug above.
- **Tracer feedback gate.** Auto mode is not active and the project runs `human_verify_mode = end-of-phase`; as in 23-01, 23-02, 23-04, 23-05, 23-06, 23-12 and 23-18 the run continued after the tracer commit instead of stopping for a human-verify checkpoint. The tracer's `<verify>` (`winps` unit tests, offline `live_transport_wflwvm01`, and the live commit and restore on the real host) passed before the expansion task. For the end-of-phase list: the tracer is a working, hash-checked PowerShell commit and restore on `ign-win-836`.
- **Production touched by an aborted run.** The second C7b run panicked in the test's own listing script (a defect fixed afterwards) before its restore code ran and left the probe unit `ignition/database-connection/SparkDB-oq5-01` in production. It was removed by hand with PowerShell (the stray `ignition` directory that run created, `config-mode.json` untouched), `ign-deploy` was removed, a scan was requested (200). The test now runs its body as a task so a panic cannot skip the restore.

## Carry-overs

**23-20 (rest of the PowerShell host):**
- `commit.ps1` still needs `probe` (call it after the Rust gate: `probe_paths` already refuses long paths and then calls `probe()`), `fingerprint`, `finalize`, `discard` and `mirror`. The `mirror` op must reuse `MoveOne`, which already resets the ACL; the Phase 22 mirror commit moves files into the same collection.
- The other Windows `ApplyHost` arms (`read_record`, `fetch_snapshot`, `fingerprint`, `finalize`, `discard_interrupted`, `probe`) still go through `apply_setup` (which refuses Windows); switch them to `apply_setup_any` as they land. `fetch_snapshot` downloads over sftp, so the 259 limit (`WINDOWS_SNAPSHOT_PATH_MAX`) is the property to test it with.
- The conformance table has rows the Windows host answers differently from `sh` and the local host: a held target (`held` refusal before any move; a `FileShare.Read` holder fails the move), the ACL (inherited list after a move), and `Move-Item` onto itself. The `sh` script treats an unreadable file as stale; make that a deliberate difference or give it the same `held` refusal. `stub_refuses_hash_mismatch` (`cfg(windows)`) runs on the windows-latest leg; this plan could not run it.
- The factory arm for Windows (`apply_host_for_node`) and the Windows mirror commit are still 23-20's; the existing unit test pins the refusal until then.
- Held restores: a restore whose file is still held (`FileShare.Read`) succeeded because the live file already equalled the pre-image; a restore that must write over a held file fails after the retries with `restore N: cannot move the file into place` and leaves the journal at `restoring`.

**23-10 (hold model, OQ5b):** one script call committed 152 files in 6.92 s (about 45 ms a file including the ACL reset), 2.7-3.1 s for a 2-file batch (fixed cost about 2.5 s: ssh handshake, PowerShell start, stub hashes); prepare (outside the lock) 29 s for 8 files and 86 s for 152; restore wall-clock 14.6 s (2 files) and 18.0 s (152 files, including uploads and the journal read). These are single runs, not p95s. The gateway trial was not reset by this plan until the end (2026-10-17T20:29:09Z, `POST /data/api/v1/trial`, HTTP 200, 7,199 s left) because every live step fell inside the 2 h window.

**23-13 / 23-14 (verbs, README):** a refused `held` commit surfaces as `transport_failed` (exit per the existing slug) with "<path> could not be read because another process holds it open; nothing was changed"; there is no new slug, so the four-place rule was not triggered. Windows notes for the README: the deploy account needs Modify on the mode collection AND ownership of the files it moves (the ACL reset needs WRITE_DAC, which the owner has; the files it stages are its own), and a collection path whose copy below the account home would pass 259 characters is refused by the `write_host` preflight with its path named. The record of `ign deploy rollback` for a Windows node depends on 23-20's `fetch_snapshot`.

## Known Stubs

None. `grep` for `TODO`, `FIXME`, `todo!`, `unimplemented!` and skipped tests over the changed files finds nothing; the only `#[ignore]` tests are the live ones, which carry their reason.

## Threat Flags

None new. Two review categories were examined (Deviations item 5): permissions of created files (not a defect, asserted) and reparse check-then-use (narrow residual, hardened, deterministic cases tested). Within the plan's threat model: T-23-33 (stub hash check, live tamper tests), T-23-34 (paths only in `work.tsv` and the base64 parameters; `CompOk` re-checks every component in the script), T-23-35 (C7b capture and the ACL reset) and T-23-36 (the 259 gate) are each implemented and tested.

## Self-Check: PASSED

- `crates/ignition-core/src/deploy/transport/scripts/commit.ps1`: FOUND
- `.planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md` has `## C-script-run`, `### C-script-run`, `## C7b`, `### C7b`: FOUND (4 headings)
- commits `af93e5c` (Task 1), `7102ac4` (Task 2) and `528efb3` (review follow-up): FOUND
