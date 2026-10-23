---
phase: 25-guarded-project-deploy
plan: 15
subsystem: deploy
tags: [deploy-projects, children, parent-project, settle-wait, read-only, drift-report]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-11 deploy_projects_run, ProjectsRunOptions, ProjectGateway, FakeProjects; 25-09 ReleaseRecord::parse; 25-04 canonical_digest; 25-14 wrong_target and Designer pre-checks; 25-13 ssh release record writer"
provides:
  - "CHILDREN_SETTLE_S (30) and CHILDREN_POLL_S (5), the K7 values, and ProjectsRunOptions.children_settle / children_poll"
  - "ChildStatus (ok, mismatch, unverified) and ChildReport {name, status, release, recorded_sha256, live_sha256}"
  - "data.children on every result: byte-sorted, empty when no sweep ran"
  - "sweep_children: after imported or imported_unverified, reads the direct children's exports until two consecutive reads agree or the bound passes, then compares each with its own release record"
  - "events children_settling and child_checked; the human children: block and child progress lines (names escaped)"
  - "FakeProjects hooks with_child, mutate_child_after_import, fail_export_of"
  - "README: the Children of a parent project paragraph; the stale ssh record sentence corrected"
affects: [25-17, 25-18, 25-19]

actuals:
  tokens: 14055   # chars/4 over `git diff da59950~1..HEAD -- crates README.md` (56222 chars)
  tasks: 2
  commits: 4

tech-stack:
  added: []
  patterns:
    - "a post-outcome sweep that can only warn: no error path out of it, nothing it reads can change outcome, ok, slug or exit code"
    - "wait bounds passed through the options (children_settle, children_poll), never an environment variable; the engine suite uses 1 s and 5 ms"
    - "fake gateway churn is scripted per project and starts only after the first import, so planning and the pre-image are unaffected"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-core/tests/common/fake_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - README.md

key-decisions:
  - "CHILDREN_SETTLE_S is 30, the binding K7 value (six times the 5 s capture C9 measured), not the 10 the plan text derived."
  - "Only direct children are swept (projects whose parent is the deployed project); grandchildren are not. Accepted by the user at the tracer gate."
  - "A child with no release record is unverified without a warning: a child this tool never deployed is the normal case, not drift."
  - "A child export or record that cannot be read is unverified with a warning carrying the error code only; the sweep has no failing path."

patterns-established:
  - "FakeProjects::mutate_child_after_import(name, times): the next `times` exports of a project after the first import each find it edited; applied to the deployed project itself it produces an imported_unverified read-back"

requirements-completed: []  # DPRJ-02 and DPRJ-03 stay open: see Requirement status

duration: "about 1 h 35 min of wall clock from the 25-14 close-out commit (11:36 MDT) to the last commit (13:09 MDT), including the planning reads and the tracer gate wait"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 15: Children sweep after a parent deploy Summary

**After `deploy projects` replaces a parent project, it now reads the parent's direct children, waits (at most 30 s, polling every 5 s) until their exports stop changing, compares each child's canonical digest with the `artifact_sha256` of that child's own release record and reports `ok`, `mismatch` (a warning) or `unverified` in `data.children`; the sweep only reads, and can never fail, roll back or change a deploy.**

## Accomplishments

- **Task 1, tracer (`da59950` RED, `e43f8e5` GREEN, accepted at the human-verify gate: "Accept, continue (Recommended)"; the acceptance includes sweeping direct children only).**
  - `CHILDREN_SETTLE_S` = 30 and `CHILDREN_POLL_S` = 5, `ChildStatus`, `ChildReport`, `DeployProjectsResult.children`, `ProjectsRunOptions.children_settle` / `children_poll` (the CLI passes the two constants).
  - `sweep_children`: lists projects, keeps those whose `parent` is the deployed project (byte-sorted, de-duplicated), reads every child's export, polls until two consecutive reads of every child agree or the bound passes, then for each child reads and parses its release record and classifies it. It runs only after `imported` or `imported_unverified`.
  - Events `children_settling` and `child_checked`; a `children:` block in the human output and child progress lines on stderr; names and releases go through `escape_control`.
  - Two CLI contract cases: `projects_apply_children_ok_and_mismatch` and `projects_apply_children_human`.
- **Task 2 (`8296553`, `e5da03a`).**
  - `FakeProjects::with_child`, `mutate_child_after_import` and `fail_export_of` (module doc names what each stands in for and that none is a write the verb can make).
  - Seven engine cases in `tests/deploy_projects.rs`: `children_settle_waits_for_stable_exports`, `children_settle_is_bounded`, `child_without_record_is_unverified`, `child_read_error_is_unverified`, `sweep_only_reads`, `no_sweep_without_children_or_import`, `unverified_parent_still_sweeps`.
    - `sweep_only_reads` asserts four things at once: the gateway call log after the verify is only `list` and the children's `export:`; the host calls in the run are only `read_release_record` for the two children (a `RecordingHost` double); the only record written is the parent's (a `RecordingWriter` double); and the children's stored members and the existing child record are byte-identical afterwards, with no record created for a child that had none.
    - `no_sweep_without_children_or_import` covers an import with no children, `created`, `recorded`, `skipped` and `--check`, each with a child listed where the outcome allows it, and asserts `children` is empty and no child export was read.
  - README `**Children of a parent project.**` paragraph: when it runs, direct children only, how it waits and the two fixed values, the three statuses and the `null` fields, the human output, "warnings only, never fails, rolls back or writes", the exit code staying with the verdict, and the S5 session residual not claimed.
  - README `**Deploying.**` ssh sentence corrected (see deviation 4).

## Verification

Build environment: `CARGO_TARGET_DIR` in the session scratchpad.

- `cargo test -p ignition-core`: 2095 passed, 0 failed, 55 ignored. `deploy_projects` 50 passed (43 before the plan plus the 7 new cases); the `deploy_transport` source scans, 64 passed.
- `cargo test -p ignition-cli`: 494 passed, 0 failed, 22 ignored. This includes the registry tests under `--bin ign` (63 passed), `contract_deploy_projects` (53 passed, 2 of them new) and `deploy_family_coverage` (9 passed).
- `cargo test -p ignition-tui`: 281 passed, 0 failed.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
- Acceptance: `grep -c 'Children of a parent project' README.md` prints 1; `grep -c 'CHILDREN_SETTLE_S' crates/ignition-core/src/actions/deploy_projects.rs` prints 3; `grep -c 'cannot take the record' README.md` prints 0.
- Mutation check: with the settle bound check forced to true (`if true || elapsed >= opts.children_settle`), `children_settle_waits_for_stable_exports` and `children_settle_is_bounded` failed; the file was restored with `git checkout` and `deploy_projects` ran 50 passed again.
- No live host was contacted. No background work is left running. The new tests use no environment variable for timing.

## Deviations from Plan

1. **[Rule 1 - Bug] `export_before_import_and_verify`'s call log gains a trailing `list`.** The sweep reads the project list after an `imported` outcome even when nothing inherits from the project, so the expected log became `export, import, export, list` (the assertion message says why). The test pins the order of the safety property; only the tail changed. Commit `e43f8e5`.
2. **Each of the two CLI contract tests takes about 5 s.** The binary has no knob for the K7 timing (a constant, not a flag or environment variable, by the plan), so the one poll interval between the first read and the second is real. It costs about 10 s of suite time in total. Commit `da59950` / `e43f8e5`.
3. **Only direct children are swept.** Grandchildren (`WHK-Distillery01-HMI` under `WHK-Controls` under `WHK-Global`) are not looked at when `WHK-Global` deploys. This is the plan's flagged assumption; the user accepted it at the tracer gate. The README says so.
4. **[Rule 1 - Bug] README ssh sentence was false.** The `**Deploying.**` section said an `ssh` node "cannot take the record yet, so a deploy to it is refused (`deploy_tree_invalid`) before anything is imported; `--check` works for it." 25-13 made the ssh writer real. Replaced with a sentence saying an `ssh` node takes the record like a `local` one, written in the deploy account's home so no write right on the data directory is needed. I read the rest of `### deploy projects` (lines 3544 to 3765) and the `**Deploying.**` section for other ssh statements: the `known_hosts` and record-location sentences are correct, none other was stale. Commit `e5da03a`. (25-14's summary carried this note forward.)
5. **The binding K7 value is `CHILDREN_SETTLE_S` = 30, not the plan's derived 10.** `## Contract decisions (25-03)` K7 records 30 (the plan text's own rule, observed 5 s with a floor of 10 s, would give 10) and says later plans must follow the recorded wording. The README and constant use 30.
6. **Tests for Task 2 passed on first run.** Task 1's implementation already held the whole sweep (settle loop, bound, unverified classification, read-only calls), so Task 2 had no RED step. The mutation check above shows the settle and bound cases fail without the loop. Commit `8296553`.
7. **The engine suite's `CHILDREN_SETTLE` is 1 s, not a few hundred ms.** The bounded-settle warning prints the bound in whole seconds (`did not settle within <n> s`), so a sub-second bound would read `0 s`. With 1 s the test asserts the exact production wording shape (`within 1 s`) and costs about 1 s. Poll is 5 ms.
8. **`unverified_parent_still_sweeps` produces its `imported_unverified` through `mutate_child_after_import` on the deployed project itself.** The plan lists three fake hooks and none makes a read-back differ; the churn hook applied to the parent (one edit on the first export after the import, which is the verify read) does it without a fourth hook. The fake's module doc states this use.
9. **A child with no record is `unverified` with no warning**, and a read error is `unverified` with a warning carrying the code only. The plan says "a child whose export or record cannot be read is unverified with a warning carrying the error code only" and "unverified (no release record for it)"; it does not say whether the second case warns. It does not: a child never deployed by this tool is the common case and would drown real drift in warnings.

No auth gates, package installs or checkpoints beyond the tracer gate.

## Requirement status

`requirements-completed` is empty on purpose; DPRJ-02 and DPRJ-03 were not marked complete in REQUIREMENTS.md. `requirements.ready-ids` for this plan returned `0/2 requirement(s) ready`: a requirement is marked by the plan that finishes it, and sibling plans without a SUMMARY still declare both.

- **DPRJ-02** (parent-first ordering, cycles and unknown parents refuse, unchanged projects skipped). This plan finishes its last code part (the K7 child report). Still declared by **25-19** (the live gates, `[DPRJ-01 ... DPRJ-05]`), which proves the whole requirement on a real 8.3.6 rig, so 25-19 will mark it.
- **DPRJ-03** (export before overwrite, verify after by canonical hash). Still declared by **25-17** (secret canary and stdout purity, `[DPRJ-03, DPRJ-04, DPRJ-05]`) and **25-19** (live proof of export-before and verify-after). The last of those, 25-19, will mark it.

## Known Stubs

None. A scan of the changed files for `todo!`, `unimplemented!`, `TODO`, `FIXME`, `#[ignore]` and placeholder text found nothing added. No entry was made in `.planning/WINDOWS.md`.

## Threat Flags

None beyond the plan's register. T-25-47 (writes to children) is mitigated: the sweep's calls are `projects`, the children's `export_bytes` and `read_release_record`, `ProjectGateway` has no write besides the deployed project's import, and `sweep_only_reads` proves the call log, the host calls, the record writes and the children's stored state. T-25-48 (endless settle) is mitigated by the `children_settle` bound, with classification on the last read and a `did not settle` warning (`children_settle_is_bounded`). T-25-49 (child content in output) is mitigated: `ChildReport` carries a name, a status, a release tag and two hashes, and warnings carry the error code only.

## Open items carried forward

- **S5 residual stays open and unclaimed.** C9 shows the child export unchanged and the inherited value updated within 5 s; it does not show what an open Designer or Perspective session of a child sees after a parent overwrite. The README says the sweep reports hash drift only.
- **The CI section of the README** still says projects are not deployed by the reusable workflows; 25-18 owns that.

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/actions/deploy_projects.rs`, `crates/ignition-cli/src/render.rs`, `crates/ignition-cli/src/main.rs`, `crates/ignition-cli/tests/contract_deploy_projects.rs`, `crates/ignition-core/tests/common/fake_projects.rs`, `crates/ignition-core/tests/deploy_projects.rs`, `README.md`: found.
- Commits exist: `da59950`, `e43f8e5`, `8296553`, `e5da03a`: found in `git log`.
- No `todo!`, `unimplemented!`, `#[ignore]` or skipped test added. No file was deleted by any of the four commits.
- `.planning/config.json` and `.planning/research/.cache/` were not staged.
