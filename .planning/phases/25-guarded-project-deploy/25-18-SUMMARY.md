---
phase: 25-guarded-project-deploy
plan: 18
subsystem: ci
tags: [deploy, projects, github-actions, reusable-workflow, targets, approval-gate, readme, dprj-01, dprj-05]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-16 `--targets DIR` on the deploy leaves; 25-17 the cutover runbook, project repo checklist and stdout purity of `deploy projects`; the C10 verdicts and K8 a in 25-LIVE-CAPTURES.md (`## Contract decisions (25-03)`, binding)"
provides:
  - ".github/workflows/deploy-projects.yml: reusable `check` then `apply` workflow for one project and one environment; apply `needs: check`, runs under `environment: ${{ inputs.environment }}` and deploys exactly the `artifact_sha256` the check produced"
  - "separate secret sets: `check` holds `IGN_CHECK_TOKEN` (read-only gateway token) and `IGN_CHECK_SSH_KEY`; `apply` holds `IGN_MASTER_TOKEN` and `IGN_SSH_KEY`; no fallback either way"
  - "`targets-repository`, `targets-ref`, `targets-dir` inputs and `IGN_TARGETS_TOKEN` on deploy-plan.yml and deploy-apply.yml (optional; existing callers unchanged), checkout at `.ign-targets`"
  - "`shared:targets-args` region, byte-identical in all three workflows (two copies in deploy-projects.yml)"
  - "README `### Running \\`deploy projects\\` in CI` with the dev-then-prod caller, the secret split and the concurrency rule; targets inputs added to `### Running deploy in CI`"
  - "harness: projects-test.sh (28 cases), targets static checks and a `.ign-targets` end-to-end case in plan-test.sh, targets-args diff in apply-test.sh, readme-test.sh re-pinned to 4814da9 plus a section 5"
affects: [25-19]

actuals:
  tokens: 28315   # chars/4 over the added lines of `git diff 4d1e938~1..HEAD -- .github README.md .planning/quick` (113259 chars)
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "approval-gate secret split: the job that runs before the environment approval gets its own read-only secrets; write-capable secrets are environment secrets released only to the job that names the environment"
    - "step-local variable names stay `IGN_MASTER_TOKEN` and `IGN_SSH_KEY` inside the shared `ign-env` region while the `env:` mapping reads a different secret per job, so the region stays byte-identical with deploy-plan.yml"
    - "per-job static checks by line range (`check:` to `apply:`) in the harness, with mutation checks that prove each rule can fail"

key-files:
  created:
    - .github/workflows/deploy-projects.yml
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh
  modified:
    - .github/workflows/deploy-plan.yml
    - .github/workflows/deploy-apply.yml
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/lib.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/plan-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/apply-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/readme-test.sh
    - README.md

key-decisions:
  - "The ungated `check` job gets its own secrets (`IGN_CHECK_TOKEN`, `IGN_CHECK_SSH_KEY`) and never references `IGN_MASTER_TOKEN` or `IGN_SSH_KEY` (user decision \"Separate check secrets\", from a MEDIUM review finding)."
  - "The apply job runs even when the check reports `skip` (user accepted at the tracer gate); `ign` then ends `skipped` and writes nothing."
  - "A newer pending apply replaces an older pending one in the concurrency group (user accepted at the tracer gate); documented in the README."
  - "No requirement is marked complete (see Requirement status)."

patterns-established:
  - "`targets_static_checks <workflow>` in lib.sh: one place that pins the optional-targets wiring of a workflow"

requirements-completed: []  # DPRJ-01 and DPRJ-05 stay open: 25-19 lists all five and proves them on a live rig; the GitHub-only behaviour of this plan is unproven

duration: "tracer 4d1e938 at 16:48 MDT; the continuation (secret-split fix, Task 2, this summary) finished at about 20:30 MDT, including the wait at the human-verify gate"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 18: Reusable deploy-projects workflow, targets inputs and the CI section Summary

**A project repo can call one reusable workflow per environment to check (GET only), wait for the environment's reviewers, and apply exactly the checked `artifact_sha256`, with the unapproved check job holding only read-only secrets and the targets read from the IaC repo; the gateway-config workflows gain the same optional targets inputs.**

## Accomplishments

- **Task 1, tracer (`4d1e938`, accepted at the human-verify gate: "Accept, continue (Recommended)").** `deploy-projects.yml` with a `check` job (`--check --json`, exposes `artifact_sha256`) and an `apply` job (`needs: check`, `environment:`, concurrency group `ign-deploy-projects-<env>-<repository>` with `cancel-in-progress: false`, `--artifact-sha256 ... --designers wait --yes --json`), the report steps (check summary, apply classification, one `::warning::` per child `mismatch`, `deploy_project_unverified` with the restore command), staging that uploads only `run.json`, `result.json`, `events.ndjson` and the two envelopes, and `projects-test.sh`.
- **Security fix (`2361c49`, between the tracer and Task 2).** See Deviations 1.
- **Task 2 (`9406237`).**
  - `deploy-plan.yml` and `deploy-apply.yml`: optional `targets-repository` (default empty), `targets-ref` (`main`), `targets-dir` (`ignition`) and `IGN_TARGETS_TOKEN`; a `Check targets inputs` step (the shared `targets-args` region, `IGN_TARGETS_REQUIRED: 'false'`), a conditional `Checkout targets` step (`if: inputs.targets-repository != ''`, `persist-credentials: false`, sparse checkout of `targets-dir`) into `.ign-targets`, and `--targets` added to the `ign` command only when `IGN_TARGETS_DIR` is set (`${targets_args[@]+"${targets_args[@]}"}`, safe under `set -u` on any bash).
  - `.ign-targets` is harmless (confirmed in source and by a harness case): the declared tree is read only from `<repo>/gateways/<gateway>/` (`load_declared_in`: `require_repo_dir(repo, "gateways")`, `SourceTree::read_gateway(gateway_dir)`), the only `git` call is `check-ignore` on `.ign/`, and `.ign-targets` sits beside `gateways/`, not inside it. `plan-test.sh` case 4b builds a repo with no `targets/`, a poisoned `.ign-targets` (an unparsable `gateways/tracer-gw/manifest.yaml`, a symlink loop, a nested `.git`) and shows the plan fails without `--targets`, succeeds with `--targets .ign-targets/ignition`, and never prints the token.
  - README: `### Running \`deploy projects\` in CI` (caller with `on: push: tags: ['v*']`, `dev` then `prod` with `needs: dev` and `release: ${{ github.ref_name }}`; the secret table and the environment-secret instructions; the check key trade-off; the concurrency rule that a newer pending apply replaces an older pending one; targets; child warnings; failures; links to `#what-a-project-repo-needs` and `#cutting-a-project-over-from-the-git-module`, as 25-17 asked; the GitHub-only testing status). The gateway-config example shows the targets inputs for WHK-Global with the D-10 sentence.
  - Harness: `projects-test.sh` 2b diffs `shared:targets-args` across plan, apply and both jobs of projects; `apply-test.sh` adds `targets-args` to the shared loop and runs `targets_static_checks` on both; `readme-test.sh` is re-pinned from 44919d4 to 4814da9 (the 25-11 commit, the last to change the `## Exit codes` region, confirmed by hashing the region at each README commit) and gains a section 5 (heading once, after Running deploy in CI, required terms, every in-page link resolves).

## Verification

All run on the final tree, `CARGO_TARGET_DIR` in the session scratchpad, no worktree, mock gateway killed (`pgrep` finds none).

- `projects-test.sh`: 28 `PASS` lines, exit 0 (`1a` actionlint+shellcheck, `2a`/`2b` shared regions, `3a` to `3d` structure and secret split, `4a` to `4c` targets validation, `5a`, `5a2`, `5b`, `5c` a real `ign --check` and refusals against a mock gateway, `6` staging, `7` report cases).
- `plan-test.sh` (Docker available, install exercised in `buildpack-deps:noble-curl`): all `PASS`, including `4a` and `4b`. `apply-test.sh`: 30 `PASS`. `readme-test.sh`: 5 `PASS`.
- `actionlint .github/workflows/*.yml`: clean (it runs shellcheck on the `run:` bodies). `shellcheck -x -S warning` over the harness: clean; the info-level notes (SC1091, SC2016, SC2030/SC2031) are the existing pattern of the harness.
- `cargo test -p ignition-cli --test deploy_family_coverage`: 9 passed. No crate source was touched, so clippy and `fmt --check` were not run.
- Mutation checks of the secret-split rule (each restored from a backup before commit): the check job taking `secrets.IGN_MASTER_TOKEN` fails `3d`; the apply job taking `secrets.IGN_CHECK_SSH_KEY` fails `3d`; `secrets.IGN_MASTER_TOKEN || secrets.IGN_TARGETS_TOKEN` fails `3b`.

## Deviations from Plan

**1. [Rule 2 - Missing critical functionality] The ungated check job held the write secrets (approval-gate bypass, MEDIUM).** Found by the security review after the tracer. The `check` job runs before and without the environment approval, yet received `IGN_MASTER_TOKEN` and `IGN_SSH_KEY`, the secrets the approved `apply` job uses, so anyone able to push a `v*` tag could use them without a reviewer. User decision: "Separate check secrets". Fix: new secrets `IGN_CHECK_TOKEN` (required, read-only gateway token) and `IGN_CHECK_SSH_KEY`; the check job's `env:` maps `IGN_MASTER_TOKEN: ${{ secrets.IGN_CHECK_TOKEN }}` and `IGN_SSH_KEY: ${{ secrets.IGN_CHECK_SSH_KEY }}`, the apply job keeps `IGN_MASTER_TOKEN` and `IGN_SSH_KEY`, and there is no fallback. The check's own refusal text now says `IGN_CHECK_TOKEN`. Files: `.github/workflows/deploy-projects.yml`, `projects-test.sh`. Harness: `3d` (the check job never references `secrets.IGN_MASTER_TOKEN` or `secrets.IGN_SSH_KEY`; the apply job never references the check secrets; each job maps the expected secret; no `||` fallback, `secrets[`, `toJSON(secrets` or `secrets: inherit`) and `5a2`. Commit `2361c49`. The README tells callers to store the write token and key as secrets of the `environment` apply uses, to give check a read-only gateway token, and sets out the check key trade-off (the same deploy key, simple but writable by an unapproved job; a second key for the same account restricted read-only by the host, untested here; never a different account, because the release record lives in the deploy account's home). Note on the wording of the rule: the step-local variable names `IGN_MASTER_TOKEN` and `IGN_SSH_KEY` still appear inside the check job's `shared:ign-env` region, which must stay byte-identical with deploy-plan.yml; they are local `env:` names, and the rule is enforced on the `secrets.` context, which is what grants access.

**Task 1 deviations (from the tracer, `4d1e938`):**

2. **`project/` and `targets/` are sibling checkout paths,** not `.ign-targets` beside the repo as the plan said, because `actions/checkout` refuses a path outside `GITHUB_WORKSPACE`. `repo-dir` is relative to `project/`. The gateway-config workflows check out at the workspace root, so there the targets go to `.ign-targets`.
3. **`actions/github-script` instead of `jq`** for the check and apply summaries, because Windows self-hosted runners may lack `jq`.
4. **`shared:targets-args` was created in the tracer,** not in Task 2, because the projects workflow needed the validation step first; Task 2 adopts it in plan and apply.
5. **`projects-test.sh` has its own inline node runner** for the report bodies, so `report-test.mjs` is untouched.
6. **The apply job is unconditional (user-accepted).** It runs when the check reports `skip`; the reviewer approves a run that ends `skipped` and writes nothing.
7. **Concurrency replaces a pending apply (user-accepted).** `cancel-in-progress: false` keeps one running and one pending apply; a newer pending apply replaces the older pending one. Documented in the README.

**Task 2 deviations:**

8. **The `Check targets inputs` step is unconditional in plan and apply** (the region itself exits 0 for an empty `targets-repository` when `IGN_TARGETS_REQUIRED` is `false`); only the checkout is conditional. Fewer conditional steps, same effect.
9. **`readme-test.sh` section 4 now stops at the next `### Running` heading** so the new section's terms are not counted for the older one.
10. **A `.ign-targets` end-to-end case was added to `plan-test.sh`** (not asked for by name) to back the "harmless, not walked" claim with a run instead of a reading of the source.
11. **The header comments of deploy-plan.yml and deploy-apply.yml** now say the shared regions are diffed across the three files, and describe the optional targets inputs.

### TDD Gate Compliance

Plan type is `execute`; the plan-level gate does not apply. Neither task is `tdd="true"`.

## What can only be proven on GitHub (not exercised)

The harness extracts the `run:` bodies and `script:` bodies and runs them locally; these remain unproven until a real run:

- Sparse cross-repository checkout of `targets-dir` with `IGN_TARGETS_TOKEN`, and the masking of that token in the runner log.
- Environment secrets (`IGN_MASTER_TOKEN`, `IGN_SSH_KEY`) reaching a reusable workflow's `apply` job through that job's own `environment:`, with the caller mapping nothing. This is the premise of the secret split.
- The concurrency queue: one running and one pending per group, and a newer pending apply replacing an older one, including what happens to the `prod` job that `needs` the replaced `dev` job.
- The `check` job's `artifact_sha256` output reaching the `apply` job through `needs`, and the workflow outputs through `workflow_call`.
- Behaviour on a self-hosted Windows runner (Git Bash, the Win32-OpenSSH client, `cygpath`).

## Requirement status

`requirements-completed` is empty on purpose.

- **DPRJ-01** (every project artifact verified against its hash before any upload) is also listed by **25-19** (`[DPRJ-01 .. DPRJ-05]`, the live gates on a real 8.3.6 rig). This plan wires the pinned hash from `check` into `apply` and the harness shows the refusal of a wrong hash (`5b`, `project_hash_mismatch`, exit 6) against a mock gateway, but the property is the verb's, proven live by 25-19.
- **DPRJ-05** (`--check` reports what would change with no writes) is also listed by 25-19. The harness runs a real `--check` against a mock gateway and shows no `.ign/` is created (`5a`); a live read-only check is 25-19's gate.
- The behaviour of this plan itself on GitHub is unproven (section above). A requirement is marked by the plan that finishes it, and 25-19 finishes all five; REQUIREMENTS.md still shows DPRJ-01..05 pending, consistent with every earlier Phase 25 plan.

## Known Stubs

None. A scan of the changed files for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]`, `.skip` and placeholder text found nothing added. No entry was made in `.planning/WINDOWS.md`; the GitHub-only items above are open verification, not code stubs.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: approval_gate_bypass (pre-existing, owner follow-up) | .github/workflows/deploy-plan.yml | The ungated `plan` job maps `IGN_SSH_KEY` (`env: IGN_SSH_KEY: ${{ secrets.IGN_SSH_KEY }}`) while `deploy-apply.yml`'s approved job maps the same secret: the same pattern the review found in `deploy-projects.yml`. Per the user's instruction this plan only added targets inputs there and did not change its existing secret model. Open follow-up for the owner (give the plan job its own read-only key, as `IGN_CHECK_SSH_KEY` does here). |
| threat_flag: secret_exposure (low) | .github/workflows/deploy-plan.yml, deploy-apply.yml | `IGN_TARGETS_TOKEN` now reaches the ungated plan job. It is a read-only token for the targets repository only, never echoed, and the checkout keeps no credentials (harness `targets_static_checks`). |

The plan's register: T-25-55 (untrusted tag deploying to prod) mitigated by `environment:` plus the README's tag-ruleset and environment instructions, and now by the secret split; T-25-56 (IaC token) by a dedicated read-only secret, `persist-credentials: false` and the echo grep (`3c`, `targets_static_checks`); T-25-57 (apply not bound to the check) by `needs.check.outputs.artifact_sha256` and Decision C (`5b`); T-25-58 (concurrent runs) by the concurrency group with `cancel-in-progress: false`; T-25-59 (summary disclosure) by building the summary from the names-and-hashes envelope.

## Open items carried forward

- **Owner follow-up:** deploy-plan.yml's ungated plan job holds `IGN_SSH_KEY` (Threat Flags).
- **25-19** is the live gate for all five requirements, and the first GitHub run of the three workflows is the proof of the section above.
- **OPEN (owner), long-term branch lifecycle of module-managed projects** and **OPEN (owner), images and themes after WHK-Global's cutover**: both remain as 25-17 recorded them; this plan links the runbook and does not answer them.
- Project repo caller workflows are added by their owners; creating `ignition/targets/<env>.yaml` and the `known_hosts` files in `whk-onprem-infra-iac` is outside this repo.

## Self-Check: PASSED

- Files exist: `.github/workflows/deploy-projects.yml`, `.github/workflows/deploy-plan.yml`, `.github/workflows/deploy-apply.yml`, `README.md`, `.planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/projects-test.sh`, `lib.sh`, `plan-test.sh`, `apply-test.sh`, `readme-test.sh`: found.
- Commits exist: `4d1e938`, `2361c49`, `9406237`: found in `git log`.
- `grep -c 'needs.check.outputs.artifact_sha256' .github/workflows/deploy-projects.yml` is at least 1; `grep -c 'cancel-in-progress: false'` is at least 1; `grep -c '^### Running `deploy projects` in CI$' README.md` prints 1; `targets-repository` occurs in deploy-plan.yml and deploy-apply.yml.
- No file was deleted by any of the three commits. `.planning/config.json` and `.planning/research/.cache/` were not staged. Nothing was pushed and no GitHub run was triggered.
