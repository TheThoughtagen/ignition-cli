---
phase: 25-guarded-project-deploy
plan: 03
subsystem: planning
tags: [contract-decisions, live-capture, host-grant, git-module, project-deploy]

requires:
  - phase: 25-guarded-project-deploy
    provides: "plans 25-01 and 25-02: captures C8a-C8e, C9, C10 and C-H before grant"
  - phase: 23-guarded-apply-rollback
    provides: "the Linux and Windows capture hosts and the recorded admin route"
provides:
  - "25-LIVE-CAPTURES.md sections ## Decisions, ## Contract proposal (25-03), ## Host grant and prod git.yaml (25-03), ## Contract decisions (25-03)"
  - "COVERAGE.md: decided gateway project route matrix (6 INTEGRATE, 9 OPT-OUT, each reasoned)"
  - "the user's recorded answer for K1-K15 and V1-V11, which plans 25-04..25-19 assert as a precondition"
affects: [25-04, 25-05, 25-06, 25-07, 25-08, 25-10, 25-11, 25-15, 25-17, 25-19]

actuals:
  tokens: 10800
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "capture-driven contract proposal: one table, one user answer, recorded verbatim with a UTC date before any code"
    - "grant verified by a narrow-then-grant-then-restore cycle on a single test project directory, never on data/projects"

key-files:
  created:
    - .planning/phases/25-guarded-project-deploy/COVERAGE.md
  modified:
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md

key-decisions:
  - "User approved every Recommended option (\"Approve all (Recommended)\", 2026-10-22): K1-K9, K11-K15, V1-V7, V9, V10 as recommended; K10, V8, V11 recorded as decided (D-13, D-14, D-15)"
  - "Departures from the plan text that the answer approves: K2 directory-inside-a-resource refusal, K3 deploy denied at the repo root only, K4 files-array sort and project.json defaults, K6 projects/list as the parent source, K7 CHILDREN_SETTLE_S = 30, K9 stat / Get-Item -Force instead of a sftp listing"
  - "K8 a (module stays installed, per-project .git removal): condition met, the user confirmed the production git.yaml keys equal the C10 production variant"
  - "Host grant applied only to the CAP25-GITPROBE test directory on ign-lnx-836 and ign-win-836 and restored; no persistent ACL change; as provisioned no grant is needed"

patterns-established:
  - "an open owner question is carried verbatim into the SUMMARY and the cutover runbook rather than decided by the executor"

requirements-completed: []  # decisions only: DPRJ-01..05 complete when deploy projects ships

coverage:
  - id: D1
    description: "Every capture C8a-C-H has a decision subsection with a quote and the option it supports; the proposal holds 26 rows (K1-K15, V1-V11) with Recommended and Evidence cells"
    requirement: DPRJ-01
    verification:
      - kind: other
        ref: "grep -c '^### C8[a-e]$|^### C9$|^### C10$|^### C-H$' prints 8; 26 proposal rows; ### Flagged assumptions names DPRJ-02, DPRJ-04, DPRJ-05"
        status: pass
    human_judgment: false
  - id: D2
    description: "The host grant is applied (user authorized) on the test directory of both capture VMs and the after-grant probes answer found where they answered denied before"
    requirement: DPRJ-04
    verification:
      - kind: other
        ref: "ign-lnx-836: stat denied (exit 1) before, 'directory drwxr-xr-x root:root' after; ign-win-836: Get-Item -Force UnauthorizedAccessException before, Attributes=18 (Hidden, Directory) after; data/projects names CAP25-GITPROBE 0 times on both afterwards"
        status: pass
    human_judgment: false
  - id: D3
    description: "The user's contract answer is recorded verbatim with the UTC date and the resolved option for all 26 items"
    requirement: DPRJ-05
    verification:
      - kind: other
        ref: "grep -c '^## Contract decisions (25-03)' prints 1; 26 table rows after that heading; K10, V8, V11 marked decided"
        status: pass
    human_judgment: false
  - id: D4
    description: "The recommendations that narrow or extend locked decisions (K2, K3, K6 against D-09 and D-05) and the K8 condition are the executor's reading of the captures; the user's answer approves them"
    requirement: DPRJ-02
    verification: []
    human_judgment: true
    rationale: "approved by the user on 2026-10-22; the long-term branch lifecycle of module-managed projects is explicitly left open"

duration: "about 2h for tasks 2 and 3 (11:23 to 13:15 local after the task 1 tracer; the two host capture runs took about 10 minutes)"
completed: 2026-10-22
status: complete
---

# Phase 25 Plan 03: Contract Decisions, Host Grant and Production git.yaml Summary

**The user approved all 23 open contract items (K1-K15, V1-V11) at their Recommended option with K10, V8 and V11 recorded as decided; the list grant was applied and verified on a test project directory of both capture VMs, and the production git.yaml keys were confirmed equal to the C10 production variant.**

## Performance

- **Duration:** this continuation (tasks 2 and 3) about 2 h of wall time including the two live host runs (Linux about 1.5 min, Windows about 8 min).
- **Tasks:** 3 (task 1 the tracer, committed earlier as `76eae72`).
- **Files:** 2 (`25-LIVE-CAPTURES.md` modified, `COVERAGE.md` created).

## Accomplishments

- **Task 1 (tracer).** `## Decisions` (8 subsections), `## Contract proposal (25-03)` (26 rows plus `### Flagged assumptions`) and the decided `COVERAGE.md`.
- **Task 2.** `## Host grant and prod git.yaml (25-03)`. On the user's "Authorize grant", the `setfacl` and `icacls` lines of `### C-H grant text` were applied to `data/projects/CAP25-GITPROBE` only (admin route: ssh `ignadmin` with sudo on Linux, `az vm run-command` as SYSTEM on Windows), probed as `svc-ignition-deploy`, and restored; both VMs were already running and were left running. Before/after: Linux `stat` exit 1 `Permission denied` then `directory drwxr-xr-x root:root`; Windows `Get-Item -Force` `UnauthorizedAccessException` then `Attributes=18 (Hidden, Directory)`; `test -e`, `Directory.Exists` and the sftp listing answered a false "absent" without the grant. The production keys were confirmed as the C10 production variant, so K8's condition is met.
- **Task 3.** `## Contract decisions (25-03)`: the verbatim answer "Approve all (Recommended)", 2026-10-22 UTC, and the resolved option for each of the 26 items.

## Task Commits

1. **Task 1 (tracer): decisions, proposal, COVERAGE.md** - `76eae72` (docs)
2. **Task 2: host grant and production git.yaml answer** - `9cb8617` (docs)
3. **Task 3: contract decisions** - `c4bea33` (docs)

**Plan metadata:** the final docs commit (SUMMARY, STATE, ROADMAP).

## Chosen option per item

All 23 open items were approved at their Recommended option, so no item differs from its Recommended value. Chosen: K1 a, K2 a, K3 a, K4 b, K5 a, K6 a, K7 a, K8 a, K9 a, K10 a (decided D-13), K11 a, K12 a, K13 a, K14 a, K15 a, V1-V7 as written, V8 a (decided D-14), V9 as written, V10 as written, V11 a (decided D-15).

## Later plans affected by the departures from plan text

The answer approved the Recommended wording where it departs from the plan text. Each plan below must follow the wording recorded under `## Contract decisions (25-03)`:

| Departure | Plans |
| --- | --- |
| K2: a directory inside a resource folder refuses `deploy_tree_invalid` | 25-04, 25-05, 25-06, 25-11 |
| K3: `deploy` denied at the repo root only (not at any depth) | 25-04, 25-06 |
| K4 b: `files` array sorted, `project.json` absent `description`/`parent` read as "" | 25-04, 25-05, 25-19 |
| K6: parent facts from one `projects/list` call; `ProjectRecord.invalid_parent` | 25-10, 25-04 |
| K7: `CHILDREN_SETTLE_S` = 30 | 25-15 |
| K9: `stat` and `Get-Item -Force` instead of a marker-led sftp listing (the 25-08 plan text names the sftp listing) | 25-07, 25-08 |

K4 b equals Recommended, so the 25-04 precondition ("answered with an option other than its Recommended one") holds. The planner should confirm 25-08's `git_entry` wording and acceptance text against K9 a before 25-08 runs.

## Deviations from Plan

### Auto-fixed Issues

None. The host-grant step was re-run through the existing `capture25.sh ch lnx|win` driver with `CAPTURE_OUT` pointed at a scratch file, and the relevant rows were summarised into `## Host grant and prod git.yaml (25-03)` (not the whole 1500-line output), as the grant text needs the narrow-then-grant-then-restore cycle on a test directory.

### Notes

- The plan's Task 2 verification asks for the production keys "pasted verbatim". The user did not paste keys; they confirmed the C10 production variant key set shown to them. This is recorded as such (user-confirmed, nothing pasted, no secret collected).
- `Test-Path -LiteralPath` on Windows threw `UnauthorizedAccessException` before the grant in this run, where the first capture recorded `Test-Path=False` with no error. The cause is not established; `Test-Path` stays unused (K9 uses `Get-Item -Force`, which answered denied in both runs).
- The persistent default ACLs on `data/projects` listed in `### C-H grant text` were not applied: the authorization named the test project directories. As provisioned no grant is needed.

## Open Items / Deferred

- **OPEN (cutover, D-11): long-term branch lifecycle of module-managed projects.** The user's words, verbatim: "Well depends. We won't want this to be on a stray production branch forever. May need to think this through. Otherwise I think its fine". Not settled by Phase 25; it does not change K8. The 25-17 cutover runbook restates it as an open owner question (alongside research Open Question 5, themes and images after the WHK-Global cutover).
- S5 (session impact on a child project after a parent overwrite) stays OPEN from C9; no criterion claims it.
- The Designer-side pull, save and hotfix paths of the git module, `tags_importOnStartup` and the real production `git.yaml` remain unexercised (C10).

## Authentication Gates

None beyond the recorded admin routes, which the user authorized ("Authorize grant").

## Known Stubs

None.

## Threat Flags

None. T-25-08 (decisions recorded verbatim with a UTC date), T-25-09 (no production configuration or credential collected) and T-25-10 (grant list/traverse only, applied to a test directory, restored) are mitigated as planned. The word `token`/`secret` appears in the new sections only in prose ("no key, token or password text is recorded").

## Self-Check: PASSED

- `25-LIVE-CAPTURES.md`: `## Decisions` has 8 capture subsections, `## Contract proposal (25-03)` has 26 rows, `## Host grant and prod git.yaml (25-03)` and `## Contract decisions (25-03)` each exist once (26 decision rows).
- `COVERAGE.md` exists with 9 OPT-OUT rows, each reasoned, and no "to be decided" text.
- Commits `76eae72`, `9cb8617`, `c4bea33` exist on `main`.
