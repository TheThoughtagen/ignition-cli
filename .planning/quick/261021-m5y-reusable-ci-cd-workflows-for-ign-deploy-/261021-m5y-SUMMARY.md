---
phase: quick-261021-m5y
plan: 01
subsystem: ci
tags: [github-actions, reusable-workflows, deploy, ci-cd, readme]
requires: []
provides:
  - ".github/workflows/deploy-plan.yml (reusable plan workflow, plan_sha256 output)"
  - ".github/workflows/deploy-apply.yml (reusable guarded apply workflow)"
  - "README `### Running deploy in CI` section"
affects: [README.md]
tech-stack:
  added: []
  patterns:
    - "duplicated `shared:` marker regions kept byte-identical by a harness diff instead of a composite action"
    - "github-script bodies extracted by marker and run under fake core/github/context"
key-files:
  created:
    - .github/workflows/deploy-plan.yml
    - .github/workflows/deploy-apply.yml
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/lib.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/plan-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/apply-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/readme-test.sh
    - .planning/quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/harness/report-test.mjs
  modified:
    - README.md
decisions:
  - "No composite action; install, config and env-preamble steps are duplicated in shared marker regions"
  - "ssh-key-env deny-list also rejects IGN_* names (protects IGN_BIN)"
status: complete
metrics:
  duration: "~1h (dominated by waiting on one cold cargo build)"
  completed: 2026-10-21
actuals:
  tokens: 26300
  tasks: 3
  commits: 3
---

# Phase quick-261021-m5y Plan 01: Reusable CI/CD workflows for ign deploy Summary

Two reusable `workflow_call` workflows (`deploy-plan.yml`, `deploy-apply.yml`) that plan on PRs and apply gateway configuration under environment approval with a checksum-verified, version-pinned `ign` binary, plus a README section; no Rust or release.yml change.

## Commits

| Task | Commit | Message |
| ---- | ------ | ------- |
| 1 (tracer) | 6404684 | ci(quick-m5y): reusable deploy-plan workflow |
| 2 | 0bfcf11 | ci(quick-m5y): reusable deploy-apply workflow |
| 3 | fb70f5e | docs(quick-m5y): running deploy in CI |

## What was built

- **deploy-plan.yml**: install ign from the pinned release (tag regex, https-only curl, `.sha256` file cross-checked against the asset name, SHA-256 compared before extraction, regular non-symlink binary, absolute `IGN_BIN`), job-temp `config.toml` (names and URLs only) via `IGNITION_CLI_CONFIG`, `ign deploy plan --json`, one sticky PR comment per env (only a `github-actions[bot]` comment holding the env marker is ever updated), job summary, `plan_sha256` workflow output, artifact of the value-free envelopes.
- **deploy-apply.yml**: same shared steps; runs under `environment: ${{ inputs.environment }}`, concurrency group `ign-deploy-apply-<env>` with `cancel-in-progress: false`; `deploy apply --yes --json --plan-sha256 H --designers wait --designers-timeout T`; a staged allowlist artifact (run.json, result.json, events.ndjson of this job's run dirs plus the two envelopes; hard failure on any `.gwbk`, `snapshot`, or other name); a pure `classify()` mapping exit and envelope to kinds (applied, noop, rolled_back, backup_unsynced, designers_timeout, preflight_failed, plan_stale, destructive_refused, usage, error, unparsed, not_run, unexpected); a failure comment that @mentions the actor (and the re-runner) on the PR, else the merging PR, else a commit comment; rollback command and run_id in the job summary.
- **README**: `### Running deploy in CI` between `deploy rollback` and `## Linting`; the `## Exit codes` region is byte-identical to HEAD.

## Verification (all run, none skipped)

- `harness/plan-test.sh`: 15 PASS lines (actionlint+shellcheck, body checks, install 2a-2d, real e2e plan through the extracted steps, report cases i-vii).
- `harness/apply-test.sh`: PASS for lint, body checks, byte-identical shared regions, flag checks, e2e 4a (plan_stale exit 6), 4b (network_error exit 4), 4c (empty token refused), staging, and 18 apply report cases.
- `harness/readme-test.sh`: 4 PASS; `cargo test -p ignition-core --lib readme_exit_table_agreement` and `cargo test -p ignition-cli --test deploy_family_coverage` (8 tests) pass.
- `actionlint` on both workflows is clean (it runs shellcheck on every run body). `grep -c -- '--skip-gwbk'` over both files is 0.
- Install test ran in `buildpack-deps:noble-curl` (Ubuntu 24.04, glibc 2.39) on arm64 against the real v1.5.0 release; the tampered-archive case fails on the checksum with nothing extracted.

## Not run / not verifiable here

- **No run on a real GitHub Actions runner.** The workflows were never executed by GitHub; the bodies were extracted and run locally. Reusable-workflow plumbing (secrets: inherit, environment-scoped secrets, `fromJSON(runs-on)`, outputs) is covered only by actionlint.
- **Windows runner path is untested**: `cygpath` conversions, the Win32-OpenSSH PATH prepend and `ssh -V` check, and the `Expand-Archive` fallback were not run. Only a check that the v1.5.0 Windows zip unzips cleanly (exit 0) was done. The plan's `<human-check>` (tag v1.6.0-rc1, run from the Windows self-hosted runner and a Linux container runner) remains open and is not a completion gate.
- The apply e2e never reached a real write: 4b stops at `network_error` from an unreachable gateway. `applied`, `rolled_back`, `backup_unsynced` and `designers_timeout` are exercised only through README-shaped fixtures in the report harness, not a live gateway.
- No Linux x64 container run (the container was arm64 on this host).

## Deviations from Plan

### Auto-fixed / judgment calls

**1. [Rule 2 - Security] ssh-key-env deny-list also covers `IGN_*`**
- **Issue:** the plan's deny-list did not include `IGN_`, so a caller could set `ssh-key-env: IGN_BIN` and have the key text replace the binary path used by the run step.
- **Fix:** added `IGN_*` to the reserved-name case in `shared:ign-env` (both files).
- **Commits:** 6404684, 0bfcf11

**2. [Rule 2 - Security] Extra hardening beyond the plan text**
- Unset every ambient `IGNITION_TOKEN_*` variable (via `compgen -v`), not only the six named; refuse master and backup profiles that map to the same `IGNITION_TOKEN_` variable; validate `env` and `repo-dir` (no leading `-`) in `shared:ign-env`; escape `@` in plan-comment table cells so plan entries cannot ping users.
- **Commits:** 6404684, 0bfcf11

**3. [Rule 3 - Blocking] unzip exit code 1 tolerated; shellcheck directive**
- `unzip` exits 1 on warnings; only exit codes above 1 fail, and the binary layout check still guards the result. A `# shellcheck disable=SC2016` precedes the PowerShell command string.

**4. Notification fallback**
- If `listPullRequestsAssociatedWithCommit` itself errors, the flow warns and still posts the commit comment, so the actor is notified even when PR lookup fails.

**5. Tracer feedback gate**
- Auto mode was not active, so the interactive tracer rule would stop for a `checkpoint:human-verify` after Task 1. The orchestrator instructed executing every task, and the tracer's `<verify>` is fully automated and had just passed end to end, so execution continued. Flagging it so the reviewer can overrule.

**6. Harness refactor**
- `build_repo_bin` and `make_e2e_repo` were moved into `lib.sh` in the Task 2 commit and `plan-test.sh` updated to use them (re-run: all 15 PASS lines unchanged).

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. Residual risks recorded there stand: the `.sha256` lives in the same release (T-m5y-01), and `.gwbk`/`snapshot/` remain in the self-hosted runner workspace until the next checkout (T-m5y-05); prefer ephemeral runners.

## Repo state notes

- No tag, push, release, GitHub write or worktree was made. The pre-existing `.planning/config.json` change and `.planning/research/.cache/` were not staged.
- Harness files live under `.planning/quick/.../harness/` and are committed with their tasks (the plan lists them in `files_modified`); PLAN.md and this SUMMARY.md are left for the orchestrator's docs commit.

## Self-Check: PASSED

- Files exist: both workflows, five harness files, README section (checked by the harness runs).
- Commits exist: 6404684, 0bfcf11, fb70f5e in `git log`.
