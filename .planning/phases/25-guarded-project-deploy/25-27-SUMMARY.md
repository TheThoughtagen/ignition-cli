---
phase: 25-guarded-project-deploy
plan: 27
subsystem: deploy-ci
tags: [uat, github-actions, cross-org, windows, composite-actions, canonical-digest]
requires:
  - phase: 25-guarded-project-deploy
    provides: static CRT release (25-22), owner ACL by SID (25-23), write-admission adopt flags (25-24), composite actions (25-25), README template (25-26)
provides:
  - live cross-org UAT of the composite actions on a reverted Windows runner (create, import, concurrency)
  - K4 canonical digest defaults for inheritable, version, restricted and overridable
  - corrected cross-org secret-delivery facts (explicit mapping works from any organization)
affects: [25-VERIFICATION]
tech-stack:
  added: []
  patterns:
    - "An absent descriptor key hashes as the default the gateway writes on import"
key-files:
  created:
    - .planning/phases/25-guarded-project-deploy/uat/deploy-composite.yml
    - .planning/phases/25-guarded-project-deploy/uat/deploy-reusable-crossorg.yml
  modified:
    - crates/ignition-core/src/deploy/project_artifact.rs
    - .github/workflows/deploy-projects.yml
    - README.md
    - .planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md
key-decisions:
  - "Operator: fix the product for the gateway's create-time descriptor defaults rather than the UAT fixture (0ab4093, second pre-release v1.6.0-uat.2)"
  - "Operator: re-probe before changing the secrets docs; the probe showed an explicit secrets mapping delivers environment secrets to a reusable workflow from any organization (d74a673)"
requirements-completed: [DPRJ-01, DPRJ-02, DPRJ-03, DPRJ-04, DPRJ-05]
completed: 2026-10-29
status: complete
gap_ids: [G-25-13, G-25-7, G-25-9, G-25-10, G-25-11, G-25-12, G-25-8]
actuals:
  tasks: 3
  commits: 8
---

# Phase 25 Plan 27: Live cross-org UAT on ign-win-836 Summary

From a WhiskeyHouse caller, the composite actions created the UAT project and then replaced it after a one-line change. The Windows runner had every earlier workaround reverted (NETWORK SERVICE, `core.autocrlf=true`, no VC++ runtime), and the logs hold no key or token. The run found one product defect, which is fixed, and one wrong documentation claim, which is corrected.

## What was built

- Task 1: the UAT callers `uat/deploy-composite.yml` and `uat/deploy-reusable-crossorg.yml`, plus the pending capture section (ca664a8).
- Task 2: the operator chose route (a), uat-build.
- Task 3:
  - **Live run:** pre-release build, VM reverts, token minting (admin credential reset first), run A, run B, the cross-org reusable-workflow run, the concurrency queue, the log scans and the cleanup. Every capture cell is filled.
  - **Product fix:** after `project.json` and `resource.json` are imported, the gateway writes in defaults the canonical digest did not fill (0ab4093).
  - **Docs fix:** the cross-org secret-delivery facts are corrected (d74a673).
  - **README:** the testing status now covers what was proven live.

## Live results

| Run | Tag | Outcome |
| --- | --- | --- |
| A | v0.0.8-test, run 40771157285 | `create` -> `created`; read-back equal, record written |
| B | v0.0.9-test, run 40775414088 | `import` -> `imported`; check and apply artifact_sha256 equal, live_sha256 before the import equal to run A's |
| reusable workflow, cross-org | run 40794529806 | `skipped`; the environment secrets reached the approved apply job through a by-name mapping |
| concurrency | v0.0.10/11/12-test | v0.0.11 replaced while pending (cancelled); v0.0.10 and v0.0.12 `recorded` (K14) |

- **Runner diagnostics in every job:** SID `S-1-5-20`, `core.autocrlf: true`, `vcruntime140.dll present: False`, and every targets file reads `w/lf`.
- **Log scans:** every run has 0 private-key headers and 0 check-token matches.

## Commits

| Item | Commit |
| --- | --- |
| UAT callers and pending capture section | ca664a8 |
| Route, release run and VM reverts recorded | 72ebd53 |
| Token minting and deviations recorded | b321fae |
| UAT diagnostics on Windows PowerShell | 204257a |
| Line-endings diagnostic without lazy fetch | 00101b5 |
| K4 defaults for inheritable, version, restricted and overridable | 0ab4093 |
| Cross-org explicit secret mapping documented | d74a673 |
| Captures, README testing status and this summary | the final docs commit |

## Verification

- `cargo test --workspace`: 2929 passed, 0 failed. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- Harness: plan-test, apply-test, readme-test, projects-test and release-test all exit 0. After the final README edit, readme-test was rerun and exits 0.
- actionlint on `deploy-projects.yml`: the same 10 known `job.workflow_*` findings as before the edit.
- Release run 40770020656 (v1.6.0-uat.2): success on all 5 targets, and the publish-crates steps were skipped.
- Cleanup:
  - `gh release view` exits 1 for v1.6.0-uat.1 and v1.6.0-uat.2, and neither tag remains.
  - ign-win-836 is `PowerState/deallocated`.

## Deviations from Plan

1. **Product code changed inside the checkpoint** (the plan says to change no code there; the operator chose the fix). The details are in the capture section.
   - Run 40755290379 ended `deploy_project_unverified`. The hand-written UAT descriptors lacked `inheritable`, `version`, `restricted` and `overridable`, which the gateway adds on create.
   - The fix treats an absent key as its gateway default, and any other value still counts as a change. Repos that already carry the keys hash the same as before.
   - Rolling it out took a second pre-release and a deletion of the CAPUAT project the failed create left behind.
2. **The cross-org reusable-workflow run did not fail.**
   - The G-25-13 probe never tried an explicit mapping. Rerun with one (run 40803747460), it shows the environment secret is delivered for every name the caller passes.
   - The README, the workflow header and the apply-secrets cause text are corrected. G-25-13 stays closed by the composite actions.
   - The probe ran on the self-hosted runner, because GitHub-hosted runners in the UAT organization are blocked by billing.
3. **UAT caller defects from Task 1** (the `pwsh` shell and the hung line-endings listing) were fixed in 204257a and 00101b5. A workspace owned by the old account was removed from the runner. None of these are product defects.
4. **Gateway admin credential reset on the test VM**, with operator approval, so tokens could be minted.
5. **VM auto-shutdown at 03:00 UTC** deallocated the VM while an approved apply was queued. The VM was restarted and the job ran.
6. **Expectation corrections:**
   - The concurrency applies ended `recorded`, not `skipped`. The content was equal, but each run carried a new release tag (K14).
   - Tags run from v0.0.8-test because the UAT ruleset blocks reuse of earlier tags.

## Follow-ups

- The DevTest auto-shutdown on ign-win-836 can interrupt long UAT sessions. Check the clock before starting one.
- The UAT organization's GitHub-hosted runners are blocked by a billing problem, which is unrelated to this repository.
