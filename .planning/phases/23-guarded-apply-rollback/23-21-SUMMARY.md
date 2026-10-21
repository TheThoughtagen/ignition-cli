---
phase: 23-guarded-apply-rollback
plan: 21
subsystem: deploy
tags: [readme, verdict-exception, gate-exits, deploy-apply, decision-a, decision-c, contract-tests, rust, tdd]
status: complete

requires:
  - phase: 23-guarded-apply-rollback
    provides: 23-13 the deploy apply verb, mount_apply_gateway and the apply_* helpers; 23-02 gates and slugs; 23-07 preflights; 23-09 gwbk and restart fields; 23-10 the 176 s hold; 23-08/23-20 the Windows host
provides:
  - README `### deploy apply` section (gate order, Decision C, the seven preflights, gwbk, designers, run directory, never-pruned runs, restart reporting, Windows and ssh-drop notes, exit codes)
  - README `### The deploy apply rolled-back verdict exception (2026-10-18)` under the output contract
  - rolled_back_verdict_exception_is_dated_and_documented, with a shared dated_exception_body helper
  - apply_stale_hash_exit_6, apply_destructive_without_flag_exit_2, apply_shadows_removal_exit_6, apply_drift_with_reason_records_it
affects: [23-14, 23-15, 23-16]

actuals:
  tokens: 5000    # chars/4 over the realized diff (20,103 characters of added lines in README.md and crates/, 432 lines added, 0 removed)
  tasks: 2
  commits: 2      # the docs(23-21) completion commit follows

tech-stack:
  added: []
  patterns:
    - "a gate refusal through the binary is asserted in one helper: exit, slug, empty stdout, zero gateway requests, no .ign/ directory, host tree equal to its pre-run copy"
    - "mutation check of a flag-mapping test: break the flag in main.rs, see the two tests that depend on it fail, git checkout the file"

key-files:
  created: []
  modified:
    - README.md
    - crates/ignition-cli/tests/deploy_family_coverage.rs
    - crates/ignition-cli/tests/contract_deploy.rs

key-decisions:
  - "The verdict exception is dated 2026-10-18 (the run date) and called the FOURTH success-path exit exception, matching the comment in main.rs"
  - "The README states the 1,528-file refusal and the 176 s hold only in the Commands row (23-13); the new section points to it from the hold_budget row and repeats no number"
  - "The test for the verdict exception shares a dated_exception_body helper with the verify test's date check; the verify test itself is unchanged"

patterns-established:
  - "Drift in a CLI test needs no extra render: local_tracer already leaves the render in scratch/out, and apply_tracer edits the live unit, so --base scratch/out marks POSTGRES as drift"

requirements-completed: []
requirements-advanced: [DAPL-01, DAPL-02, DAPL-07]

duration: ~1h40m (the three workspace test and clippy runs dominate)
completed: 2026-10-18
---

# Phase 23 Plan 21: README for `deploy apply`, the rolled-back verdict exception and the gate-exit matrix Summary

**A caller can now learn from the README that `ok: true` with `data.slug` `deploy_rolled_back` and exit 6 means a rollback, how to run `deploy apply`, and every gate refusal (stale hash, destructive, shadowed removal, unflagged drift) is pinned through the binary as a zero-request, zero-write exit.**

## README headings

- `### The \`deploy apply\` rolled-back verdict exception (2026-10-18)` under `## Output contract (for agents)`, after the verify exception. It names `data.ok`, `data.slug`, `data.cause` (`lock_expired`, `write_failed`, `scan_incomplete`, `verify_failed`), exit 6 and Decision A, and says `deploy_rolled_back` is in no exit-table row. It lists which failures stay ordinary error envelopes (including `rollback_failed`).
- `### \`deploy apply\`` after `### \`deploy verify\``. It covers, in order: the eight-step gate order; Decision C; the seven preflight ids (`mode_master`, `mode_backup`, `wrong_target`, `write_rest`, `write_host`, `designers`, `hold_budget`) plus the separately raised `gwbk` and `run_dir_not_ignored`; the `.gwbk` (`type=all`, 0600, ZIP-checked, `--skip-gwbk` recorded, D-13); `--designers` fail, wait and proceed (D-14, D-15); the run directory `<repo>/.ign/deploy/<gateway>/<run-id>/` with the `.ign/` ignore requirement (D-05); runs and on-box snapshots never pruned and how to remove them by hand (D-08); restart reporting without restarting (SF-3) with `restart_pending_unavailable` and `snapshot_copy_differs`; the Windows notes (Modify on the mode collection plus ownership of the moved files, the 259-character snapshot path gate, a held file as `transport_failed`); the one line on waiting out the hold after an ssh drop; output; exit codes.

No new error slug, so the four-place rule was not triggered. `readme_exit_table_agreement` and `exit_code_mapping_enumerated` stay green with the new text.

## Tests

| Behaviour | Test |
|---|---|
| dated section, `data.ok`, `data.slug`, `data.cause`, `deploy_rolled_back`, `exit 6`, `` `ok: true` ``, Decision A; slug absent from `## Exit codes` and from `DEPLOY_SLUGS`; the `### deploy apply` heading exists | `deploy_family_coverage::rolled_back_verdict_exception_is_dated_and_documented` |
| stale `--plan-sha256` after the live unit changed again: exit 6 `plan_stale`, the reviewed hash in the message, zero requests, no `.ign/`, host tree equal | `contract_deploy::apply_stale_hash_exit_6` |
| a removed unit without `--allow-destructive`: exit 2 `destructive_change_refused` naming the entry, nothing sent or written; the same plan with the flag applies (`applied`) and deletes the unit | `apply_destructive_without_flag_exit_2` |
| a removed unit that a `core` copy shadows: exit 6 `deploy_blocked` with no flags, with `--allow-destructive`, and with `--allow-destructive --allow-drift`, each with zero requests and no writes | `apply_shadows_removal_exit_6` |
| drift from `--base`: exit 2 `destructive_change_refused` without `--allow-drift`; with `--allow-drift "reviewed in PR 12"` exit 0, `applied`, and `run.json` holds `allow_drift` exactly that and `allow_destructive` false | `apply_drift_with_reason_records_it` |

TDD: the coverage test was written first and failed ("no heading under the output contract"), then the README text made it pass. The four gate tests passed on first run because 23-13's flag mapping is correct; to prove they bite, `allow_destructive` and `allow_drift` were forced off in the prepare call in `main.rs`, which made `apply_destructive_without_flag_exit_2` and `apply_drift_with_reason_records_it` fail, and the file was restored with `git checkout`. `main.rs` is unchanged in this plan.

## Verification

- `cargo test -p ignition-cli --test deploy_family_coverage`: 6 passed.
- `cargo test -p ignition-cli --test contract_deploy apply_`: 11 passed (7 from 23-13, 4 new).
- `cargo test --workspace --no-fail-fast`: 2448 passed, 0 failed, 56 ignored (live and Windows-host gates); 2443 before this plan. The known flake `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up` did not fail.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check`: clean.
- `grep -c '^### `deploy apply`' README.md` prints 1; `grep -c 'rolled-back verdict exception' README.md` prints 2.
- No live step: this plan has none. The trial was not touched and no reset was made. `ign` on PATH was not reinstalled because no verb changed.

## Task Commits

1. **Task 1 (tracer):** `897a8b4` docs(23-21): the README sections and `rolled_back_verdict_exception_is_dated_and_documented`
2. **Task 2:** `89988f0` test(23-21): the four gate-exit tests

## Deviations from Plan

### Plan departures

1. **[Plan structure] Test and implementation of Task 1 share a commit.** The RED state (test failing on the missing heading) was run but not committed alone, as in 23-13. There is no separate `test(...)` commit for Task 1.
2. **[Plan departure] No flag-mapping defect found.** Task 2's action allows a fix in the `main.rs` apply arm; none was needed, so `main.rs` is not in the diff. The mutation check above is the evidence that the tests detect a broken mapping.
3. **[Scope] `apply_shadows_removal_exit_6` seeds the shadow by hand.** `shadow_old_unit_in_core` writes `config/resources/core/ignition/database-translator/OLD/config.json` beside 23-13's `seed_old_unit_and_ledger` helper; no new fixture file.
4. **[Wording] The verify exception text still says `deploy apply` "will reuse" its envelope.** That sentence in the older section is now true, not wrong; it was left alone.

No auth gates. No Rule 1-3 fixes. No architectural change.

## Tracer feedback gate

Task 1 is the tracer. Auto mode is off and `human_verify_mode` is `end-of-phase`, and the orchestrator directed a sequential unattended run, so execution continued after the tracer commit instead of stopping for a `checkpoint:human-verify`, as in 23-10, 23-12 and 23-13. The tracer's `<verify>` (`deploy_family_coverage`, the two grep counts) passed before the commit and the whole workspace passed after Task 2. For the end-of-phase list: read the two README sections as an agent would.

## Known Stubs

None. A grep for `TODO`, `FIXME`, `todo!`, `unimplemented!`, `#[ignore]` and `skip` over the added lines finds nothing.

## Threat Flags

None beyond the plan's register. T-23-79 (destructive or shadowed change through the binary): `apply_destructive_without_flag_exit_2`, `apply_shadows_removal_exit_6`. T-23-80 (stale hash): `apply_stale_hash_exit_6`. T-23-81 (rolled-back read as success): the dated exception and its test. T-23-82 (drift reason): `apply_drift_with_reason_records_it` asserts it lands in `run.json`; this plan does not assert that it is absent from stdout, stderr and events, which is 23-15's canary.

## Requirements

`requirements-completed` is empty by instruction. Advanced: DAPL-01 (gates through the binary), DAPL-02 (stale hash and drift reason through the binary), DAPL-07 (Decision A documented and pinned).

## Carry-overs

### 23-14 (rollback verb)

- The README `### deploy apply` section links forward to nothing; it names the recovery command `ign deploy rollback --run <id> --env <env>` only as what the `deploy_journal_present` message prints. Add `### \`deploy rollback\`` after it, and mention the restore-time ssh-drop wait there too (the apply section carries the one line already; state it once more only if the rollback behaviour differs).
- The apply section says a run whose snapshot is removed can no longer be rolled back, and not to delete a `journal.json` by hand unless the `rollback_failed` guidance says so; the rollback section should be consistent with both.
- Rollback outcomes `restored` and `cleared` are not in the verdict-exception section (it covers apply only). If rollback gets its own exit-6 behaviour, extend or add a section rather than editing the dated apply one.
- `shadow_old_unit_in_core`, `run_apply` and `assert_refused_before_any_write` in `contract_deploy.rs` are reusable for rollback refusals.

### 23-15 (canary, stdout purity)

- `assert_refused_before_any_write` already checks empty stdout and zero requests for four refusals; a canary case can reuse it. The drift reason `reviewed in PR 12` is recorded verbatim in `run.json`; whether it also reaches the envelope, stderr or `events.ndjson` is not asserted here, so the canary should check those channels.

### 23-16 (live)

- Nothing in this plan needed the live host. The README's Windows notes (Modify plus ownership, the 259 gate, a held file as `transport_failed`) are stated from the 23-08 and 23-20 captures; a live CLI apply on `ign-win-836` is the end-to-end proof.

## Self-Check: PASSED

- Files exist: `README.md`, `crates/ignition-cli/tests/deploy_family_coverage.rs`, `crates/ignition-cli/tests/contract_deploy.rs`, this SUMMARY.
- Commits `897a8b4` and `89988f0` exist in `git log`.
- `grep -c 'rolled_back_verdict_exception_is_dated_and_documented' crates/ignition-cli/tests/deploy_family_coverage.rs` prints 1; `apply_shadows_removal_exit_6` is in `contract_deploy.rs`.
