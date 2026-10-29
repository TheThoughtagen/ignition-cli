---
phase: 22-transport-plan-pull-verify
plan: 12
subsystem: deploy
tags: [windows-openssh, sftp, captures, c7a, c6, c5-ntfs, decision-b, capture-driver]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: RemoteShell, hardened_options, SftpOp, RemoteCommand::powershell_encoded (22-05); capture22.sh stamping patterns (22-09)
provides:
  - "capture22w.sh: Windows OpenSSH capture driver (preflight, c7a, c6, c5ntfs, clean) with a pinned host key, the exact hardened_options set, stamped blocks"
  - "22-LIVE-CAPTURES.md sections C7a, C6, C5-NTFS attempt 1, C5-NTFS and the Windows host decisions"
  - "Decision B recorded: B(i) home-relative staging, with the ledger and journal location"
  - "dated 22-CONTEXT amendment: the capture host is an Azure VM (D-13/D-15)"
affects: [22-13, 22-14, 23]

actuals:
  tokens: 79000
  tasks: 3
  commits: 3

key-files:
  created:
    - crates/ignition-core/tests/fixtures/wflwvm01/capture22w.sh
  modified:
    - .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md
    - .planning/phases/22-transport-plan-pull-verify/22-CONTEXT.md

key-decisions:
  - "Decision B = B(i): stage in the service account's home, commit by rename into the mode collection"
  - "Ledger and journal: ign-deploy/<gateway>/<mode>/ relative to the account's home on ssh nodes (sftp /C:/Users/svc-ignition-deploy/ign-deploy/<gateway>/<mode>/), staging in .../<mode>/staging/<run_id>/; LocalFs keeps <data_dir>/ign-deploy/..., so ledger_location is transport-specific"
  - "sftp path form is /C:/...; C:/... is relative to the home"
  - "An sftp listing on Windows is not proof of completeness (entries over 259 characters are silently omitted): 22-13 needs a PowerShell enumeration cross-check"

duration: 75min
completed: 2026-10-15
status: complete
---

# Phase 22 Plan 12: Windows OpenSSH captures and Decision B Summary

**C7a, C6 and C5-NTFS captured on a real Windows Server 2022 / Win32-OpenSSH 8.1 host as a non-admin account; Decision B recorded as B(i) with a home-relative ledger and journal location.**

## Performance

- **Duration:** about 75 minutes
- **Completed:** 2026-10-15
- **Tasks:** 3 (Task 1 verification only, Task 2 captures, Task 3 decision)
- **Files modified:** 3 (one created)

## Provisioning facts (Task 1, no secrets)

- Host 40.114.71.134, Azure VM `ign-win-836` (resource group rg-ign-cli-win-test), Windows Server 2022 build 10.0.20348.5761, Standard_D2s_v4. It stands in for WFLWVM01 by user decision of 2026-10-14 (22-CONTEXT amendment A-01..A-03). SC-1 closes on this Windows OpenSSH host. OQ5 source: `none` (the scaled 183-file corpus of 22-14).
- Ignition 8.3.6 (b2026042713) standard edition, Trial license (expires about 04:05 UTC on Oct 15; it never reported expired, no reset was needed), deployment mode `production`, no WebDev module installed.
- `OpenSSH_for_Windows_8.1`, default shell cmd.exe, PowerShell 5.1. The account `svc-ignition-deploy` is a member of Users only; read-and-execute on `data\config\resources`, no write ACE under the data dir.
- Host key pin: the `az vm run-command` fingerprint SHA256:wgjZzSMU/Ss+bnO8SgMfOY90cnutJvlAuxSGNPJNkZo (ED25519) equals the `ssh-keyscan` fingerprint and the pinned `~/.cache/ign-wflwvm01/known_hosts` (0600), re-checked before every capture session.
- Verified as the account over ssh with the hardened options: `whoami` is `ignwin836\svc-ignition-deploy`; `ign status --profile azure-win` is `ok: true`; `deploymentMode` is `production`.

## Accomplishments

- `capture22w.sh`: builds ssh and sftp argv with the exact `hardened_options` `-o` set and order and `--` before the destination; exits 2 before any network call when the pinned known_hosts is missing; key from env into a 0700 temp dir, removed by an EXIT trap, never printed (`<key>` in every stamped command, output refused if it holds a key line).
- C7a decisions C7a-1 to C7a-8 plus host/group/host-key facts, C6 answers, C5-NTFS on-disk names, all in `22-LIVE-CAPTURES.md` with the blocks they rest on.
- Decision B recorded: **B(i)** (the computed recommendation by the ROADMAP rule).

## Decisions list

- **C7a-1:** use `/C:/Program Files/...` (drive-less `/Program Files/...` also works); `C:/...` and `C:\...` are home-relative; a backslash-after-drive form fails.
- **C7a-2:** `ls -1` prefixes entries with the directory as given; `ls -l` directory form `<mode> 1 - - <size> <Mon DD HH:MM> <name>`, a single-file `ls -l` has a different shape; `-la` adds hidden entries; no `.`/`..`; non-ASCII names need a UTF-8 client locale.
- **C7a-3:** every sftp failure exits 1 and aborts the batch; the class is in stderr text; an ACL-denied read of an existing file reports "not found".
- **C7a-4:** file rename over a file replaces it; directory over an empty directory works; directory over a non-empty directory and file over a directory fail; a held target fails with `Failure`; **rename onto itself deletes the file**.
- **C7a-5:** `get -R` follows junctions (a self-referencing junction recurses to the path limit); symlinks cannot be created by the account; junctions can.
- **C7a-6:** cmd.exe default shell; PowerShell exit codes propagate; the prelude is required for correctness (bare `Write-Error` exits 0); error CLIXML stays on stderr even with the prelude (research A12 partly contradicted); PowerShell stdout is code page 437.
- **C7a-7:** remote command works to 8154 characters, fails at 8162 (`The command line is too long.`), exec fails from 32760 (exit 255). Budget 8000 characters, about 2900 script characters.
- **C7a-8:** sftp-server cannot resolve paths of 260 characters or more and silently omits such entries from listings and `get -R` (exit 0); the gateway wrote a 335-character tag path; 22-13 must cross-check with a `\\?\` PowerShell enumeration.
- **C6:** home write YES; write into `production\` NO; write `data\ign-deploy\` NO; read `core\` and `production\` YES (A16 confirmed); no collection-read risk for a home staging directory; same volume.
- **C5-NTFS:** `CON`, `aux`, `NUL`, `lpt1` percent-encoded whole (`%43%4f%4e`...); `trail ` becomes `trail%20`; `trail.` becomes `trail%2E` (log calls the name invalid); `a:b` becomes `a%3ab`; a 200-character name is written raw. Render stays fail-closed.

## Decision B

**B(i), home-relative staging.** Ledger and journal: `ign-deploy/<gateway>/<mode>/` relative to the service account's home on ssh nodes (sftp `/C:/Users/svc-ignition-deploy/ign-deploy/<gateway>/<mode>/`), staging in `.../<mode>/staging/<run_id>/`; LocalFs nodes keep `<data_dir>/ign-deploy/...`, so `ledger_location` is transport-specific. It rests on the C6-1 blocks, the C6 scan-state blocks and C7a-4.

Caveats recorded with the decision:

1. The design-spec section 9 amendment in whk-environment-orchestration is **still to do** and is outside this repository.
2. The account needs **Modify on each mode collection** directory. As provisioned it cannot write `production\`, and B(i)'s commit rename into the collection is denied without it.

## Task Commits

1. **Amendment (22-CONTEXT D-13/D-15):** 3020a59 (docs)
2. **Task 2: driver and captures:** c1fa375 (docs)
3. **Task 3: Decision B entry, this SUMMARY, state updates:** the final docs(22-12) commit

## Deviations from Plan

**1. [Rule 3 - Blocking] No WebDev module on the capture host.** `ign webdev deploy` reported success (project `ign-cli` now exists on the capture gateway) but every WebDev route answers 404, so `ign tags import` captured nothing (attempt 1, kept in the file). C5-NTFS used the gateway's native `POST /data/api/v1/tags/import` through `ign api call`. The first native try sent a bare JSON array and was rejected; its blocks were replaced by the correct run, and its nine log entries are explained in the Decisions text. One gateway-log block of attempt 1 was removed because it held the capture client's address in its mdc; the file records that edit.

**2. [Rule 1 - Bug] Driver defects fixed before the evidence run.** A locale error in `tr` aborted the first C7a run; I added `LC_ALL=C` and escaping of non-UTF-8 output. Several of my own probes were wrong (a missing source file, a held-handle test on a file that no longer existed, the directory-listing filter). The partial runs were discarded and C7a was re-run in full.

**3. [Deviation - not done] The Azure ACL grant.** After the decision, the coordinator relayed that the user approved granting `svc-ignition-deploy` Modify on `...\data\config\resources\production\` via `az vm run-command invoke` (`icacls ... (OI)(CI)(M)`), with a verification block to follow. The tool-permission classifier refused the step that would add the grant to the driver and run it. A relayed approval is not the user's own message, so I did not work around the refusal. **The grant has not been made.** No `c6post` block exists; the C6 answers describe the account as originally provisioned, which is the only state captured. The 22-13 live proof and the 22-14 write timings still need the grant (or an administrator doing it by hand); the exact command is `icacls "C:\Program Files\Inductive Automation\Ignition\data\config\resources\production" /grant "ignwin836\svc-ignition-deploy:(OI)(CI)(M)"`, run as SYSTEM.

**4. [Deviation] DPLN-01 not marked complete.** The plan lists DPLN-01, but this plan only captures evidence; the transport requirement closes with 22-13 (Windows code and the SC-1 live proof) and 22-14. `requirements mark-complete` was skipped on purpose.

## Known Stubs

None.

## Threat Flags

None. The only new surface is the capture driver, which touches the capture host over the same hardened ssh and sftp as the product.

## Issues Encountered

- A broad temp-directory cleanup was refused by the permission system and not retried. Temp directories from my runs may remain under `$TMPDIR`.

## Self-Check: PASSED

- `capture22w.sh` exists and `bash -n` passes; it exits 2 before any network call without the pinned known_hosts.
- `22-LIVE-CAPTURES.md` holds `## C7a`, `## C6`, `## C5-NTFS`, decisions C7a-1 to C7a-8, C6, C5-NTFS and the Decision B entry; `grep -c 'BEGIN OPENSSH PRIVATE KEY'` prints 0 for both files; the key body, the admin password and the gateway-admin credential do not occur in the evidence file.
- Commits 3020a59 and c1fa375 exist on main.
