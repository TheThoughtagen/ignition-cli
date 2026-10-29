---
phase: 24-redundant-pair
plan: 08
subsystem: cli
tags: [ignition, redundancy, deploy-apply, backup-timeout, exit-6, readme, verdict-exception]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: "24-05 BackupNode and the read-only pair module; 24-06 converge_backup and applied_backup_unsynced; 24-07 ten preflights; 24-02 contract decisions V1, V2, V4, V5"
provides:
  - "ign deploy apply --backup-timeout SECS (default 300, clap range 1..=86400)"
  - "the backup's read half built with for_node(.., \"nodes.backup\", ..) only when nodes.backup declares ssh or local; None for a REST-only backup"
  - "human backup block and stderr progress lines; exit 6 for applied_backup_unsynced after the envelope"
  - "README: fifth dated success-path exit exception, backup-check paragraph, ten preflight rows, Commands row, rollback sentence"
  - "backup_unsynced_verdict_exception_is_dated_and_documented"
affects: [24-09, 24-10]

actuals:
  tokens: 40000
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "a second verdict kind reuses the dated-exception pattern: payload slug in data.slug, never a CoreError, never an exit-table row"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/src/actions/deploy_apply.rs
    - crates/ignition-cli/tests/common/mod.rs
    - crates/ignition-cli/tests/contract_deploy.rs
    - crates/ignition-core/tests/live_deploy_apply.rs
    - README.md
    - crates/ignition-cli/tests/deploy_family_coverage.rs

key-decisions:
  - "The backup node never gets a write half in the CLI: only for_node's TreeReader is built, and write_half_unreachable stays green"
  - "The README exception is dated 2026-10-20 and sits under the output contract before the real exit-codes section; the literal exit-codes heading text appears once"
  - "A killed run during the backup wait is documented: applied master, no verdict, follow with deploy verify --node backup"

patterns-established:
  - "README exception text is pinned by needles (envelope fields, four reasons, exit 6, Decision A, not rolled back, follow-up command)"

requirements-completed: []

duration: not timed
completed: 2026-10-20
---

# Phase 24 Plan 08: ign deploy apply checks the backup and exits 6 when it does not converge Summary

**`ign deploy apply` to a redundant pair now builds the backup's read-only file access, takes `--backup-timeout`, prints the backup verdict in both output modes and exits 6 with `applied_backup_unsynced` when the backup is not proven, and the README documents that verdict as the fifth dated exit exception, the backup check and the ten preflights.**

## What was built

- **Task 1 (tracer, commit 815fe0d, user approved).** `Apply.backup_timeout` in `cli.rs` (default 300, `value_parser` range 1..=86400, so 0 is a clap usage error, exit 2); `ApplyRunOptions.backup_timeout` feeds `BackupPolicy { budget, interval: BACKUP_POLL_S }`; the apply arm in `main.rs` builds `for_node(&repo, &declared.target_file, "nodes.backup", node)` only for a backup with `ssh` or `local`, `None` otherwise, and hands it to `BackupNode::new(s.api(), s.api(), reader)`. `render.rs` prints `backup: converged` or `backup: NOT converged (<reason>)`, waited seconds, resync result and one line per check, plus the `ign deploy apply:` stderr progress lines for the wait, resync and verdict. Contract tests: `apply_backup_unsynced_exit_6`, `apply_backup_converged_exit_0`, `apply_backup_timeout_bounds`, `apply_backup_human_block`, and `apply_flags_are_exactly_these` gained `backup-timeout`.
- **Task 2 (commit 2f9d685).** README: `### The deploy apply backup-unsynced verdict exception (2026-10-20)` under the output contract (envelope shape, the four reasons in precedence order, exit 6, master not rolled back, Decision A, the killed-run note); the `deploy apply` Commands row (flag, one-sentence behaviour, verdict, `backup` data key); a `**The backup check.**` paragraph in the `deploy apply` section including the REST-only `files` warn sentence; "Ten checks" with the `master_active`, `backup_read` and `secrets` rows (K8 404 provider skipped with a named detail; K7 500/501 provider unverifiable and not a failure); the apply exit-code paragraph; the rollback sentence (rollback runs `master_active`; `backup_read` and `secrets` are skipped). `backup_unsynced_verdict_exception_is_dated_and_documented` pins the needles, the absence of `deploy_backup_unsynced` from the exit table and from `CoreError::DEPLOY_SLUGS`.

## Deviations from Plan

**1. [Process] No separate RED commit for Task 1.** Tests and implementation were written together and committed once (815fe0d): each build is about 8 minutes under load and the disk was nearly full. The tests assert the new behaviour and the new flag, so they would fail without the implementation. Task 2 also has no separate RED commit: the doc pin and the README text were committed together (2f9d685); the pin failed once on a wrapped line before the README was fixed.

**2. [Test fixture] The backup's `deploymentMode` in the mock is `dev`, not `production`.** The tracer target is `mode: dev`, so `mode_backup` only passes with `dev`. The plan text said production.

**3. [Comment only] The rollback arm still passes `None` as the backup reader.** Rollback skips `backup_read`, so no reader is needed; only the stale comment was replaced.

**4. [Wording] Two of 24-06's stderr progress lines were reworded** (the wait line and the resync line) to the plan's wording. Nothing else asserted the old text.

**5. [Gate] The tracer gate stopped after Task 1;** the user approved ("tracer verified") and Task 2 followed.

**6. [README] A line wrap broke a pinned needle.** `--node backup` was split across lines in the first draft; the paragraph was reflowed so the pinned text is on one line. No behaviour change.

## Verification

- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --no-fail-fast`: 2616 passed, 1 failed. The one failure is the known unrelated load failure `live_gateway::guard_drop_during_unwind_inside_runtime_still_cleans_up`, not touched by this plan. `actions::diagnostics::tests::unknown_states_keep_polling_until_the_deadline` and the docker `contract_rig` tests did not fail in this run.
- Plan checks: `deploy_family_coverage` 8 passed (including the new pin, `every_deploy_leaf_has_a_readme_commands_row`, `rollback_section_agrees_with_apply_section`); `readme_exit_table_agreement` passed; `grep -c '^## Exit codes' README.md` prints 1; three preflight table rows grep as expected; `write_half_unreachable` green; `contract_deploy` apply tests green.

## Known Stubs

None.

## Threat Flags

None. T-24-28 (only the read half for `nodes.backup`, backup mock saw GETs only), T-24-29 (verdict contract pinned by the new test, `DEPLOY_SLUGS` unchanged, flag list pinned) and T-24-30 (clap range) are mitigated as planned.

## Self-Check: PASSED

- FOUND: .planning/phases/24-redundant-pair/24-08-SUMMARY.md
- FOUND: README.md, crates/ignition-cli/tests/deploy_family_coverage.rs (modified in 2f9d685)
- FOUND commits: 815fe0d, 2f9d685
