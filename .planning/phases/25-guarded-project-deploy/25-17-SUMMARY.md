---
phase: 25-guarded-project-deploy
plan: 17
subsystem: deploy
tags: [deploy, projects, canary, stdout-purity, cutover-runbook, readme, dsurf-03, sc-5]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-11 to 25-15 apply, verdict, pre-checks and children sweep of `deploy projects`; 25-16 `--targets`; the C10 verdicts and K8 a in 25-LIVE-CAPTURES.md (`## C10`, `## Host grant and prod git.yaml (25-03)`, `## Contract decisions (25-03)`)"
provides:
  - "`PROJECTS_CANARY_EXEMPT = [\"export\"]` in `actions::deploy_projects`: the one name under which a run may hold project content (the 0600 pre-image `export/<project>.zip`)"
  - "CLI canary suite: `projects_canary_apply_json_and_human`, `projects_canary_check_and_verdict`, `projects_canary_refusals` (project_hash_mismatch, plan_stale, deploy_preflight_failed twice, import_denied) in `tests/contract_deploy_projects.rs`"
  - "core canary suite: `canary_refusals_clean`, `canary_aborted_runs_clean`, `canary_children_clean` in `tests/deploy_projects.rs`"
  - "`projects_stdout_one_envelope` in `tests/contract_stdout_purity.rs`: `--check --json` and a confirmed `--json` apply print exactly one envelope under maximum diagnostics"
  - "README `#### Cutting a project over from the git module`, `#### What a project repo needs`, `#### Open question: images and themes`"
affects: [25-18, 25-19]

actuals:
  tokens: 14988   # chars/4 over `git diff b130167~1..HEAD -- crates README.md` (59952 chars)
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "canary scan that decodes zip members as well as raw bytes, so a deflated copy of the canary cannot hide in a run file; the walk must find the four audit files and the exempt file must exist, hold the canary and be 0600 in a 0700 directory, so the scan is never vacuous"
    - "`run_observed` in the engine suite: `run_with_policy` delegates to it so a test can hear every event a run reports"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-core/tests/deploy_projects.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - README.md

key-decisions:
  - "The canary exemption is the whole `export` subtree, named by one constant, and every test asserts the exempt file is the only one and that it holds the canary."
  - "The runbook follows K8 a (module stays installed, per-project `.git` removal by an administrator) and restates the owner's long-term-branch question as OPEN without answering it."
  - "No requirement is marked complete (see Requirement status)."

patterns-established:
  - "`DenyImport(&FakeProjects)` wrapper gateway in the engine suite: forwards reads and answers the import with the gateway's own denial"

requirements-completed: []  # DPRJ-03, DPRJ-04, DPRJ-05 stay open: 25-19 (and 25-18 for DPRJ-05) still list them

duration: "about 2h15m of wall clock: the tracer commit b130167 at 14:09 MDT, Task 2 at 16:23, Task 3 at 16:25 (this continuation began after the tracer gate)"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 17: Secret canary, stdout purity and the cutover runbook Summary

**A canary string in the checkout and in the live project never leaves `export/<project>.zip` through any success, check, verdict, refusal or aborted run of `ign deploy projects`, in JSON and in human mode; and the README now carries the per-project cutover runbook, the project repo checklist and the open images and themes question.**

## Accomplishments

- **Task 1, tracer (`b130167`, accepted at the human-verify gate: "Accept, continue (Recommended)").** `PROJECTS_CANARY_EXEMPT`, `projects_canary_apply_json_and_human`, `projects_canary_check_and_verdict` and `projects_stdout_one_envelope`. The canary sits in the checkout script and the live export; stdout, stderr, every run file and the release record are scanned (raw bytes and zip members).
- **Task 2 (`d5eb464`).**
  - Engine, `tests/deploy_projects.rs`:
    - `canary_refusals_clean`: `project_hash_mismatch`, `project_git_managed`, `project_parent_invalid`, `destructive_change_refused` (gates `downgrade` and `first_deploy`) and `deploy_preflight_failed` (`designers`, `wrong_target`). The error's display, debug form, hint, endpoint and code are canary-free, no `.ign/` exists, nothing was imported, and the gateway fake was not asked anything for the git refusal.
    - `canary_aborted_runs_clean`: `plan_stale` (another administrator moves the live project between plan and run, K15) and `import_denied` (a `DenyImport` wrapper). `run.json`, `plan.json`, `events.ndjson`, `result.json`, the observer events and the error are canary-free; only `export/CapApp.zip` holds it, it is the only exempt file and it is 0600 in 0700 directories; `result.json` carries the slug and no record is written.
    - `canary_children_clean`: a mismatching child, a child with no record and a child whose export fails (all three hold the canary): the serialized result, the children report, the warnings, the events and the run files are canary-free.
  - CLI, `tests/contract_deploy_projects.rs`: `projects_canary_refusals` drives `project_hash_mismatch` (no request sent), `plan_stale` (an export responder that moves after the first read), `deploy_preflight_failed` for an open Designer and for a wrong target, and `import_denied` (a 200 answer with `success: false`) through the binary; stream and run-file scans as in the tracer, run directory and pre-image asserted where one is expected, release record byte-identical afterwards.
- **Task 3 (`fd30aed`).** Three README sections at the end of `### deploy projects`:
  - `#### Cutting a project over from the git module`: built from the C10 verdicts (`api-import-reverted: no`, `import-removes-git: yes`, `git-recreated: no`) and K8 a. Seven numbered steps per project: release tag (the module pulls `v*`), deploy account grant and one account per gateway (the record lives in its home), administrator removes `.git`, `--check` expecting `action: record` (and what `--allow-first-change` is and is not for), apply with the reviewed `artifact_sha256` expecting `recorded`, disable the module's tag pull for the project (spec section 13 step 8), later releases. States that the verb never deletes `.git`, lists what C10 did not exercise, and names K8 b as the fallback for a gateway outside the exercised keys. Restates the owner's question verbatim, marked OPEN, unanswered: "We won't want this to be on a stray production branch forever. May need to think this through."
  - `#### What a project repo needs`: `project.json`, `deploy/project.yaml`, `.ign/` in `.gitignore` (refuses `deploy_preflight_failed` otherwise), `deploy/** -text` in `.gitattributes`, optional `.ignexclude`, and that `Manifest.projects` pins are not read (D-04).
  - `#### Open question: images and themes`: gateway resources, not project content; nothing in this tool delivers WHK-Global's `images/` and `themes/` after cutover; recorded as open without an answer.
  - The older sentence "the full runbook arrives with the cutover documentation of this phase" now links to the runbook.

## Verification

Build environment: `CARGO_TARGET_DIR` in the session scratchpad. No worktree, no live host, nothing left running.

- `cargo test -p ignition-core` (all suites, including `deploy_projects` 53 passed, the lib 1109, the `deploy_transport` source scans) and `cargo test -p ignition-cli` (including `--bin ign` registry tests, `contract_deploy_projects` 56, `contract_stdout_purity` 14, `deploy_family_coverage` 9; 506 passed, 0 failed, 22 ignored) and `cargo test -p ignition-tui` (281 passed): all green.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.
- `readme_exit_table_agreement` passes; `grep -c '^## Exit codes' README.md` prints 1; the two plan greps each print 1.
- Mutation check for the scans: changing `PROJECTS_CANARY_EXEMPT` to `["exportx"]` made `canary_aborted_runs_clean` and `canary_children_clean` fail with `canary in ["export/CapApp.zip"]`, so the exemption is the only thing hiding the pre-image and the scan reads it. The change was reverted (`git checkout`) before the commit.
- Task 1's own mutation (moving the export to `exportx/`, and a deflated copy needing the zip-member decode) is recorded in the deviations below.

## Deviations from Plan

1. **Task 1: no RED phase.** The tracer is `tdd="true"` but there was no leak to catch, so no test could fail first. To prove the tests can fail, a mutation that moved the export to `exportx/` failed both canary tests, and the zip-member decode was needed to catch a deflated copy of the canary. (Commit `b130167`.)
2. **Task 1: the canary sits at `ignition/script-python/cap/code.py`, not `a/code.py`,** because the existing `CapApp` fixture was reused. (`b130167`.)
3. **Refusal cases moved to Task 2.** The tracer's behaviours did not include the four refusals (`project_hash_mismatch`, `plan_stale`, `deploy_preflight_failed`, `import_denied`) at the CLI level; `projects_canary_refusals` covers them in Task 2, and `crates/ignition-cli/tests/contract_deploy_projects.rs` was added to Task 2's file list, which the plan names only for Task 1. (`d5eb464`.)
4. **Task 1: the purity fixture in `contract_stdout_purity.rs` is duplicated,** because the suite helpers (`fixture`, `mount_deployable` and the others) are private to their binary and cannot be imported. (`b130167`.)
5. **Task 2, no source change.** The plan allowed fixes in `deploy_projects.rs` for any leak the tests found; none was found, so Task 2 changed tests only. `run_with_policy` in the engine suite now delegates to a new `run_observed` (so a test can hear events); no existing assertion changed.
6. **Task 2, no RED phase either.** The canary tests passed on first run for the same reason as the tracer (the behaviour was already correct); the exempt-name mutation in Verification stands in for a RED run.
7. **README: the older `--targets` and Git-managed paragraphs are unchanged except one sentence,** which now links to the runbook instead of saying it is to come.

### TDD Gate Compliance

Plan type is `execute`, so the plan-level RED/GREEN gate does not apply. Tasks 1 and 2 are `tdd="true"` but add tests over behaviour that already holds; there is no `feat(...)` commit for either because nothing needed implementing beyond the `PROJECTS_CANARY_EXEMPT` constant. Warning recorded here rather than faked.

## Requirement status

`requirements-completed` is empty on purpose. This plan proves the DSURF-03 and SC-5 canary on every surface and writes the cutover documentation, but:

- **DPRJ-03** (exported before overwrite, verified after by canonical hash) is also listed by **25-19** (`[DPRJ-01 .. DPRJ-05]`, the live gates on a real 8.3.6 rig). 25-19 proves it against a real gateway; no earlier plan marked it.
- **DPRJ-04** (unmanaged projects reported and never deleted; a git-managed project refused) is also listed by **25-19**. The runbook and the refusal are in place, but the live `.git` refusal on the capture hosts is 25-19's gate.
- **DPRJ-05** (`--check` reports what would change with no writes) is listed by **25-18** (`[DPRJ-01, DPRJ-05]`, the reusable workflow that runs the check in CI) and by **25-19**.

A requirement is marked by the plan that finishes it, and 25-19 finishes all five, so it will mark them. REQUIREMENTS.md still shows DPRJ-01..05 pending, consistent with every earlier Phase 25 plan.

## Known Stubs

None. A scan of the changed files for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and `.skip` found nothing added. No entry was made in `.planning/WINDOWS.md`.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: information_disclosure (low) | crates/ignition-core/src/error.rs (`ImportDenied`) | `import_denied` prints the gateway's own `problem` text verbatim, and its hint repeats it. The canary tests answer with a fixed problem, so they pass; a gateway that quoted project content in its own refusal would carry it to stderr. This is existing behaviour of every import verb, not new surface of this plan, and not something the canary can cover because the gateway chooses the text. Not changed here. |

The plan's register is otherwise mitigated: T-25-52 (outputs and run files) by the canary suites over success, check, verdict, refusal and aborted paths with the one exempt file asserted 0600; T-25-53 (two deployers for one tag) by runbook step 6; T-25-54 (silent change at cutover) by runbook step 4 requiring `action: record` before the first apply.

## Open items carried forward

- **OPEN (owner), long-term branch lifecycle of module-managed projects:** "We won't want this to be on a stray production branch forever. May need to think this through." Restated in the runbook, unanswered.
- **OPEN (owner), images and themes after WHK-Global's cutover (research F6 / Open Question 5):** nothing delivers them.
- **25-18** should link the runbook from the CI section it writes. **25-19** should include a `record` cutover run against the live rig if the phase gate wants step 4 and 5 of the runbook proven there.
- The Designer-side pull, save and hotfix paths of the git module, `tags_importOnStartup` and the real production `git.yaml` remain unexercised (C10); the runbook says so.

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/actions/deploy_projects.rs`, `crates/ignition-core/tests/deploy_projects.rs`, `crates/ignition-cli/tests/contract_deploy_projects.rs`, `crates/ignition-cli/tests/contract_stdout_purity.rs`, `README.md`: found.
- Commits exist: `b130167`, `d5eb464`, `fd30aed`: found in `git log`.
- `grep -c 'PROJECTS_CANARY_EXEMPT' crates/ignition-core/src/actions/deploy_projects.rs` prints at least 1; the three tests of each canary suite exist and pass.
- No file was deleted by any of the three commits. `.planning/config.json` and `.planning/research/.cache/` were not staged.
