---
phase: 23-guarded-apply-rollback
plan: 01
subsystem: testing
tags: [live-captures, ignition-8.3.6, scan-lock, gwbk, restart-tasks, bash]

requires:
  - phase: 22-transport-plan-pull-verify
    provides: capture22.sh rig driver (compose project ign-deploy-rig, bind-mounted resources, tokens), assert_local, cap and shcap helpers
provides:
  - capture23.sh, a localhost-only rig capture driver (csysname, crestart, cgwbk, c11, c2)
  - 23-LIVE-CAPTURES.md with the Windows host record (pending), the Captures Owed table and the C-sysname, C-restart, C2, C11, C-gwbk captures with Decisions
  - the OPEN STATE.md blocker for the pending Windows host, stop point 23-08
affects: [23-02, 23-05, 23-07, 23-08, 23-09, 23-10, 23-17]

actuals:
  tokens: 34900
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "capture driver headings are exactly the capture id, so later plans find a capture by name"
    - "a request line names the token variable, never the token or the header carrying it"

key-files:
  created:
    - crates/ignition-core/tests/fixtures/live-rig-deploy/capture23.sh
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
  modified:
    - .planning/STATE.md

key-decisions:
  - "C-sysname: REST gateway-info name and on-disk systemName agree on a fresh rig (ign-deploy-rig); divergence was not probed"
  - "C-restart: restart-tasks/pending answers {\"pending\":[]} or {\"pending\":[\"Module Disabled: <id>\"]}; no token and a token with no security level both get 401 HTML"
  - "C2: the scan-lock queues another actor's API write until the hold lapses (59.94 s); a file written with no lock, under a held lock or across a lapse is not loaded without a scan within 120 s"
  - "C11: a tag provider and its tag definitions load in one scan"
  - "C-gwbk: a gwbk is a ZIP sent chunked with no Content-Length; a half copy has no central directory, so opening it as a ZIP is the completeness check"

requirements-completed: []

coverage:
  - id: D1
    description: "Windows capture host recorded as pending with its routing row and the STATE.md blocker"
    requirement: "DAPL-04"
    verification:
      - kind: other
        ref: "grep '^## Windows capture host (23-01)' 23-LIVE-CAPTURES.md; grep 'OPEN 23-01' .planning/STATE.md"
        status: pass
    human_judgment: false
  - id: D2
    description: "capture23.sh refuses a non-local gateway before any read or request"
    verification:
      - kind: other
        ref: "BASE='http://evil.example#@localhost' bash capture23.sh csysname; exit 2"
        status: pass
    human_judgment: false
  - id: D3
    description: "C-sysname, C-restart, C2 (a, c, d, e), C11 and C-gwbk recorded verbatim from a live 8.3.6 rig, each with a Decisions entry"
    requirement: "DAPL-06"
    verification:
      - kind: other
        ref: "grep -c '^## C2$\\|^## C11\\|^## C-gwbk\\|^### C2$\\|^### C11\\|^### C-gwbk' 23-LIVE-CAPTURES.md prints 13"
        status: pass
    human_judgment: false
  - id: D4
    description: "C2b: a Designer or gateway-UI save under a held lock"
    requirement: "DAPL-06"
    verification: []
    human_judgment: true
    rationale: "A gateway UI or Designer save is an interactive session the driver cannot perform; the steps are in 23-LIVE-CAPTURES.md ## C2b (manual)"

duration: 27min
completed: 2026-10-16
status: complete
---

# Phase 23 Plan 01: Capture Phase 23 Rig Facts Summary

**Localhost-only capture driver and 23-LIVE-CAPTURES.md: the Windows host recorded `pending` (stop point 23-08), and the scan-lock, provider-ordering, `.gwbk`, restart-pending and system-name facts taken from a fresh 8.3.6 rig before any engine code depends on them.**

## Performance

- **Duration:** 27 min
- **Started:** 2026-10-16T14:04Z
- **Completed:** 2026-10-16T14:31Z
- **Tasks:** 3 of 3 (Task 1 is a human-action checkpoint, answered before this run)
- **Files modified:** 3 (two created, STATE.md edited)

## Task 1 resume signal and routing row

The user's answer, relayed by the orchestrator: **"pending"** (the host will come later). The orchestrator had confirmed on 2026-10-16 that `az vm show -d -g rg-ign-cli-win-test -n ign-win-836` still returns `ResourceGroupNotFound` in subscription "Microsoft Azure Sponsorship", that no `IGN_WINVM_*` or `IGN_WFLWVM01_*` variable is set, and that `~/.cache/ign-wflwvm01/known_hosts` exists; this run re-checked the env and the known_hosts file read-only. No ssh was attempted, no key text recorded.

The matching row of `## Windows host outcome routing`, as quoted back in `23-LIVE-CAPTURES.md`:

> | `pending` | 23-01 Tasks 2-3, 23-02..23-07, 23-18, 23-19, 23-17, 23-09, 23-11, 23-12 | the 23-08 Task 1 precondition (an unmet precondition halts with "Blocked by") | paused, not complete. When the host exists, the user exports the env and the executor re-runs the Task 1 verification (`az vm show`, `ssh ... pwd`), appends a dated `ready` line to `## Windows capture host (23-01)`, and execution resumes at 23-08. A host recorded `ready` that stops answering by 23-08 is treated as `pending` | one OPEN line: host pending, stop point 23-08 |

Plans that will not run until a host is recorded `ready`: 23-08, 23-20, 23-10, 23-13, 23-21, 23-14, 23-15, 23-16. The OPEN line is in `.planning/STATE.md` `### Blockers`.

## Decisions (one line each; full text in 23-LIVE-CAPTURES.md `## Decisions`)

- **C-sysname:** REST `gateway-info.name` and the on-disk `systemName` (core collection) are both `ign-deploy-rig` on a fresh rig, so a wrong-target binding can compare them. The capture does not show them diverging.
- **C-restart:** `GET restart-tasks/pending` answers `200 {"pending":[]}` with nothing pending and `200 {"pending":["Module Disabled: com.inductiveautomation.sfc"]}` after a module disable (the shape is the OpenAPI shape, FA-23-01 closed); the entry persists after a reversal. No token and a token with no security level both get `401` with a Jetty HTML body, never reaching the route.
- **C2:** (a) the lock queues token B's API write until the hold lapses (59.94 s, then 200); (b) not run, manual and pending; (c) no lock, (d) held lock, (e) lapsed lock: a file written into the mode collection is not loaded and `lastScanTimestamp` does not move for 120 s, so loading needs the engine's own scan.
- **C11:** a tag provider and its tag definitions load in one scan.
- **C-gwbk:** a `.gwbk` is a ZIP (3,674,513 bytes, 1597 entries, 744 ms), sent chunked with no Content-Length; a half-size copy has no end-of-central-directory record, so opening it as a ZIP is D-13's completeness check.

## Accomplishments

- `capture23.sh` reuses the Phase 22 localhost guard (the `http://evil.example#@localhost` case exits 2 before any request) and delegates bring-up to `capture22.sh`; `setup23` adds a second token and a token with no security levels.
- `23-LIVE-CAPTURES.md` opens with the Windows host record and the Captures Owed table (eight ids), so capture discipline is visible before dependent code.
- Five rig captures are recorded verbatim, each block preceded by its command and a UTC time, with Decisions that state answers.

## Task Commits

1. **Task 1 (human-action: Windows capture host):** no commit; the answer is recorded in Task 2's files.
2. **Task 2 (tracer: capture23.sh, C-sysname, C-restart):** `eb5a871`
3. **Task 3 (C2, C11, C-gwbk):** `fa0fba4`

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP) follows this file.

## Tracer feedback gate

`<execution_flow>` asks an interactive run to stop with a `checkpoint:human-verify` after the tracer commit. This project runs `workflow.human_verify_mode = end-of-phase` (human-verify checkpoints do not halt mid-flight) and the orchestrator directed Tasks 2 and 3 to continue. The tracer's own `<verify>` (syntax check, the `evil.example#@localhost` refusal at exit 2, the heading grep, the secret-header grep) passed before the commit and again after Task 3. Nothing human-verifiable was skipped silently: the C2b manual check is the open item below.

## Files Created/Modified

- `crates/ignition-core/tests/fixtures/live-rig-deploy/capture23.sh` - capture driver (up23, setup23, tokens23, header23, csysname, crestart, cgwbk, c11, c2, down23)
- `.planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md` - the captures and Decisions
- `.planning/STATE.md` - OPEN blocker for the pending Windows host (also carries the orchestrator's earlier status edits)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] The request line in captured blocks must not name the token header**
- **Found during:** Task 2
- **Issue:** capture22.sh's `cap` prints `-H 'X-Ignition-API-Token: $IGN_TOKEN_CAP_A'` in each command line; the plan's acceptance grep (`X-Ignition-API-Token:` prints 0) would fail on every block.
- **Fix:** `cap` prints `[token $IGN_TOKEN_CAP_A]` (or `[no token]`) instead, and takes the token variable from `CAP_TOK_VAR` so token B and the no-permission token are named the same way. The `.gwbk` header probe passes the header through a 0600 temp file (`curl -H @file`) so no token text reaches a command line. The grep prints 0.
- **Files modified:** `capture23.sh`
- **Verification:** `grep -c 'IGN_.*KEY=\|Authorization:\|X-Ignition-API-Token:' 23-LIVE-CAPTURES.md` prints 0
- **Committed in:** eb5a871

**2. [Rule 3 - Blocking] The rig had only one token and no token without permission**
- **Found during:** Task 2
- **Issue:** `setup22` mints cap-a only; C2(a) needs a second actor (token B) and C-restart wants a token without the permission, and the rig offered neither.
- **Fix:** `tokens23` runs `mint-token.sh cap-a cap-b` and writes an API token `cap-ro` with an empty `securityLevels` list into the bind-mounted `external` collection; keys stay in the 0600 `tokens.env`. `shcap`'s leak check covers all three tokens.
- **Files modified:** `capture23.sh`
- **Committed in:** eb5a871

**3. [Rule 1 - Bug] The poll printed `scanActive=?` for a false value**
- **Found during:** Task 3 (first full c2 run)
- **Issue:** jq's `//` treats `false` as absent, so every poll line showed `?` instead of `false`.
- **Fix:** `.scanActive | tostring`; the C2 section was removed from the doc and the whole `c2` re-run, so the doc matches the committed script (the earlier run's numbers are not kept).
- **Files modified:** `capture23.sh`
- **Committed in:** fa0fba4

---

**Total deviations:** 3 auto-fixed (1 missing critical, 1 blocking, 1 bug). **Impact:** no scope change; all three were needed for the captures to be recorded as the plan's acceptance requires.

**Other plan departures, not rule-driven:**
- The capture headings are exactly the capture id (`## C2`, `## C11`, `## C-gwbk`, `## C-sysname`, `## C-restart`) and the sub-section titles carry the descriptions, because the plan's verify greps match `^## C2$`. `## Decisions` was written in Task 2 and moved to the end of the file in Task 3 so it stays last.
- `requirements-completed` is empty: this plan records facts, and DAPL-04, DAPL-05, DAPL-06 and DAPL-10 are satisfied by code that later plans write, so `requirements mark-complete` was not run.

## Auth Gates

None. Task 1 was a human-action checkpoint answered before this run (`pending`).

## Known Stubs

None in code. One recorded unrun check: `## C2b (manual)` holds the steps for the gateway-UI and Designer save under a held lock and reads `Result: PENDING`. It is a `<human-check>` of Task 3 (deferred to end-of-phase under `human_verify_mode`). The broken-windows ledger append (`windows append --kind unrun-verify`) failed with `Ledger entry 3 has invalid kind: "process"`, a pre-existing bad entry in `.planning/WINDOWS.md` outside this plan's scope; the append is best-effort, so the item is tracked here and in `23-LIVE-CAPTURES.md` only.

## Threat Flags

None. The capture driver adds no network endpoint to the product; it talks to a localhost rig only. T-23-01 to T-23-04 mitigations hold: `assert_local` refuses `http://evil.example#@localhost` (exit 2), no token or token header is in the doc (grep 0), the `.gwbk` was downloaded into a temp directory outside the repo and deleted (`git status` shows no `.gwbk`), and no key text is recorded.

## Issues Encountered

- The trial licence of the disposable rig expires 2026-10-16 16:09 UTC (about two hours after the rig started). Re-capturing after that needs `rig` teardown and `up23`/`setup23` again (`down23` prints the teardown command).
- The module toggle route `PUT /data/api/v1/modules/toggle-state` answered `500` with an empty body for both the disable and the enable although the disable took effect; recorded under C-restart. The rig now shows SFC disabled pending a restart.

## Next Phase Readiness

- 23-02 (contract checkpoint) can choose the wrong-target rule from C-sysname, and takes the `.gwbk` completeness rule from C-gwbk.
- 23-05 and 23-17 can write fault-injection assertions against the C2 and C11 answers.
- 23-09 can add `DeployApi::restart_pending` and its `DEPLOY_ENDPOINTS` row from C-restart.
- Execution continues in wave order until the 23-08 Task 1 precondition, where it stops until the Windows host is recorded `ready`.

## Self-Check: PASSED

- `capture23.sh` exists and is executable; `23-LIVE-CAPTURES.md` exists with `## Windows capture host (23-01)`, `## Captures Owed`, `## C-sysname`, `## C-restart`, `## C2`, `## C2b (manual)`, `## C11`, `## C-gwbk`, `## Decisions`.
- Commits `eb5a871` and `fa0fba4` exist in `git log`.
