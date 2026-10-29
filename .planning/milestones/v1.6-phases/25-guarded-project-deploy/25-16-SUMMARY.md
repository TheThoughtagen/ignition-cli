---
phase: 25-guarded-project-deploy
plan: 16
subsystem: deploy
tags: [deploy, targets, cli-flag, d-07, d-10, d-14, known-hosts, whk-global]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04 and 25-08 `deploy projects --targets` and the targets-root known_hosts resolution; D-14 in 25-CONTEXT.md and V8 `a (decided: D-14)` in 25-LIVE-CAPTURES.md"
provides:
  - "`--targets DIR` on `ign deploy render`, `plan`, `pull`, `verify`, `apply` and `rollback` (default `--repo`)"
  - "core entry points `load_declared_in`, `deploy_render_in`, `deploy_pull_preview_in`, `deploy_pull_in`, `deploy_apply_run_in`, `deploy_rollback_prepare_in`, `deploy_rollback_prepare_for_env_in`, `deploy_rollback_run_in`; every old name delegates with `targets = repo`"
  - "flag pins for the six leaves; contract tests for the split layout (gateway repo with no `targets/`, separate targets checkout)"
  - "README: `[--targets DIR]` in the six Commands rows and six synopses, and one paragraph in the deploy section defining the flag for the gateway-config verbs"
affects: [25-17, 25-18, 25-19]

actuals:
  tokens: 16548   # chars/4 over `git diff 37ac989~1..HEAD -- crates README.md` (66193 chars)
  tasks: 2
  commits: 5

tech-stack:
  added: []
  patterns:
    - "additive `_in` entry points with a targets root; the old function delegates with `targets = repo`, so about 150 existing core call sites compile unchanged"
    - "one `targets_root = targets.unwrap_or_else(|| repo.clone())` per CLI arm, passed to target loading, to `for_node` (master and backup read halves) and to the apply and rollback entries"
    - "the run directory (`.ign/`), the gateway tree, `render --out` and the pull destination keep using `repo`; only the target file and what it names use the targets root"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - README.md

key-decisions:
  - "Additive `_in` entry points instead of a new `ApplyRunOptions` field, so no existing core test changes."
  - "Two entry points (`deploy_apply_run_in`, `deploy_rollback_run_in`) take eight arguments and carry `#[allow(clippy::too_many_arguments)]`; wrapping the targets root into a struct would have touched the callers the plan asks to leave alone."

patterns-established:
  - "`split_into_targets(tracer)` in the CLI contract suite: moves `targets/` and the data directory out of the gateway repo into a separate checkout and rewrites `data_dir` to the relative `data`, which only resolves if the host is built against the targets root"

requirements-completed: []  # DPRJ-01 stays open: see Requirement status

duration: "about 40 min of wall clock from the 25-15 close-out (13:09 MDT) to the last code commit (13:48 MDT), including the tracer gate wait"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 16: `--targets DIR` on every deploy leaf Summary

**`ign deploy render`, `plan`, `pull`, `verify`, `apply` and `rollback` now take `--targets DIR`: the target file, its relative `known_hosts` and its relative `local.data_dir` resolve against DIR, while the gateway tree, `render --out`, the pulled tree and the `.ign/` run directory stay under `--repo`; without the flag nothing changes.**

## Accomplishments

- **Task 1, tracer (`37ac989` RED, `1ab2f20` GREEN, accepted at the human-verify gate: "Accept, continue (Recommended)").**
  - `load_declared_in(repo, targets, env)`; `load_declared` delegates with `targets = repo`.
  - `--targets` on `deploy plan`, with the master reader built against the targets root.
  - Tests `plan_reads_targets_dir`, `plan_without_targets_unchanged`, `plan_flags_include_targets`, and the `split_tracer` and `deploy_leaf_flags` helpers.
- **Task 2 (`95e4f34` tests, `b691f03` code, `941ebe1` README).**
  - Core: `deploy_render_in`, `deploy_pull_preview_in`, `deploy_pull_in` (target read from the targets root; the existing tree and the pulled files stay under `repo`). `deploy_apply_run_in`, `deploy_rollback_prepare_in`, `deploy_rollback_prepare_for_env_in`, `deploy_rollback_run_in` (target loaded and `apply_host_for_node` built against the targets root; the `repo` argument still names the run directory).
  - CLI: `targets: Option<PathBuf>` on `Render`, `Pull`, `Verify`, `Apply`, `Rollback` with the doc from the plan; each arm computes `targets_root` once. In `verify` and `apply` both the master and the backup read halves (`for_node`) use it.
  - Tests in `contract_deploy.rs`: `render_reads_targets_dir`, `pull_reads_targets_dir`, `verify_reads_targets_dir`, `apply_and_rollback_read_targets_dir`, `render_pull_verify_rollback_flags_include_targets`, and `targets` added to `apply_flags_are_exactly_these`.
    - Each positive case also shows the negative: the same command without `--targets` on a repo with no `targets/` refuses `deploy_tree_invalid` (exit 2), and for apply the refusal sends no request to the gateway and writes nothing to the host.
    - The apply case asserts the run directory is under `<repo>/.ign/` and that no `.ign` exists under the targets checkout; the rollback case deletes `.ign/`, restores from the on-box record with `--targets`, and checks the configuration tree equals its pre-apply listing.
    - The pull case asserts nothing lands under the targets checkout and that the replace guard still looks at `--repo` (a second pull needs `--yes`).
  - README: `[--targets DIR]` in the six Commands rows and the six section synopses (14 lines now carry the string; the plan asked for at least 7). One paragraph before `### deploy render` defines the flag for the gateway-config verbs: D-07 (targets and reviewed known_hosts live only in `WhiskeyHouse/whk-onprem-infra-iac`, folder `ignition/`), what resolves against DIR, what stays under `--repo`, and D-10 (WHK-Global runs `deploy apply` and `deploy projects` as separate jobs).

## Verification

Build environment: `CARGO_TARGET_DIR` in the session scratchpad. No worktree was used.

- `cargo test -p ignition-core -p ignition-cli -p ignition-tui --no-fail-fast`: 93 test binaries, **2878 passed, 0 failed, 77 ignored** (the ignored ones are the existing live and gated cases). This includes the registry tests under `--bin ign` (63 passed), `contract_deploy` (all cases, including the 8 `targets` cases), `contract_deploy_projects`, `deploy_family_coverage` and the core suites `deploy_render`, `deploy_plan`, `deploy_pull`, `deploy_verify`, `deploy_apply` and the `deploy_transport` source scans, all unchanged.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
- RED check for Task 2: the new tests were written after the code in one pass, so I saved the source diff, restored the four source files to `1ab2f20`, ran `cargo test -p ignition-cli --test contract_deploy targets`, and re-applied the diff. Five of the eight cases failed (`unexpected argument '--targets' found`); the three plan cases from Task 1 passed, as they should. Then the commits were made in the order test, code, docs.
- Acceptance: `grep -c '\[--targets DIR\]' README.md` prints 14.
- No live host was contacted. No background work is left running.

## Deviations from Plan

1. **Test order within Task 2.** The tests were written after the implementation and the RED state was reproduced by reverting the source (see Verification); the commits are in the order `test`, `feat`, `docs`. Commits `95e4f34`, `b691f03`.
2. **Test grouping differs from the plan's four behaviour names.** `render_pull_verify_read_targets_dir` became three tests (`render_reads_targets_dir`, `pull_reads_targets_dir`, `verify_reads_targets_dir`) because verify needs a mock gateway and the others do not. `apply_reads_targets_dir` and `rollback_reads_targets_dir` are one test, `apply_and_rollback_read_targets_dir`, because the rollback needs the run the apply made. The four new flag pins (render, pull, verify, rollback) are one test, `render_pull_verify_rollback_flags_include_targets`; the apply pin was extended in place. Same assertions, fewer fixtures.
3. **`#[allow(clippy::too_many_arguments)]` on two functions** (`deploy_apply_run_in`, `deploy_rollback_run_in`, eight arguments each). The existing `apply_ops.rs` uses the same attribute twice.
4. **No new known_hosts test for the gateway-config verbs.** The resolution of a relative `known_hosts` and a path that leaves the root is in `node_access`, which the six verbs now reach with the targets root. 25-08's `project_host_known_hosts_from_targets_root` and the README sentence cover it; a gateway-config ssh case would need a live host. The data-dir case (`local.data_dir: data` resolving under DIR) is proven by the tracer and the apply test.
5. **README: `--targets` is defined in two places.** The new paragraph in the deploy section defines it for the six gateway-config verbs and names `projects`; the existing `**--targets**` paragraph in `### deploy projects` (25-04, 25-08) is unchanged. They agree (default `--repo`, known_hosts resolves against DIR, escapes refused). I did not delete the older one because it also documents the `deploy_tree_invalid` refusal wording for projects.

No auth gates, package installs or checkpoints beyond the tracer gate.

## Requirement status

`requirements-completed` is empty on purpose: DPRJ-01 (`ign deploy projects` verifies every project artifact against its manifest sha256 pin before any upload) was **not** marked complete in REQUIREMENTS.md. This plan implements D-14, which is a contract decision around `deploy projects` (the gateway-config verbs reading targets from the IaC checkout), not the hash-pin gate itself. Sibling plans that have no SUMMARY yet still declare DPRJ-01: **25-18** (`[DPRJ-01, DPRJ-05]`, the reusable `deploy-projects.yml` that runs the pinned deploy in CI) and **25-19** (`[DPRJ-01 .. DPRJ-05]`, the live gates that prove the requirement on a real 8.3.6 rig). A requirement is marked by the plan that finishes it, so 25-19 will mark it.

## Known Stubs

None. A scan of the changed files for `todo!`, `unimplemented!`, `TODO`, `FIXME`, `#[ignore]` and placeholder text found nothing added. No entry was made in `.planning/WINDOWS.md`.

## Threat Flags

None beyond the plan's register. T-25-50 (host key file from an unexpected place) is mitigated: every host build in the six verbs goes through `for_node` / `apply_host_for_node` with the targets root, and `known_hosts` resolves only under that root (`known_hosts_problem` refuses `..` and absolute paths when the target is read). T-25-51 (regression in existing deploy verbs) is mitigated: the old entry points delegate with `targets = repo`, and every existing deploy suite ran unchanged and green (2878 passed). The negative case in each new test shows a repo with no `targets/` still refuses without the flag.

## Open items carried forward

- **25-18** must pass `--targets` in `deploy-plan.yml` and `deploy-apply.yml` (the `targets-dir` inputs in the phase inventory) and update the README CI section.
- **25-19** should include a gateway-config verb run with `--targets` against the live rig if the phase gate wants the ssh known_hosts path proven there; the flag itself is covered offline.

## Self-Check: PASSED

- Files exist: `crates/ignition-cli/src/cli.rs`, `crates/ignition-cli/src/main.rs`, `crates/ignition-core/src/actions/deploy.rs`, `crates/ignition-core/src/actions/deploy_apply.rs`, `crates/ignition-cli/tests/contract_deploy.rs`, `README.md`: found.
- Commits exist: `37ac989`, `1ab2f20`, `95e4f34`, `b691f03`, `941ebe1`: found in `git log`.
- No `todo!`, `unimplemented!`, `#[ignore]` or skipped test added. No file was deleted by any of the five commits.
- `.planning/config.json` and `.planning/research/.cache/` were not staged.
