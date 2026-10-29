---
phase: quick-261029-df7
plan: 01
subsystem: ci-cd
tags: [github-actions, attestation, supply-chain, deploy-projects, release]
requires:
  - phase: 25-guarded-project-deploy
    provides: deploy-projects actions and the 25-REVIEW.md WR-04 finding
provides:
  - Opt-in verify-attestation input on both deploy-projects actions and deploy-projects.yml
  - Attestation gate in shared:install-ign (four byte-identical copies)
  - release.yml attest job publishing build-provenance for each archive
affects: [release.yml, deploy-projects, deploy-plan, deploy-apply]
tech-stack:
  added: [actions/attest-build-provenance@v4, actions/download-artifact@v8]
  patterns: [job-scoped id-token and attestations write, gh attestation verify bound to repo, signer workflow, tag ref and hosted runners]
key-files:
  created: []
  modified:
    - .github/actions/deploy-projects-check/action.yml
    - .github/actions/deploy-projects-apply/action.yml
    - .github/workflows/deploy-plan.yml
    - .github/workflows/deploy-apply.yml
    - .github/workflows/deploy-projects.yml
    - .github/workflows/release.yml
    - RELEASING.md
    - README.md
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/release-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/readme-test.sh
    - .planning/phases/25-guarded-project-deploy/25-REVIEW-FIX.md
key-decisions:
  - "Gate sits in shared:install-ign between the sha256 check and extraction, so the binary never runs before its provenance is checked"
  - "Unset IGN_VERIFY_ATTESTATION means false (no-colon expansion); set-but-empty is refused"
  - "GH_TOKEN is set only when the input is exactly true and unset before the binary runs"
  - "attest is its own job so id-token/attestations write never reach the build job's third-party actions"
  - "(F6) attest signs the archives this run's build job uploaded, never the release's assets"
  - "(F7) attest runs for refs/tags/v* only; verify adds --source-ref refs/tags/<tag> and --deny-self-hosted-runners"
requirements-completed: [QUICK-df7, WR-04]
duration: single session
completed: 2026-10-29
status: complete
estimate:
  tokens: 120000
  tasks: 3
actuals:
  tokens: 9900
  tasks: 3
  commits: 5
---

# Phase quick-261029-df7 Plan 01: Optional release-binary attestation (WR-04) Summary

Opt-in `verify-attestation` that runs `gh attestation verify` (repo, signer workflow, tag ref, hosted runners) on the downloaded `ign` asset after the sha256 check and before extraction, backed by a tag-only `attest` job in release.yml that signs this run's own build archives.

## Commits

| Commit | Message |
|--------|---------|
| 876fb62 | fix(25): WR-04 opt-in release attestation check in the ign install step (Task 1, tracer) |
| 42250d7 | feat(release): WR-04 publish build-provenance attestations and pass verify-attestation through deploy-projects (Task 2) |
| bcdf27b | docs(25): WR-04 document verify-attestation and the release binary trust model (Task 3) |
| e92d430 | fix(quick-261029-df7): bind attestation to this run's build output and the tag ref (orchestrator review F6, F7) |
| 4fd8f2f | docs(25): record WR-04 opt-in attestation in the review fix report (Task 3) |

## Harness and lint results

| Check | Result |
|-------|--------|
| projects-test.sh | PASS, 46 PASS lines, exit 0 (includes 2a, 3d2, 3d3, 3h, 3i, 4j) |
| apply-test.sh | PASS, 30 PASS lines, exit 0 |
| release-test.sh | PASS, cases 1-9, exit 0 |
| readme-test.sh | PASS, 11 PASS lines (cases 1-7, 6a-6d, 7a, 7b), exit 0 |
| plan-test.sh | PASS, 17 PASS lines, exit 0. Docker was reachable; case 2a ran the changed install body against the real v1.5.0 release |
| actionlint release.yml, deploy-plan.yml, deploy-apply.yml | PASS (clean) |
| actionlint deploy-projects.yml | PASS only with the two `job.workflow_repository` / `job.workflow_sha` "not defined" messages ignored. Plain actionlint reports those 10 messages on the 408f65b file too (installed actionlint predates the properties); harness case 1a ignores them the same way |

The default path (verify-attestation false, and unset) is byte-identical to the 408f65b install body: stdout, stderr, exit code, GITHUB_ENV and curl calls, with gh never invoked (case 4j).

## Deviations from Plan

### Orchestrator-directed change (security review)

**1. [Review F6/F7] Attest the run's own build output, bind verify to the tag**
- **Found during:** after Task 2, from the orchestrator's review of the release.yml commit.
- **Issue (F6):** the attest job downloaded assets from the release and checked them against `.sha256` files from the same release, so a workflow_dispatch re-run would sign whatever sits on the release. **Issue (F7):** the signer identity was not bound to a ref.
- **Fix:** build job now uploads each archive with `actions/upload-artifact@v7` (`ign-archive-<target>`, before the release upload). attest uses `actions/download-artifact@v8` and attests `assets/ign-*`; the `gh release download` path and the sha256 re-check are gone. attest has `if: startsWith(github.ref, 'refs/tags/v')`. Every shared install block adds `--source-ref "refs/tags/$tag" --deny-self-hosted-runners`. Harness 4j argv, release-test 7-9, README trust note and RELEASING.md updated.
- **Consequences:** the build job is no longer byte-for-byte unchanged (it gained the upload step, which is first-party and holds no token); the .sha256 files are no longer attested, archives only; the attest job has two `uses:` lines (download-artifact and attest). A workflow_dispatch run from a branch publishes no attestation.
- **Commit:** e92d430

### Process note

**2. Tracer gate.** `workflow.auto_advance` and `_auto_chain_active` are both false in `.planning/config.json`, so the interactive tracer gate would have stopped for a human-verify checkpoint after Task 1. The caller directed execution of all tasks, and the tracer's `<verify>` is a fully automated harness run, so I re-ran it end to end (projects-test, apply-test, plan-test, actionlint all green) and continued instead of returning a checkpoint.

**3. Rule 3 note on verification.** Case 4j and the baseline comparison passed on the first GREEN run. I did not separately demonstrate RED for 3h/4j (RED was demonstrated at 3d2 only, the harness stops at first failure).

Otherwise the plan was executed as written.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. The F6/F7 changes narrow T-df7-01 and T-df7-06 (the attest job no longer reads release assets).

## Pending human verification

A live `gh attestation verify` cannot be exercised until a release is built from a tag after this change (v1.6.0-rc.3 or later). After that tag runs release.yml: the attest job is green, `gh attestation verify ign-<tag>-x86_64-unknown-linux-gnu.tar.gz --repo TheThoughtagen/ignition-cli --signer-workflow TheThoughtagen/ignition-cli/.github/workflows/release.yml --source-ref refs/tags/<tag> --deny-self-hosted-runners` on a downloaded archive prints a verified result, and a deploy-projects run with `verify-attestation: true` against that tag installs `ign`. Also unverified: that `actions/download-artifact@v8` with `pattern` and `merge-multiple` lays the files out directly under `assets/` (the list step fails clearly if not).

## Self-Check: PASSED

All five commits found in `git log` (876fb62, 42250d7, bcdf27b, e92d430, 4fd8f2f); all listed files exist; `.planning/config.json` and the two untracked paths untouched.
