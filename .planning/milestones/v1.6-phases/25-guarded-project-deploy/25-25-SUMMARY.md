---
phase: 25-guarded-project-deploy
plan: 25
subsystem: ci-cd
tags: [github-actions, composite-action, deploy, secrets, autocrlf]
requires:
  - phase: 25-guarded-project-deploy
    provides: deploy-projects.yml reusable workflow, quick-m5y harness, adopt write scope (25-24)
provides:
  - composite action deploy-projects-check (read-only check, no write-capable input)
  - composite action deploy-projects-apply (requires artifact-sha256 and targets-sha, refuses before any checkout)
  - deploy-projects.yml rewired to run both actions from its own commit (job.workflow_sha) with a named cross-org failure
  - core.autocrlf disabled on every checkout and git or ign step through GIT_CONFIG_COUNT/KEY_0/VALUE_0
affects: [25-26, 25-27]
tech-stack:
  added: []
  patterns:
    - "step logic lives once in composite actions; the reusable workflow checks them out at job.workflow_sha and verifies HEAD"
    - "secrets reach composite actions as inputs; the caller's own environment job resolves them natively"
key-files:
  created:
    - .github/actions/deploy-projects-check/action.yml
    - .github/actions/deploy-projects-apply/action.yml
  modified:
    - .github/workflows/deploy-projects.yml
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/lib.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh
key-decisions:
  - "O3: composite actions in this public repo carry the logic so a caller in any organization owns both jobs and its apply job's environment resolves secrets natively"
  - "deploy-projects.yml stays for same-org callers and holds no step body other than actions-ref, verify-actions-sha and apply-secrets"
  - "GIT_CONFIG_* step env (command-line scope) outranks the Git for Windows system gitconfig, so no runner config is needed"
requirements-completed: [DPRJ-01, DPRJ-05]
duration: 40min
completed: 2026-10-28
status: complete
gap_ids: [G-25-13, G-25-9]
actuals:
  tokens: 39500
  tasks: 2
  commits: 2
---

# Phase 25 Plan 25: Composite deploy actions and autocrlf fix Summary

Check and apply step logic moved into two composite actions (byte-identical bodies) so a caller in any organization can run the guarded deploy with its own environment secrets, and every checkout and git/ign step now disables core.autocrlf for itself.

## What was built

- `deploy-projects-check` action: 14 inputs, none write-capable (`check-token`, `check-ssh-key`), no `secrets.` reference; outputs `artifact_sha256`, `action`, `targets_sha`.
- `deploy-projects-apply` action: first step `body:apply-handoff` refuses a non-64-hex `artifact-sha256`, a non-40-hex `targets-sha`, an empty `master-token`, and an empty `ssh-key` when `ssh-key-env` is set, each naming the input, before any checkout or ign command. Targets are read at `targets-sha` and verified.
- `deploy-projects.yml`: each job locates (`body:actions-ref`), checks out (sparse `.github/actions`, no persisted credentials) at `job.workflow_sha`, verifies HEAD (`body:verify-actions-sha`) and runs the action. The apply job first runs `body:apply-secrets`, which names the `secrets: inherit` same-organization cause and the composite actions when IGN_MASTER_TOKEN or IGN_SSH_KEY is empty. Header, `ssh-key-env`, `runs-on` and secret descriptions updated (G-25-8, G-25-12).
- Harness: `CHECK_ACTION`, `APPLY_ACTION`, `lint_with_local_actions`, `shellcheck_bodies`, `autocrlf_checks`; projects-test.sh retargeted with new cases 1c, 2c, 3d2, 3d3, 3f, 3g, 3g2, 4e, 4f, 4g, 4h.

## Tasks

| Task | Name | Commit |
| ---- | ---- | ------ |
| 1 (tracer) | Check path through a composite action, autocrlf disabled on every checkout | 6446593 |
| 2 | Apply path through a composite action requiring the checked hash and commit; apply-secrets cause | 6ca2016 |

The tracer gate: after Task 1 the full projects-test.sh, plan-test.sh and apply-test.sh were green in the mixed state (check from the action, apply inline) before Task 2 expanded.

## Verification

- projects-test.sh: all cases PASS (1a-1c, 2a-2c, 3a-3g2 including 3d2/3d3, 4a-4h, 5a-5c, 6, 7).
- plan-test.sh, apply-test.sh, readme-test.sh, release-test.sh: exit 0.
- `git diff --quiet HEAD -- deploy-plan.yml deploy-apply.yml`: clean (untouched).
- Byte-identical move confirmed with the harness `extract_first` against `git show 46ef601:.github/workflows/deploy-projects.yml`: run-check, check-report, targets-args, targets-sha (check action); run-apply, stage, apply-report, verify-targets-sha, install-ign, write-config, targets-args (apply action).
- Negative check: a bogus input added to the workflow copy is rejected by actionlint ("input bogus-input is not defined in action"), proving local action inputs are validated.
- Counts: `>>> shared:` in deploy-projects.yml 0; `>>> body:` 5; `GIT_CONFIG_KEY_0: core.autocrlf` in the check action 4.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] G-25-9 env on the still-inline apply job in Task 1**
- **Found during:** Task 1 (3f autocrlf_checks on PROJECTS_WF)
- **Issue:** The plan leaves the apply job untouched in Task 1 but requires 3f to pass on deploy-projects.yml, whose inline apply job had checkouts and a git/ign step without the autocrlf env.
- **Fix:** Added the three GIT_CONFIG env keys to the apply job's two checkouts, `Verify targets commit` and `Run ign deploy projects` in Task 1; Task 2 then moved them into the action.
- **Commit:** 6446593

**2. [Rule 1 - Bug] Harness grep patterns matched substrings**
- **Found during:** Task 1 and 2 harness runs
- **Issue:** `grep -F 'token: ${{ secrets.IGN_TARGETS_TOKEN }}'` also matched `targets-token: ...`, and the inputs_of regex `[a-z][a-z-]*` missed `artifact-sha256`.
- **Fix:** Anchored the token-line regexes and allowed digits in the input-name regex.
- **Commits:** 6446593, 6ca2016

Secret descriptions in deploy-projects.yml say `secrets inherit` (no colon) because a `: ` inside a plain YAML scalar is a syntax error; the prose meaning is unchanged.

## Known Stubs

None.

## Threat Flags

None. The new cross-repository `uses:` and the `job.workflow_sha` checkout are the surface the plan's threat model (T-25-84 to T-25-90) already covers.

## Open items for later plans

- `job.workflow_repository` and `job.workflow_sha` are unknown to the installed actionlint (1.7.12); the harness ignores exactly those two messages. Live population is proven only by plan 25-27.
- README caller template and Secrets section: plan 25-26.

## Self-Check: PASSED

Created files found: both action.yml files, this SUMMARY. Commits 6446593 and 6ca2016 exist on branch phase-25-gaps-rc3.
