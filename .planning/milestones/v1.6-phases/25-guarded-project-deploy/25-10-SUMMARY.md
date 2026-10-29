---
phase: 25-guarded-project-deploy
plan: 10
subsystem: deploy
tags: [deploy-projects, parent, inheritance, cycle, project_parent_invalid, check]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-04 ProjectMeta.parent and ProjectGateway::projects; 25-07 four-place slug wiring; 25-09 release gates and the order of checks in prepare; 25-03 contract decision K6 a"
provides:
  - "ParentReason (missing, cycle, not_inheritable), ParentLink, parent_chain, check_parent: pure parent decision over one projects/list answer"
  - "CoreError::ProjectParentInvalid / project_parent_invalid (exit 6) in all four places, DEPLOY_SLUGS (14), README exit-6 row"
  - "ProjectRecord.invalid_parent (the gateway's own invalidParent flag from projects/list)"
  - "data.parent in the check result; parent: <name|none> human line through escape_control"
  - "reparent warning when the live project's parent differs from the repo's; warning when the usable parent is itself flagged invalidParent"
  - "README: the Parents paragraph and the parent data key"
affects: [25-11, 25-12, 25-15, 25-19]

actuals:
  tokens: 10755   # chars/4 over `git diff 386be09~1..HEAD -- crates README.md` (43020 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "the parent check reads the one projects/list answer the check already needs; no per-name project_find for the parent"
    - "an acyclic chain cannot visit more projects than the gateway lists, so a walk longer than len + 1 is a cycle (T-25-31)"
    - "a gateway flag the importer ignores (invalidParent) is reported as a warning, while the three reasons the importer never checks are refusals"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/deploy/projects.rs
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/client/projects.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - crates/ignition-core/tests/common/fake_projects.rs
    - README.md

key-decisions:
  - "K6 a: the parent, its inheritable flag and its own parent come from the single projects/list call, so the walk is bounded by steps over that list (len + 1), not by finds."
  - "A parent the gateway lists whose own ancestor is not listed refuses missing (a dangling ancestor), not cycle."
  - "invalidParent: true on the usable parent is a warning, not a refusal (K6 a; the post-import read-back warning belongs to 25-11). Accepted by the user at the tracer gate."
  - "The reparent warning fires whenever the live parent and the repo parent differ, including a repo with no parent over a live project that has one, and never for a project the gateway does not have."

patterns-established:
  - "Human lines for repo-named values (the parent) go through render.rs escape_control"

requirements-completed: []  # DPRJ-02 stays open: the parent-first import itself and the unchanged-project skip on apply land in 25-11 onward, as 25-09 left it

duration: "tracer by the first executor, then a continuation of about 25 minutes"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 10: Parent check Summary

**`deploy projects --check` now refuses a project whose repo-named parent is missing, cyclic (self, chain back to the project, or a chain that never ends) or not inheritable with `project_parent_invalid` (exit 6), deciding everything from one `projects/list` read before the project is exported, and warns when an import would reparent the live project.**

## Accomplishments

- **Task 1, tracer (`386be09`, accepted at the human-verify gate).** `ParentReason`, `ParentLink`, `parent_chain`, `check_parent`; `CoreError::ProjectParentInvalid` wired in the four places plus `DEPLOY_SLUGS` (14) and the README exit-6 row; `ProjectRecord.invalid_parent`; `data.parent`; a warning when the parent's own `invalidParent` is true.
- **Task 2 (`47e004b`).**
  - Reparent warning: "project <name>: importing this project changes its parent from <live|none> to <repo|none>" when the live record's parent differs from the repo's; silent when equal and for a project the gateway does not have (`create`).
  - Human output prints `parent: <name|none>` through `escape_control`.
  - README: the `**Parents.**` paragraph (sources, three reasons, the bound, parent-first is the CI's job, the two warnings) and `parent` in the data key list with its description.
  - Engine tests (below).

## Test results

- `cargo test -p ignition-core -p ignition-cli`: all targets passed, 0 failed (lib 1095 passed; `deploy_projects` engine 27 passed; `contract_deploy_projects` and `deploy_family_coverage` passed; `readme_exit_table_agreement`, `project_parent_invalid_four_places`, `deploy_slugs_are_pinned_and_never_a_verdict` passed). Live gates stay ignored.
- New engine tests in `crates/ignition-core/tests/deploy_projects.rs`: `parent_cycle_self_refuses`, `parent_cycle_chain_refuses`, `parent_not_inheritable_refuses`, `parent_missing_refuses`, `parent_chain_is_bounded` (asserts one list read and zero finds, per K6), `reparent_warns`, `child_check_reads_parent_only` (call log is one list, one find of the child, one export of the child).
- New render test `parent_line_names_the_parent_or_none_and_prints_escaped`.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Acceptance greps: `grep -c 'project_parent_invalid' README.md` prints 2; `grep -c 'project_parent_invalid' crates/ignition-core/src/error.rs` is above 3.
- Built in a private target dir under the session scratchpad; no background work left running.

## Deviations from Plan

### Interface and design changes (Task 1, recorded here)

1. **[Contract K6] `projects/list` replaces the plan's `project_find` walk.** The plan's action and key link describe a `project_find` walk bounded by `projects().len() + 1` finds. The approved K6 a (25-LIVE-CAPTURES.md, `## Contract decisions (25-03)`) says the source is one `projects/list` call and not a per-name `find`; the bound becomes steps over that one list. The `key_links` pattern `check_parent\(` still holds. `parent_chain_is_bounded` asserts `list_count() == 1` and `find_calls()` empty.
2. **[Interface addition] `parent_chain` and `ParentLink.invalid_parent`.** The plan's `ParentLink` had three fields and `check_parent` took a chain built by the action. `parent_chain(parent, all)` is a new pure function (so the walk is testable without a gateway) and `ParentLink` gains `invalid_parent`, as K6 requires `ProjectRecord.invalid_parent`. A dangling ancestor (an ancestor the list does not name) yields `Missing`.
3. **[Contract K6 a] `invalidParent: true` is a warning, not a refusal.** The plan's third truth reads `not_inheritable` as the `inheritable: false` flag; the three refusals stay exactly that. The gateway's own `invalidParent` on a usable parent only warns. The read-back warning after the import belongs to 25-11. The user accepted this at the tracer gate.
4. **README exit-6 row landed in Task 1**, not Task 2, because the four-place rule (`readme_exit_table_agreement`) fails without it. Task 2 added the paragraph and the data key.

### Process

- **Tests first, partly.** Task 1 tests were written before the implementation but no separate red build was observed. In Task 2 the reparent warning and the render line were written, then the engine tests; they went green on first run, so those tests were not seen failing. `parent_chain_is_bounded` and the cycle tests exercise logic that Task 1's unit tests already pin.
- **Extra test:** `parent_missing_refuses` at engine level (not in the plan's list) to keep the third reason covered beside the other two.
- **`requirements mark-complete` not run:** DPRJ-02 also covers the parent-first import and skipping unchanged projects on apply (25-11 onward), as 25-09 left it.
- **Estimate:** the plan estimated 95k tokens on a low-confidence scale; the realized diff is about 10.8k on the chars/4 scale.

## Open points

- 25-11 reads `invalidParent` back after the import and warns on a true value (K6 a).
- The apply must run the same parent check and the same reparent warning; it shares `deploy_projects_prepare`.

## Known Stubs

None.

## Threat Flags

None beyond the register. T-25-31 mitigated (`parent_chain` bound, tests `parent_chain_is_bounded` and `parent_chain_is_bounded_by_the_listed_projects`). T-25-32 mitigated (`child_check_reads_parent_only`; `ProjectGateway` has no write beyond 25-11's own-project import). T-25-33 mitigated (the parent name is only compared against listed names; it is never put in a path or shell argument).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/deploy/projects.rs` (contains `pub fn check_parent`), `crates/ignition-core/src/error.rs` (contains `project_parent_invalid`), `crates/ignition-core/tests/deploy_projects.rs` (contains `fn reparent_warns`), `README.md`.
- Commits found in `git log`: 386be09, 47e004b.
