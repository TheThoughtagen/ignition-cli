---
phase: quick-261102-kus
plan: 01
subsystem: deploy
tags: [deploy, redundancy, drift, apply_base]
requires: [261102-cef]
provides: ["apply_base judges a pair migration's .core/ rows by the mode copy the plan deletes"]
affects: [crates/ignition-core/src/deploy/plan.rs]
key-files:
  modified:
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/tests/deploy_plan.rs
    - README.md
status: complete
actuals:
  tasks: 2
  commits: 2
---

# Quick 261102-kus: base drift on pair tag migration

A pair migration planned with `--base` no longer reports false drift: a clean migration applies with `--allow-destructive` alone. Real drift (edited or missing mode file, any core copy that differs, ambiguous removal) is unchanged. Closes 261102-cef VERIFICATION W1 (tracked as W4).

## The rule as built

In `apply_base` (plan.rs), a `.core/` row takes the sha of the bare row at the same collection path as its live side only when all hold: its own `live_sha256` is `None`; the entry has no `core` in `live_in` (no core copy at all); the bare path is in the plan's `delete_set`; the entry has a bare row at that path. Every other row keeps its own live sha. Bare rows are unchanged (mode sha vs base). A `.core/` row whose bare path is missing from the mode copy keeps `None` and drifts. `plan_with`, `plan_sha256`, apply gates, actions and layout are untouched.

## Commits

- ca4933a fix(261102-kus): judge a pair migration's core rows by its mode copy under --base
- 78874d0 test(261102-kus): pin real drift on pair migrations under --base; README note

## Gate tokens

- `T1-OK`
- `FULL-GATE-OK` (workspace: 96 `test result: ok` lines, none FAILED; fmt, workspace clippy and windows-msvc xwin clippy clean)

## RED and mutant evidence

- RED (pre-fix, 261102-kus-red.log): `test core_route::core_route_base_migration_clean_no_drift ... FAILED` (assertion at the drift check, `left: Some(true) right: Some(false)`, render [1]).
- M1 (no core copy and all bare rows deleted is never drift), 261102-kus-m1.log: `test core_route::core_route_base_migration_mode_edit_drifts ... FAILED`
- M2 (mode sha replaces any `.core/` row whose bare path is deleted; both `core_absent` and the null-live check dropped), 261102-kus-m2.log: `test core_route::core_route_base_migration_core_copy_drifts ... FAILED`
- M2b (extra: only `core_absent` dropped), 261102-kus-m2b.log: `core_route_base_migration_core_copy_drifts ... FAILED`
- M3 (every `.core/` row with a null live sha ignored), 261102-kus-m3.log: `test core_route::core_route_base_migration_mode_edit_drifts ... FAILED` (core_copy_drifts also fails)

All logs are in the scratchpad directory.

## README sentence

"With `--base`, a moved unit's `core` rows are judged by the mode copy the apply deletes, so the migration is `drift: true` only when that mode copy differs from the base or a `core` copy already exists that differs from it; a clean migration needs no `--allow-drift`."

## Deviations from Plan

1. [Process] Mutants were applied and reverted by copying saved files over plan.rs (cp from a saved good copy), not with Edit. Equivalent effect; `git diff --quiet HEAD -- plan.rs` held after every revert and exactly one commit touches plan.rs.
2. [Process] My first M3 attempt skipped only the substituted (migrated) rows and did not fail b2 (the non-deleted `.core/tags.json` row still drifts). It was replaced by the plan's M3 definition (all null-live `.core/` rows ignored), which fails as required. The weaker variant is not recorded as evidence.
3. [Process] The README insertion split the line ending "The backup of a" (necessary to place the sentence directly after "`--allow-destructive`."); the phrase "judged by the mode copy the apply deletes" is kept on one line for the grep gate.

## Known Stubs

None.

## Self-Check: PASSED

Both commits exist (ca4933a, 78874d0), plan.rs equals Task 1's commit, no protected file staged or committed, no Co-Authored-By, no push. SUMMARY is uncommitted by design.
