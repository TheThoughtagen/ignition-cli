---
phase: 25-guarded-project-deploy
plan: 12
subsystem: deploy
tags: [deploy-projects, unverified-verdict, exit-6, restore-command, success-path-exception, readme, coverage-pin]

requires:
  - phase: 25-guarded-project-deploy
    provides: "25-11 write path and the imported_unverified fail-safe (ok false, no record, no second import); 25-03 contract decisions V6 and K11"
provides:
  - "PROJECT_UNVERIFIED_SLUG (deploy_project_unverified) as data.slug beside data.ok false, and data.restore_command after an overwrite"
  - "DeployProjectsResult::failure_exit_code and the sixth sanctioned success-path exit hook in main (exit 6 after the envelope)"
  - "StderrProjectsObserver progress lines prefixed `ign deploy projects:` and the human verdict and restore lines, escaped"
  - "README sixth dated exception (2026-10-23), the Commands row for deploy projects (guarded, Decision C, outcomes, verdict) and the **Deploying.** paragraph"
  - "projects_unverified_verdict_exception_is_dated_and_documented, pinning the slug out of the exit table and out of DEPLOY_SLUGS (15 entries)"
affects: [25-13, 25-14, 25-15, 25-17, 25-18, 25-19]

actuals:
  tokens: 10747   # chars/4 over `git diff 91c9703~1..HEAD -- crates README.md` (42989 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "a verdict is a payload slug beside data.ok false on a success envelope; the process exit is set after the envelope from a failure_exit_code method on the result"
    - "the restore command is data, printed for the operator and never run by the tool (K11)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/deploy_projects.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_deploy_projects.rs
    - crates/ignition-cli/tests/deploy_family_coverage.rs
    - README.md

key-decisions:
  - "deploy_project_unverified is a payload verdict (Decision A), never a CoreError slug and never an exit-table row; a test keeps it out of both."
  - "The restore command is repo-root-relative with no --profile; the human output names the gateway it is for. The user accepted this at the tracer gate."
  - "A failed or unusable read-back also yields the unverified verdict, not an error return."

patterns-established:
  - "Sixth success-path exit exception: the same shape as the rolled-back and backup-unsynced apply verdicts (frozen envelope, then exit 6)"

requirements-completed: []  # DPRJ-03 and DPRJ-05 stay open: see Requirement status

duration: "tracer by the first executor, then a continuation of about 10 minutes"
completed: 2026-10-23
status: complete
---

# Phase 25 Plan 12: Unverified verdict Summary

**A project import whose read-back does not equal the artifact now ends as a success envelope with `data.ok: false`, `data.slug: deploy_project_unverified`, `data.outcome: imported_unverified` and the exact restore command, then exits 6 with no record written and no second import; the README documents it as the sixth dated exception and a test keeps the slug out of the exit table and `DEPLOY_SLUGS`.**

## Accomplishments

- **Task 1, tracer (`91c9703`, accepted at the human-verify gate: "Accept, continue (Recommended)").**
  - `PROJECT_UNVERIFIED_SLUG`, `DeployProjectsResult::failure_exit_code()` (`Some(6)` for `ImportedUnverified`) and `restore_command(project, run_dir)`: `ign project import <name> --file <run_dir>/export/<name>.zip --collision-policy overwrite --yes`, shell-quoted, set only after an overwrite (a created project has no pre-image, so it is `null`).
  - `main.rs`: the exit hook for `ActionOutput::DeployProjects`, commented as the SIXTH sanctioned success-path exit exception (Decision A); `StderrProjectsObserver` in human mode, the no-op observer otherwise.
  - `render.rs`: `StderrProjectsObserver` (`ign deploy projects: <line>`, ids, counts and flags only), the human verdict line and the restore line, both through `escape_control`.
  - Tests: `projects_apply_unverified_exit_6` and `projects_apply_human_output` (wiremock CLI tests), plus render unit tests for the escaped restore command and the progress lines.
- **Task 2 (`15dfa23`).**
  - README: `### The \`deploy projects\` unverified verdict exception (2026-10-23)` under the output contract, after the backup-unsynced exception: envelope shape, outcome and slug, exit 6 after the envelope, no record, no second import, no automatic restore, `restore_command` and its repo-root-relative form, Decision A, and that the slug lives only in `data.slug`.
  - The Commands row for `deploy projects` now says **guarded** (exit 2 `confirmation_required` without `--yes`, the refusal is the plan), Decision C with `--artifact-sha256` (`invalid_input` before contact, `project_hash_mismatch` exit 6), the outcome words including `created` for an absent project (D-13), and the exit-6 verdict.
  - The deploy section's lead is now `**Deploying.**` (the `deploying-a-project` anchor is unchanged); the placeholder saying the verdict "arrives with the next release" was replaced with the published verdict, the progress lines and the `--json` silence of stderr.
  - `projects_unverified_verdict_exception_is_dated_and_documented`: the dated section names `data.ok`, `data.slug`, `deploy_project_unverified`, `imported_unverified`, `exit 6`, `ok: true`, `Decision A`, `restore_command`, `--collision-policy overwrite`, `SIXTH`, no record and no second import; the exit-code section does not contain the slug; `CoreError::DEPLOY_SLUGS` does not contain it and still has 15 entries.

## Notes from Task 1

- **Restore command form.** It is repo-root-relative and carries no `--profile`. The human output names the gateway it is meant for. The user accepted this at the tracer gate; the README states the same.
- **Read-back failure.** A failed or unusable read-back (not only a differing digest) also gives the unverified verdict.
- **Test order.** The Task 1 tests were written after the implementation. A mutation run that disabled the exit hook and one that dropped `restore_command` each made the tests fail, so they test the change.
- **Engine-level gap.** There is no engine-level differing-read-back test, because `FakeProjects` stores the artifact exactly as sent. The wiremock CLI tests (`projects_apply_unverified_exit_6`) cover that case end to end.

## Test results

Build environment: `CARGO_TARGET_DIR` in the session scratchpad.

- `cargo test -p ignition-cli --test contract_deploy_projects --test deploy_family_coverage --test tui_coverage`: 48, 9 and 5 passed.
- `cargo test -p ignition-cli --bin ign` (registry and MCP tests): 63 passed. `cargo test -p ignition-cli --lib` runs 0 tests (no library target tests; the registry tests live in the binary).
- `cargo test -p ignition-core --lib` (includes `readme_exit_table_agreement` and `exit_code_mapping_enumerated`): 1103 passed. `cargo test -p ignition-core --test deploy_projects`: 34 passed.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- Acceptance greps: `grep -c '^## Exit codes' README.md` prints 1; `grep -c "unverified verdict exception" README.md` prints 1; `grep -c '\*\*Deploying\.\*\*' README.md` prints 1; `grep -c 'failure_exit_code' crates/ignition-cli/src/main.rs` prints at least 2.

## Deviations from Plan

1. **[Rule 1 - Bug, docs] Stale README sentence replaced.** The 25-11 Deploying text said the verdict slug, restore command and exit code "arrive with the next release of this verb"; it now describes the published verdict. Commit `15dfa23`.
2. **Deploying lead renamed.** The existing paragraph was headed `**Deploying (without \`--check\`).**`; the plan's acceptance grep expects `**Deploying.**`, so the lead was renamed and the paragraph body kept. The `deploying-a-project` anchor and every link to it are unchanged. Commit `15dfa23`.
3. **Task 1 tests after implementation** (the plan says tests first): see Notes from Task 1. Commit `91c9703`.
4. **Verify-command note.** The plan's `cargo test -p ignition-cli --lib` form runs 0 tests; the registry tests ran under `--bin ign`.
5. **Added to the pin beyond the plan's list:** the `SIXTH` marker, the no-record and no-second-import phrases, and the `DEPLOY_SLUGS.len() == 15` assertion. Commit `15dfa23`.

No other deviations: no auth gates, no package installs, no checkpoints beyond the tracer gate.

## Requirement status

`requirements-completed` is empty on purpose and no requirement was marked complete in REQUIREMENTS.md. A requirement is marked only when the plan finishes it, and neither of this plan's requirements is finished here.

- **DPRJ-03** (export before overwrite, verify after, by canonical hash): this plan publishes the verdict half. A deploy to an ssh node is still refused before any import (25-13 builds the ssh release-record writer), 25-14 adds the Designer and wrong-target preflights, and 25-19 proves it live on the rig.
- **DPRJ-05** (`--check` writes nothing): this plan adds nothing to the check path. 25-17 (canary, stdout purity), 25-18 (CI check mode) and 25-19 (live) still add evidence.
- DPRJ-01, 02 and 04 are not in this plan's frontmatter and stay open.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-25-64 is mitigated (`data.ok: false`, slug, exit 6 after the envelope, same in `result.json`; `projects_apply_unverified_exit_6`). T-25-65 is mitigated (names, hashes and the run path only; the observer formats ids, counts and flags; the escaped-output render tests). T-25-66 is mitigated (a single POST import, asserted; the restore command is printed, never run).

## Self-Check: PASSED

- Files exist: `crates/ignition-core/src/actions/deploy_projects.rs`, `crates/ignition-cli/src/main.rs`, `crates/ignition-cli/src/render.rs`, `crates/ignition-cli/tests/contract_deploy_projects.rs`, `crates/ignition-cli/tests/deploy_family_coverage.rs`, `README.md`: found.
- Commits exist: `91c9703`, `15dfa23`: found in `git log`.
- No `todo!`, `unimplemented!`, `#[ignore]` or skipped test added by this plan.
- `.planning/config.json` and `.planning/research/.cache/` were not staged. No background work is left running.
