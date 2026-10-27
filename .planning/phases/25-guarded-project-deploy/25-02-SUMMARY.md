---
phase: 25-guarded-project-deploy
plan: 02
subsystem: testing
tags: [ignition-8.3.6, project-inheritance, git-module, sftp, powershell, live-capture, bash]

requires:
  - phase: 25-guarded-project-deploy
    provides: "plan 25-01: capture25.sh, the rig, the C8 import and export captures"
  - phase: 23-guarded-apply-rollback
    provides: "the Linux and Windows capture hosts, the pinned known_hosts and the recorded admin route"
provides:
  - "capture25.sh subcommands c9, c10up, c10, c10down and ch (lnx|win)"
  - "25-LIVE-CAPTURES.md blocks C9, C10 and C-H before grant, with C10 and C-H readings and ### C-H grant text"
  - "measured 8.3.6 behaviour that 25-03 turns into contract items K7 (D-06 settle), K8 (D-11 git module) and K9 (DPRJ-04 probe)"
affects: [25-03, 25-07, 25-08, 25-15, 25-17]

actuals:
  tokens: 63000
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "state snapshots instead of log reading: digest, on-disk hash and .git kind per project, polled and recorded verbatim"
    - "a second disposable gateway (ign-c10-rig, port 18291) with its own token file, pinned by the driver before assert_local"

key-files:
  created: []
  modified:
    - crates/ignition-core/tests/fixtures/live-rig-deploy/capture25.sh
    - crates/ignition-core/tests/fixtures/live-rig-deploy/mint-token.sh
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md

key-decisions:
  - "C9 for K7: a parent overwrite leaves the child export untouched and the child reads the parent's new value within 5 s; a 30 s settle ceiling is ample; session impact on a child stays OPEN (S5 residual)"
  - "C10 for K8: an API overwrite import is not reverted by the git module, but it deletes the module-managed project's .git and the module never restores it; deploying over a module-managed project ends its management"
  - "C-H for K9: the .git probe must be stat (Linux) and Get-Item -Force or the enumeration (Windows); test -e, Exists and Test-Path answer absent when the project directory cannot be traversed"
  - "C-H grant: none needed as provisioned; where project directories are not world-readable the narrowest grant is r-x / (RX) on the project directory, no write right"

patterns-established:
  - "a probe's false-negative is captured on purpose: narrow the test directory, probe, grant, probe again"

requirements-completed: []  # evidence only: this plan records behaviour for DPRJ-02 and DPRJ-04, which complete when deploy projects ships

coverage:
  - id: D1
    description: "C9 records the child export, the script result and the gateway log across a parent overwrite and sizes the D-06 settle wait"
    requirement: DPRJ-02
    verification:
      - kind: other
        ref: "grep -c '^## C9' 25-LIVE-CAPTURES.md prints 1; 25 poll lines; settle_seconds=5 s; child_export_digest_changed=no; no CAP25-* project left on the rig"
        status: pass
    human_judgment: false
  - id: D2
    description: "C10 records, for the template and the production git.yaml, whether an API import is reverted, whether .git is recreated and whether an unmanaged project is touched, across a restart and a scan"
    requirement: DPRJ-04
    verification:
      - kind: other
        ref: "grep -c '^## C10' prints 1; ### C10 setup present; api-import-reverted, git-recreated and other-project-touched lines for both variants; no credential text; WHK-Global status hash unchanged"
        status: pass
    human_judgment: false
  - id: D3
    description: "C-H before grant has a probe table per host for the shapes none, directory, file, symlink (Windows: hidden directory, visible directory, hidden file, junction) with exit status and wall time, and ### C-H grant text"
    requirement: DPRJ-04
    verification:
      - kind: other
        ref: "grep -c '^## C-H before grant' prints 1; grep -c '^### C-H grant text' prints 1; both hosts clean afterwards (listing of data/projects names CAP25-GITPROBE 0 times, REST project list empty)"
        status: pass
    human_judgment: false
  - id: D4
    description: "The C9, C10 and C-H readings are the executor's interpretation of raw answers and feed contract decisions K7 to K9"
    requirement: DPRJ-02
    verification: []
    human_judgment: true
    rationale: "25-03 has a checkpoint where a human confirms the decisions; the C10 module-behaviour reading (which side removes .git) is explicitly not settled by the capture"

duration: 3h (about 14:25Z to 17:16Z for tasks 2 and 3 after the task 1 tracer, most of it gateway restarts and the 25 to 38 minute C10 runs)
completed: 2026-10-22
status: complete
---

# Phase 25 Plan 02: C9, C10 and C-H Captures Summary

**Live captures show a parent overwrite settles for a child within 5 s without changing its export, that an API overwrite import deletes a git-module project's `.git` for good while the module never reverts the content, and that the deploy account sees every `.git` shape on both hosts but only `stat` and `Get-Item -Force` tell "denied" from "absent".**

## Performance

- **Duration:** about 3 h for tasks 2 and 3 (task 1 was the earlier tracer); wall time is dominated by gateway restarts and two C10 reruns.
- **Tasks:** 3 (task 1 is the tracer, approved at its gate).
- **Files:** 3 modified (the driver, `mint-token.sh`, the captures file).

## Accomplishments

- **C9 (task 1, K7/D-06).** Parent `CAP25-P` overwritten while child `CAP25-C` (`parent: CAP25-P`) is polled every 5 s for 120 s: the child export has the same 18 members and digest at every poll, `find` stays enabled, and `cap25lib.answer()` in the child scope flips from 1 to 2 by the first 5 s poll. The log names the child once (`Restarting gateway scripts... project=CAP25-C, collection=CAP25-P`). A 30 s settle ceiling is ample. The S5 residual (session impact) is left OPEN.
- **C10 (task 2, K8/D-11).** A disposable 8.3.6 gateway (`ign-c10-rig`, loopback 18291) with the signed git module 2.3.4 installed through `ign rig up --with-module`, fed by a bare repository over HTTP and a `git.yaml` rendered by the WHK-Global CI renderer (read only). For both the template and the production variants: the API overwrite import took effect at once and was not reverted by a restart, the startup sync or a scan; the import deleted `CAP25-GIT/.git` and the module then logs `exists but has no .git directory, skipping sync` from then on; a `.git` removed by hand is not recreated; an unmanaged project is never touched. A control shows a foreign `.git` (one HEAD file) in an unmanaged project survives an overwrite import.
- **C-H (task 3, K9/DPRJ-04).** On both capture hosts, as `svc-ignition-deploy`, five Linux and eight Windows candidate probes against all shapes: as provisioned, no grant is needed and every probe is right. With the project directory narrowed, `test -e`, `[IO.Directory]::Exists` and `Test-Path` answer "absent" with exit 0; only `stat` (`Permission denied`) and `Get-Item -Force` (`UnauthorizedAccessException`) say denied. A4 resolved: `Test-Path -LiteralPath` sees a Hidden `.git` without any switch (and `Test-Path` has no `-Force` here). A junction is a plain directory to sftp. The narrowest grants (`setfacl -m u:...:r-x`, `icacls ... (OI)(CI)(RX)`) were applied to the test directory only and closed the gap; no write right is needed.

## Task Commits

1. **Task 1 (tracer): C9** - `cb7773b`, `3db9fb4` (docs)
2. **Task 2: C10** - `9675748` (docs)
3. **Task 3: C-H before grant** - `5512bf0` (docs)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `mint-token.sh` was hard-wired to the compose project `ign-deploy-rig`**
- **Found during:** Task 2
- **Issue:** the C10 gateway has its own compose project and needs its own token; the script could only mint on the shared rig.
- **Fix:** `PROJECT=${IGN_RIG_PROJECT:-ign-deploy-rig}`; the default is unchanged.
- **Files modified:** `mint-token.sh`
- **Commit:** `9675748`

**2. [Rule 1 - Bug] The first C10 design observed nothing**
- **Found during:** Task 2
- **Issue:** step (i) ran while the gateway was still restarting after the token mint (503), and step (ii) ran on a project whose `.git` the import had already removed, so it measured nothing; a later run stopped in the production variant on an import conflict (409), and a third was cut off when the Docker engine stopped.
- **Fix:** wait for CAP25-GIT before D0, retry imports on 503, start step (ii) from a fresh clone (`c10reclone`), skip an existing CAP25-OTHER, add a control for an unmanaged `.git`. Each failed run was discarded and the captures file restored; only the complete rerun is recorded (stated in the C10 reading).
- **Files modified:** `capture25.sh`, `25-LIVE-CAPTURES.md`
- **Commit:** `9675748`

**3. [Rule 2 - Missing critical functionality] Extra C-H probes and a false-negative test**
- **Found during:** Task 3
- **Issue:** the plan's probe list could not show which probe lies when the account cannot traverse the project directory, which decides K9.
- **Fix:** added `stat` and `test -L/-d/-f/-e` (Linux), `Get-Item -Force` and the enumeration (Windows), the Windows shapes visible directory and junction, and a restrict, probe, grant, probe step on the test project directory only.
- **Files modified:** `capture25.sh`, `25-LIVE-CAPTURES.md`
- **Commit:** `5512bf0`

### Out-of-plan operational notes

- An ad-hoc `az vm run-command` call of mine created a stray directory in the run-command plugin's own `Downloads` folder (a backslash-stripped path); it was removed as SYSTEM and the host checked clean (no `CAP25` entry under the plugin folder). It was not under the Ignition data directory.
- The Docker engine (OrbStack) stopped during the third C10 run; the shared rig container `ign-deploy-rig-gw-1` was started again and answers RUNNING with 0 projects. C10 did not use the shared rig.

## Authentication Gates

None. The orchestrator started both capture VMs and supplied the host environment file; the admin route for setup was the one recorded in 23-LIVE-CAPTURES.md (Linux `ignadmin` with sudo, Windows `az vm run-command` as SYSTEM).

## Known Stubs

None. `capture25.sh` and the captures file contain no placeholder data; no `pending` row was needed (both hosts answered).

## Threat Flags

None. `ign-c10-rig` is a loopback-only disposable gateway torn down at the end of the block; the capture hosts were changed only under `data/projects/CAP25-GITPROBE`, as the plan's threat model allows (T-25-04), and no ACL was applied outside it (T-25-05). WHK-Global status hash unchanged (T-25-06). No credential, token or key text in the captures file (T-25-07).

## Self-Check: PASSED

- `crates/ignition-core/tests/fixtures/live-rig-deploy/capture25.sh` exists and passes `bash -n`; `mint-token.sh` passes `bash -n`.
- `25-LIVE-CAPTURES.md` holds `## C9`, `## C10`, `### C10 setup`, `## C-H before grant`, `### C-H grant text` (grep counts 1 each).
- Commits `cb7773b`, `3db9fb4`, `9675748`, `5512bf0` exist (`git log`).
- `grep -ciE 'password|token: [A-Za-z0-9]|X-Ignition-API-Token: [A-Za-z0-9]'` prints 0; no private key text.
- The rig project list is empty (0 `CAP25-*`); `data/projects` on both capture hosts lists only `.resources`; the `ign-c10-rig` containers, volume and `$STATE/c10` are gone.
