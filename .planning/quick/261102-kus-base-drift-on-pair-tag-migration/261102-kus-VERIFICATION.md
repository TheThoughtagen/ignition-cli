---
phase: quick-261102-kus
verified: 2026-11-02T00:00:00Z
status: passed
score: 5/5 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Quick 261102-kus Verification Report

**Goal:** On a redundant pair, a tag-unit migration planned with `--base` is not drift when the mode copy equals the base. Real drift is still reported: an edited or missing mode file, an unledgered or partial core copy. An ambiguous removal stays `deploy_blocked`.
**Commits:** ca4933a (fix), 78874d0 (tests and README) on top of 8d7bf60.
**Re-verification:** No, initial verification.

## Observable truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Clean migration with `--base`: `drift: false`, `summary.drifted` 0, `evaluate_gates` Ready with `--allow-destructive` alone | VERIFIED | `core_route_base_migration_clean_no_drift` passes (two renders, [1] and [2]; asserts drift, summary, delete set, unchanged `plan_sha256`, Ready). Pre-fix RED in `261102-kus-red.log`: `... FAILED`. |
| 2 | Edited mode copy, or a base file missing from the mode copy, is still drift and refused (`destructive_change_refused`, exit 2) | VERIFIED | `core_route_base_migration_mode_edit_drifts` (b1, b2) passes. Mutants M1 and M3 make it fail (`261102-kus-m1.log`, `m3.log`). |
| 3 | A routed unit with any core copy is judged on its own shas (unledgered or partial copy is drift). Ambiguous removal stays `shadows_removal` / `deploy_blocked` exit 6 | VERIFIED | `core_route_base_migration_core_copy_drifts` (c1, c2, c3) passes. Mutants M2 and M2b make it fail. |
| 4 | Standalone and non-migration verdicts unchanged. Existing tests pass, `plan_sha256` untouched, five goldens byte-identical | VERIFIED | I re-ran `cargo test --workspace`: exit 0, 96 `test result: ok`, none FAILED. All 16 `core_route` tests pass. All five golden hashes are present once each. `git diff` shows no removed lines in `deploy_plan.rs`. No diff under `deploy/apply`, `actions` or `layout.rs`. |
| 5 | README migration bullet says how `--base` judges a migration | VERIFIED | One sentence is inserted after "`--allow-destructive`." with a 6-line rewrap. The gate phrase "judged by the mode copy the apply deletes" is on one line. No other README line changed. |

## Adversarial review of `apply_base` (plan.rs 1463-1498)

The substitution fires only for a row that meets all of these conditions:
- the row is a `.core/` row,
- its own live sha is `None`,
- the entry has no `core` in `live_in`,
- the bare path is in the plan-wide bare delete set,
- the entry has a bare row at that path.

The row then takes the mode sha at that path as its live side.

I found no uncovered case where this masks real drift.

- **Bare rows still catch any mode edit.** Every mode file produces a bare row (live = mode sha, desired null) that is not substituted. So `mode[p] != base[p]` is always drift on that bare row. The substitution can only clear a `.core/p` row when `mode[p] == base[p]` or `mode[p] == desired[p]`. In the second case the bare row still drifts unless the mode sha also equals the base. This holds for a mode file edited to equal the render, and for a unit absent from the base (mode `Some` against base `None`).
- **Missing mode file.** `mode_shas.get(p)` is `None`, so a `.core/p` row against a base that has `p` is drift. If the base also lacks `p`, there is nothing to drift.
- **Unledgered or partly ledgered mode file.** It is not in the delete set, so the row is not substituted and is judged `None` against the base, which is conservative drift. `pair_tag_keys` also refuses the residue.
- **Core copy of any shape, including partial.** `core_absent` is false, so no substitution happens (c1, c2). Mutant M2b (only `core_absent` dropped) fails `core_copy_drifts`, which shows the guard is load-bearing.
- **Cross-unit contamination.** `mode_shas` is per entry. Paths include the unit directory, so the plan-wide `deleted_bare` set cannot leak a match between units.
- **Standalone targets.** `routed_rows` is the only producer of `.core/` rows with a null live sha, and it runs only when `redundancy == Pair` and the unit is a routed tag type. Standalone `.core/` rows are core-delete pre-images with `live_sha256: Some`, so `migrated` is false by construction and standalone drift is unchanged. Non-tag units never get `.core/` rows.
- **Gates.** `evaluate_gates` is untouched, and `entry.drift` is the only input. Test c3 pins `deploy_blocked` exit 6 with both flags.

One intended side effect, not a gap: a `.core/` row whose base entry is absent and whose mode copy is absent is not drift. That is the existing behaviour for added files.

## Gates re-run by the verifier

| Check | Result |
|-------|--------|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | exit 0; 96 `test result: ok`; no FAILED; log at `.../scratchpad/261102-kus-verify-ws.log` |
| Mutant and RED logs (red, m1, m2, m2b, m3) | each named test is FAILED in its log as claimed |
| Goldens (`PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE`, fingerprint `GOLDEN`, `RENDER_GOLDEN`) | each present once, unchanged |
| Cargo manifests and lock | no diff 8d7bf60..HEAD |
| IP-shaped literals in added lines | none |
| `Co-Authored-By` in commits | none |
| Protected files committed or staged (`.planning/config.json`, `.planning/research/.cache/`, `*-uat4.ps1`, `.planning/debug/tg-pg-table-check.md`) | none committed, nothing staged |
| Commit scope | ca4933a touches plan.rs and deploy_plan.rs only; 78874d0 touches README.md and deploy_plan.rs only |
| Worktrees | only the main checkout |
| windows-msvc xwin clippy | not re-run (cross-compile toolchain). I relied on `kus-gate2.out` in the scratchpad, which ends `FULL-GATE-OK`. |

## Anti-patterns

None. There are no TBD, FIXME or XXX markers and no stubs in the added code.

## Requirements

QUICK-kus is satisfied. The cef VERIFICATION W1 follow-up (tracked as W4) is closed.

## Gaps

None.

## Notes

- SUMMARY deviation 1: mutants were reverted by `cp` from a saved copy rather than with Edit, and the final state is byte-identical. `plan.rs` has exactly one commit since 8d7bf60.
- The README sentence reads "a `core` copy already exists that differs from it". A partial core copy that equals the base also drifts (c2), because the missing file has no live core sha. The behaviour is the conservative side, and the wording is acceptable.

_Verifier: Claude (gsd-verifier)_
