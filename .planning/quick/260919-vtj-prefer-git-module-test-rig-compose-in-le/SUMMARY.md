---
phase: quick-260919-vtj
plan: 01
subsystem: rig-discovery
tags: [rig, discovery, git-module, docker-compose, tdd]
status: complete
dependency-graph:
  requires: []
  provides: [level-5-test-rig-precedence]
  affects: [ign-rig-status, ign-rig-up, ign-rig-tui]
tech-stack:
  added: []
  patterns: [ordered-const-array-probe-order, chained-iterator-relpath-loop]
key-files:
  created: []
  modified:
    - crates/ignition-core/src/rig/mod.rs
    - README.md
decisions:
  - "D-01: replaced single GIT_MODULE_RELPATH const with ordered GIT_MODULE_RELPATHS array (test-rig first, repo-root second), chained with WHK_GLOBAL_RELPATH via .chain(std::iter::once(...)) in the single level-5 loop — probe order expressed in exactly one place."
  - "D-04: changed the README [rigs.git-module] example compose_file to the test-rig path (not left at repo-root) — leaving it at the repo-root file would contradict the table two lines above and hand a copy-pasting reader the rig this change exists to stop auto-selecting."
metrics:
  completed: 2026-09-19
actuals:
  tasks: 3
  commits: 3
---

# Phase quick-260919-vtj Plan 01: Prefer git-module test rig in level-5 discovery Summary

> **OUTCOME: NOT MERGED — superseded by v1.3 Phase 18.**
>
> This work was completed and verified, opened as PR #7, and then closed
> deliberately. The ordering fix was correct but treated a symptom: level-5
> convention discovery hardcodes repo names and home paths into the shipped
> binary, and this change added a third hardcoded path to that list rather than
> removing the mechanism. RDISC-01/RDISC-02 (Phase 18) replace the convention
> scan with config-declared roots, which makes the ordering question moot.
>
> The commit SHAs cited below (`f3bbea7`, `746893b`, `550c457`) lived only on
> the deleted branch `worktree-git-module-rig-discovery` and are no longer
> reachable. This document is kept for the findings, not the code — in
> particular the upstream defect it uncovered: in `ignition-git-module`,
> `docker/modules/Git-unsigned.modl` is a *directory*, not a file, so the
> repo-root rig bind-mounts a folder onto the gateway's module path and cannot
> boot. That is recorded as out-of-scope in `.planning/REQUIREMENTS.md`.

Level-5 ("WHK conventions") rig discovery now resolves the git-module repo's
`docker/test-rig/docker-compose.yml` before its `docker/docker-compose.yml`,
before falling through to WHK-Global — because the test rig is the only
git-module rig whose gateway can actually load the module.

## Why

`ignition-git-module/docker/docker-compose.yml` mounts the unsigned
`Git-unsigned.modl` into a stock `inductiveautomation/ignition` image with
neither `ACCEPT_MODULE_CERTS` nor `ACCEPT_MODULE_LICENSES` set, so Ignition 8.3
quarantines the module on boot. It also wants a live `GATEWAY_GIT_USER_SECRET`
and commissions against real WhiskeyHouse GitHub repositories. Discovery
auto-selecting that file meant `ign rig up` inside the git-module repo brought
up a gateway that could not load the module under test, against live remotes.

`docker/test-rig/docker-compose.yml` is the working, network-isolated rig: a
built gateway image with developer mode baked in before the first module scan,
both accept-env vars set, and a local `git-server` service.

## What Changed

**Task 1 (tracer, TDD)** — `crates/ignition-core/src/rig/mod.rs`:

- RED test `git_module_test_rig_beats_the_repo_root_rig` written first and
  confirmed failing on pre-fix code (it resolved the repo-root path).
- `const GIT_MODULE_RELPATH: &str` replaced with
  `const GIT_MODULE_RELPATHS: &[&str]` holding the test-rig path first and the
  repo-root path second. Repo-root is kept as a fallback, not dropped —
  checkouts that predate the test rig must still discover something.
- Level-5 loop header rewritten to
  `GIT_MODULE_RELPATHS.iter().copied().chain(std::iter::once(WHK_GLOBAL_RELPATH))`.
  The loop body (root nesting, `trail.push`) is untouched, so outer=relpath /
  inner=root ordering is preserved exactly.
- Module doc `## Discovery order (LOCKED)` level-5 bullet updated to the new
  three-entry order with a short why-clause.

**Task 2 (auto)** — same file, test hygiene:

- Comments on `git_module_convention_probes_both_roots_first_hit_wins` and
  `whk_global_convention_tried_after_git_module` clarified to state the new
  three-entry reality. No fixture or assert changed — these three tests each
  write only `docker/docker-compose.yml`, which after the change is simply the
  second git-module probe, so none of them actually broke.
- Assert message on `git_module_beats_whk_global_when_both_exist` made explicit
  that it covers repo-root-vs-WHK-Global, not test-rig-vs-repo-root (Task 1's
  new test owns that).
- One assertion added to `no_rig_anywhere_errors_with_search_trail` pinning that
  the trail names the new test-rig probe path, keeping the "every probed path
  appears in the trail" contract pinned for the third entry.

**Task 3 (auto)** — `README.md`, exactly 2 changed lines:

- Discovery-table row 5 lists all three convention relpaths in order with a
  why-clause for test-rig precedence.
- The `[rigs.git-module]` example `compose_file` now points at the test-rig path
  (D-04).

## Deviations from Plan

None in the source work. One process deviation: the executor subagent was
blocked at tool level from writing this SUMMARY.md ("Subagents should return
findings as text, not write report files"), so the orchestrator wrote it.

## Verification Gates

Baseline before any change, measured on this worktree:
`cargo test -p ignition-core --lib` → 428 passed, 0 failed (77 `rig::` tests).

Gates re-run by the orchestrator, not only self-reported by the executor:

1. `cargo build --workspace --all-targets` — clean, exit 0.
2. `cargo test -p ignition-core --lib` — **429 passed, 0 failed**;
   `rig::` test count 77 → 78; `rig::tests::git_module_test_rig_beats_the_repo_root_rig`
   confirmed present in the test list.
3. `cargo clippy --workspace --all-targets` — no new warnings, and specifically
   no `dead_code`, which would have meant the old `GIT_MODULE_RELPATH` const was
   left orphaned.
4. `cargo test --workspace` — green, covering the two out-of-scope claims: the
   TUI compose-path fixture at `crates/ignition-tui/src/ui/rig.rs:443` (a
   hardcoded literal for tail-ellipsis fitting, not produced by discovery) and
   `crates/ignition-cli/tests/contract_rig.rs` (points `IGNITION_RIG_ROOTS` at an
   empty temp dir, so an extra probed relpath changes nothing it asserts).

`git diff --stat` across the three task commits: two files — `README.md`
(2 insertions, 2 deletions) and `crates/ignition-core/src/rig/mod.rs`
(82 insertions, 14 deletions).

## Scope Guards Held

`whk_roots()`, `expand_path`, `CWD_CANDIDATES`, `WHK_HOME_ROOTS`, the
nothing-found error string, `crates/ignition-tui/src/ui/rig.rs`, and all
`.planning/` history were left untouched. No new crate dependencies.

## Known-safe interaction

`crates/ignition-core/src/error.rs:1391` parses README.md, but
`parse_readme_exit_table` (~line 1343) scopes itself to the `## Exit codes`
section via `readme.split("## Exit codes").nth(1)`. The discovery table edited
here sits outside that section, so the exit-table agreement test is unaffected —
re-confirmed green in gate 2.

## Commits

| Task | Type | Commit | Message |
|------|------|--------|---------|
| 1 | fix | `f3bbea7` | fix(rig): prefer the git-module test rig in level-5 discovery |
| 2 | test | `746893b` | test(rig): pin the three-entry level-5 probe order |
| 3 | docs | `550c457` | docs(readme): level-5 discovery prefers the git-module test rig |

SHAs are post-rebase. The branch was originally cut from `origin/main` (`728c40d`)
and rebased onto local `main` (`7859247`) before the docs commit, so the executor's
original SHAs (`bfc9e81`, `b48accd`, `607734d`) no longer exist.

## Follow-ups Not Done Here

From the originating evaluation, deliberately left out of scope:

- Rig-side module provisioning (`ign rig up --with-git-module`) — blocked for
  unsigned modules, which need developer mode baked into the image before the
  first module scan. The git-module repo's own `docker/test-rig/deploy.sh`
  already owns this.
- Gateway-module-backed CLI verbs — would compete with the existing
  `ign workspace checkout/status/push` sync story.
