---
phase: 23-guarded-apply-rollback
plan: 24
subsystem: ci-evidence
tags: [ci, windows-latest, github-actions, g2, conformance, linux-regression]
gap_closure: true
gap_ids: [G2]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: marker-led sftp listings (23-22), survey_root and the honest mkdir text (23-23)
provides:
  - a green pull_request CI run on phase-23-ci with windows-latest running all 47 deploy_host_conformance tests
  - the Linux regression line for survey_root on ign-lnx-836
  - the `## Windows CI (windows-latest)` round table in 23-LIVE-CAPTURES.md
affects: [phase 23 verification, PR #21]

actuals:
  tokens: 6000
  tasks: 2
  commits: 4

key-files:
  created:
    - .planning/phases/23-guarded-apply-rollback/23-24-SUMMARY.md
  modified:
    - .planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md
    - .planning/phases/23-guarded-apply-rollback/23-UAT.md

key-decisions:
  - "no fix round was needed: 23-22 plus 23-23 were sufficient on the real windows-latest runner"
  - "the one ubuntu failure (contract_mcp::mcp_tools_call_write_verbs_unknown, 15 s starvation timeout) was a recorded load-sensitive flake: one gh run rerun --failed, then green, per routing (b)"

requirements-completed: []

duration: ~1h50m
completed: 2026-10-19
---

# Phase 23 Plan 24: windows-latest CI proof (G2) Summary

**The draft PR #21 run on `phase-23-ci` is green on ubuntu, macos, windows-latest and keyring-smoke, and windows-latest runs all 47 `deploy_host_conformance` tests ok (every `conformance_ps_*` row and `ps_*` test), with no further code change after 23-22 and 23-23.**

## Status: COMPLETE

Every must-have is met (details below). `requirements-completed` stays empty (human_verify_mode end-of-phase), as in 23-23.

## Step 0: Linux regression gate (skipped by 23-23)

`lnxvm_apply_script_run` against ign-lnx-836 (rg-ign-cli-linux-test), HEAD 08d12b0, 2026-10-19T06:54:26Z: PASS, 306.10 s. Same commit report and restore report as the 23-22 run on the previous tree; the tampered work list and script are refused; scratch and staging gone. The VM was deallocated by its 0300 UTC auto-shutdown and was started with `az vm start` (06:52:48Z, running 06:53:31Z); the public IP (20.127.14.223) was unchanged, so `~/.cache/ign-lnxvm01/host` and `known_hosts` were not touched. It is left running. Recorded under 23-LIVE-CAPTURES.md `## G2: 8.3 short names (windows-latest)`, commit 9d068e5.

## Task 1 (tracer): local gate, push, round 1

Local gate under rustup stable (updated to rustc/cargo 1.99.1), HEAD 9d068e5, every step exit 0:

| Step | Result |
|---|---|
| style-token grep (ci.yml recipe) | 0 |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| `cargo build --workspace` | 0 |
| `cargo test --workspace` | 0 |
| `cargo build -p ignition-cli --no-default-features` | 0 |
| `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` | 0 |

Push: `git fetch origin phase-23-ci`, `git merge-base --is-ancestor origin/phase-23-ci HEAD` exit 0, `git push origin HEAD:refs/heads/phase-23-ci` (9062706..9d068e5, fast-forward). origin `main` stayed `ab8608a3d64229eb12cd25bcd5092cd76c23b94b`.

## CI rounds

| Round | Head | Run | check (ubuntu) | check (macos) | check (windows) | keyring-smoke |
|---|---|---|---|---|---|---|
| 1, attempt 1 | 9d068e5 | [39219142462](https://github.com/TheThoughtagen/ignition-cli/actions/runs/39219142462) | failure | cancelled (fail-fast) | cancelled (fail-fast) | success |
| 1, attempt 2 (`gh run rerun --failed`) | 9d068e5 | 39219142462 | success | success | success | success |
| 2 (docs-only head) | ce4aec0 | [39220703311](https://github.com/TheThoughtagen/ignition-cli/actions/runs/39220703311) | success | success | success | success |

Attempt 1 ubuntu failure: `contract_mcp::mcp_tools_call_write_verbs_unknown` (contract_mcp.rs:217) "no response with id 32 arrived within 15s - protocol death or starvation"; 11 passed, 1 failed. It is an MCP stdio-harness timing test unrelated to deploy, sftp or the listing parser, and the brief names it as a known load-sensitive flake (previously seen on macOS; this is its first recorded ubuntu failure). One rerun, then green; not rerun again. The matrix fail-fast cancelled macos and windows in attempt 1, so the Windows `deploy_host_conformance` result is from attempt 2.

Windows `deploy_host_conformance`:
- run 39219142462 attempt 2 (job 117951482193): `test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 31.19s`
- run 39220703311 (job 117955062250): `test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 25.34s`
- on 9062706 (run 39175065312) the same file failed 18 of 47.

No `conformance_ps_` or `ps_` test reported FAILED. All of this is in 23-LIVE-CAPTURES.md `## Windows CI (windows-latest)`, closed with the G2 line.

## Task 2: fix rounds

None needed (0 of 3 used). No source, test or workflow file was changed by this plan. `git diff 9062706 -- .github/workflows/ci.yml` is empty. PR #21 is still an open draft; it was not marked ready or merged.

## Commits

- `9d068e5` docs(23-24): record the Linux apply gate on the survey_root tree, PASS on ign-lnx-836
- `8236be0` docs(23-24): record CI round 1 on phase-23-ci, windows-latest conformance green
- `ce4aec0` docs(23-24): close G2 on the green pull_request run, windows-latest conformance 47 of 47
- the final docs commit (this summary, 23-UAT.md, STATE, ROADMAP) is local only; it is not pushed (see below)

## Deviations from Plan

**1. [Instruction] The last docs push was not preceded by a full local gate.** `ce4aec0` differs from the gated 9d068e5 only by `.planning/**/23-LIVE-CAPTURES.md` text, so the gate result stands for the code; the run on ce4aec0 was green on all four jobs.

**2. [Scope] The final summary/state commit is not pushed.** Pushing it would start another run for another docs-only head, so the branch tip is `ce4aec0` (run 39220703311, all green) and local `main` is one docs commit ahead of `phase-23-ci`. The plan's automated check that the newest run is for local HEAD will therefore compare against ce4aec0, not the final docs commit. Push of the last commit, if wanted, is a one-line fast-forward.

### Authentication gates

None.

## Updated outside the plan's files

- `23-UAT.md`: test 5 `pass`, summary passed 5 / issues 1, G2 gap `status: resolved`. Test 2 (G1, FA-23-05) was left as recorded by 23-22.

## Noticed, not changed

- `~/.config/ign-azure-win/env` still has a stale `IGN_WINVM_HOST` (from 23-23).
- `.planning/config.json` and `.planning/research/.cache/` were left uncommitted as instructed.

## Known Stubs

None.

## Threat Flags

None. One ref (`phase-23-ci`) was pushed twice, both fast-forwards guarded by `merge-base --is-ancestor`; no force, tag or `main` push (T-23-94). No assertion was relaxed and ci.yml is unchanged (T-23-95). Only result lines and test names were copied from CI logs (T-23-96). Zero fix rounds (T-23-97).

## Self-Check: PASSED

Commits 9d068e5, 8236be0, ce4aec0 exist; `git ls-remote origin refs/heads/phase-23-ci` names ce4aec0; `refs/heads/main` unchanged at ab8608a; `## Windows CI (windows-latest)` present once in 23-LIVE-CAPTURES.md.
