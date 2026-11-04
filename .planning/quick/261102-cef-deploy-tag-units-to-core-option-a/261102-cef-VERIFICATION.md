---
phase: quick-261102-cef
verified: 2026-11-02T22:30:00Z
status: human_needed
score: 7/8 must-haves verified
behavior_unverified: 1
overrides_applied: 0
behavior_unverified_items:
  - truth: "Every apply host (local, commit.sh, commit.ps1) writes, deletes, snapshots, restores and fingerprints .core/ keys in config/resources/core, never creates core or removes core/ignition/<type>, refuses .core/ keys outside the two tag types, and write_host fails closed naming the folder"
    test: "Run the conformance_ps_core_* rows (cargo test -p ignition-core --test deploy_host_conformance _core_) on the Windows CI leg"
    expected: "All ps core rows pass: migration in one commit, restore puts the mode copy back, pre-image check, core type folder kept, core root absent never created, outside-type keys refused, CanAddFile probe names the folder"
    why_human: "The ps rows are #[cfg(windows)] and only compile on macOS. local and sh rows were run and pass here; commit.ps1 behaviour was not executed"
human_verification:
  - test: "Windows CI leg: conformance_ps_core_* rows"
    expected: "Every ps core row passes"
    why_human: "Windows-only tests, not runnable on this Mac"
  - test: "Before production use, capture the Windows deploy account's Modify grant on config\\resources\\core\\ignition\\tag-definition and tag-type-definition on a real host (O4/A4)"
    expected: "The account can replace and delete gateway-owned core tag files; write_host passes"
    why_human: "Real Windows ACL behaviour; the Docker rig cannot exercise it (explicitly out of scope, recorded as an open item)"
  - test: "Decide whether a migration planned with --base should report drift"
    expected: "Operator accepts that a migration with --base needs --allow-drift, or a follow-up adjusts apply_base for routed .core/ rows whose mode copy matches the base"
    why_human: "Design decision. Reasoned from code, not tested: apply_base compares a .core/ row's live sha (None, because core has no copy yet) with the base sha of the bare path (Some), so every migrated unit gets drift: true and is refused without --allow-drift. It fails closed, so it is a usability gap, not a safety gap"
---

# Quick 261102-cef: deploy tag units to core on redundant pairs (A2), verification report

**Goal:** On a target with `nodes.backup`, write tag-definition and tag-type-definition units to `core` (D1, pairs only). Treat Designer edits to ign-managed core tag folders as drift (D2). Move tag units already in a pair's mode collection to core in one apply (D3). Standalone targets stay byte-identical.
**Verified:** 2026-11-02T22:30Z, against HEAD a3e367d (base 1e9482d)
**Status:** human_needed
**Re-verification:** No, this is the first verification

## Observable truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | D1: on a pair, plan routes the two tag types to `.core/` rows compared with the core copy, apply writes them into core in one commit and scan, the mode collection gains no tag path, and the backup converges | VERIFIED | `layout::route` qualifies a path only when it is `Pair && is_pair_unsynced_tag_path`. `plan_with` sets `routed`, uses `basis = core_copy` and builds rows with `routed_rows`. Every production plan call passes `Redundancy::of(&declared.target)`: `deploy_plan` (actions/deploy.rs:412), verify (1238) and backup verify (1627). There is no remaining unrouted `plan(` call in src. `write_set` reads bytes by `split_key(..).1`. `pair_core_added_unit_lands_in_core` passes. The live gate `live_pair_core_tag_units_converge` wrote 4 `.core/` files, `waited_s 0`, resync None, Files Pass 8, no snapshot mismatch, both nodes Good |
| 2 | D3: a ledgered mode unit moves to core in one apply (core written, mode files and emptied type folder deleted in the same commit, the ledger lists `.core/`), and the move is destructive and needs `--allow-destructive` | VERIFIED | `delete_set` uses the routed render, so a bare ledger path whose unit now routes to core is deleted from `observed.mode`. The emptied-dir floor is 2 for a bare tag path. Tests `core_route_migration_rows_and_deletes`, `pair_core_migration_moves_mode_unit_in_one_commit`, `pair_core_migration_needs_allow_destructive` and `conformance_{local,sh}_core_migrate_mode_unit_in_one_commit` pass. Live `live_pair_core_migration_converges`: the pair was wedged first (mismatch at 2 s, TEC-5105 at 12 s), then one apply wrote 4 `.core/` files and deleted 4 mode files, converged after the one allowed resync, and the next non-tag change converged with `waited_s 0` and no resync |
| 3 | `deploy_pair_tags_refused` (exit 6) is unchanged as a slug and fires only for residue; added, changed and same units in core pass the gate, the engine guard and the plan warning | VERIFIED | `pair_tag_keys` lists only bare rows that are live in mode and not in the delete set, or that carry a desired sha. Gate test `pair_tags_written_or_kept_in_the_mode_collection_are_refused_on_a_pair_only` is rewritten and passes. Live `live_pair_refuses_a_tag_unit` refused with exit 6, wrote nothing and did not contact gw-b |
| 4 | D2: a Designer edit in a core tag folder is `changed`/`differs`, `--base` gives `drift: true` and refuses without `--allow-drift`, and an unledgered core file is never deleted (`shadows_removal` / `deploy_blocked`) | VERIFIED | `apply_base` strips `.core/`. `delete_set` only ever deletes ledger entries, and a `.core/` entry outside the two types is skipped. A `removed` unit is `shadows_removal` unless `core_gone`. Tests `core_route_drift_on_core_copy` (refusal, then allowed with the flag) and `core_route_ambiguous_removal_shadows` pass. See WARNING W1 below on migration plus `--base` |
| 5 | Standalone and tag-free pair plans, ledgers, journals, records, work lists and fingerprints are byte-identical, and the goldens are unchanged | VERIFIED | `plan()` is `plan_with(.., Standalone)`. The three PLAN_GOLDEN hashes, RENDER_GOLDEN `7c8b41b1...` and fingerprint GOLDEN `ca50ebbf...` appear exactly once at both HEAD and 1e9482d. The deploy_plan.rs diff has no `-` lines, and fingerprint.rs and crates/ignition-cli are untouched. `core_route_standalone_equals_plan` and `core_route_tag_free_pair_equals_standalone` pass |
| 6 | Every apply host (local, commit.sh, commit.ps1) handles `.core/` keys, never creates core or removes `core/ignition/<type>`, refuses keys outside the two types, and write_host fails closed naming the folder | PRESENT_BEHAVIOR_UNVERIFIED (ps host only) | local and sh: 37 `_core_` tests pass, among them `conformance_sh_core_root_absent_never_created` and `conformance_sh_core_probe_checks_core_root`. commit.sh derives `CR=${CO%/*}/core` (never from input), refuses `.core/*` outside the two types, runs symlink, missing and not-dir checks, and never creates `CR`. `rmdir_up` uses floor 3 for `.core/` keys. commit.ps1 changed (+160/-47 lines), but its rows are `#[cfg(windows)]` and were not executed here |
| 7 | Verify on a pair judges against core, adds one mode-residue `data.warnings` line with no new check id, the files check accepts gw-b's re-serialized `unary-resource.json`, and a pulled pair tree is a fixpoint | VERIFIED | Both verify plan calls use `plan_with(.., Redundancy::of)`. `pair_mode_tags_warning` is added only on a pair, from the master verify only. `FILE_NORMALIZATION` gains `("unary-resource.json", &[])`. Tests `pair_verify_core_units_green`, `pair_verify_warns_on_mode_tag_files`, `standalone_verify_no_pair_warning`, `pull_pair_core_tags_fixpoint` and `core_key_files_check_unary_parsed` pass. Live: gw-b `unary-resource.json` was parsed-equal and `udts.json` byte-equal |
| 8 | Live 8.3.6 pair: tag-definition plus udts.json apply ends `applied` with no resync and no mismatch, a following non-tag change needs no resync, and the migration of both types converges in one apply | VERIFIED (from the log, not re-run) | The log shows `LIVE-RAN` and `... ok` for all 8 gates. `test result: ok. 8 passed; 0 failed`. There are no IP literals in the log. Caveat: the log is from 21:25-21:32Z and commit a3e367d is from 21:41Z, so the run was against the pre-commit working tree |

**Score:** 7/8 truths verified. One is present but behavior-unverified: the ps host.

## Unplanned addition: unrouted pair plans are refused

`pair_tag_keys` also lists a routed-type entry that has a bare row with a desired sha. On a pair, `routed_rows` never emits a bare desired row, and `routed` (`is_pair_unsynced_tag_path(key)`) selects the same entries as the filter (`module == ignition && ty in PAIR_UNSYNCED_TAG_TYPES`). So the rule can only fire on a plan built unrouted, such as a standalone plan handed to an engine whose target is now a pair. It cannot false-refuse a correctly routed plan. Standalone targets are never checked. `pair_tags_unrouted_plan_is_refused_on_a_pair` and `pair_run_apply_refuses_a_tag_plan_that_skipped_the_gates` pass, and the second asserts no write, no run directory, no host call and no lock. **The addition holds up.**

## Offline gates (re-run by this verifier)

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | exit 0: 96 result lines, 3157 passed, 0 failed, 96 ignored (the live gates are ignored by design) |
| `cargo xwin clippy` (windows-msvc) | Not re-run here. The executor reports T2-OK and FULL-GATE-OK |

## Scope and hygiene

| Check | Result |
|-------|--------|
| Cargo.toml and Cargo.lock (all three crates) vs 1e9482d | unchanged |
| Protected files in commits 1e9482d..HEAD (`.planning/config.json`, `.planning/research/.cache/`, `*-uat4.ps1`, `.planning/debug/tg-pg-table-check.md`) | none. `.planning/config.json` is tracked from earlier history but untouched by these commits |
| IP-shaped literals in added lines (excluding loopback and 192.0.2.x) | none |
| Co-authored-by lines | 0 |
| Staged changes | none |
| Worktrees | only the main checkout, and no `.claude/worktrees` |
| Debt markers (TBD/FIXME/XXX/TODO/HACK) in added lines | none |
| New `#[ignore]` | only the two new live gates, using the existing live-gate pattern |

## Warnings

- **W1, migration with `--base` reports drift (reasoned from code, not tested).** On a `.core/` row the live sha is `None`, because core has no copy yet. The base, which is unrouted, holds the bare path, so `live != base && live != desired` and every migrated unit is `drift: true`. A migration planned with `--base` then needs `--allow-drift` as well as `--allow-destructive`. It fails closed, so this is not a safety gap. Neither the README nor any test covers the case. The live gates ran with `base: None`.
- **W2.** The live evidence comes from the working tree about 9 minutes before commit a3e367d. The offline suite at HEAD is green.
- **W3.** The Windows ACL capture and the production-pair migration are still open items recorded in SUMMARY (O4). Per the plan, they are out of scope.

## Human verification required

1. The `conformance_ps_core_*` rows on the Windows CI leg.
2. The Windows deploy-account `Modify` grant on the core tag folders, before production use.
3. A decision on W1: accept that a migration with `--base` needs `--allow-drift`, or open a follow-up.

---

_Verified: 2026-11-02T22:30Z_
_Verifier: Claude (gsd-verifier)_
