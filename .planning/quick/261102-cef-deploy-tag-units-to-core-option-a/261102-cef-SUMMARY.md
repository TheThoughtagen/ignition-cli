---
phase: quick-261102-cef
plan: 01
subsystem: deploy
tags: [deploy, redundancy, tags, core-collection, ignition-8.3.6, commit.sh, commit.ps1]

requires:
  - phase: debug deploy-tag-redundancy-sync
    provides: root cause (mode-collection tag paths wedge an 8.3.6 pair), option A, rig controls C1/C3
  - phase: quick 261102-9t6
    provides: pair_tag_keys delete-set rule, deploy_pair_tags_refused (v1.6.5)
provides:
  - "A2 routing: on a target with nodes.backup, tag-definition and tag-type-definition units are planned, applied, ledgered, rolled back and verified in config/resources/core"
  - "The .core/<collection path> apply key across plan rows, delete set, ledger, ops, journal, record, fingerprint and work list"
  - "One-apply migration of a pair's ledgered mode-collection tag units into core"
  - "core root, core/ snapshot and staging slots and floor-3 emptied-dir rule on the local, sh and ps hosts"
  - "write_host core probe (core_missing, core_symlink, core_not_dir, core_tags_unwritable:<type>)"
  - "Routed deploy verify with a mode-residue data.warnings line"
  - "unary-resource.json parsed-JSON row in the pair files check"
affects: [deploy apply, deploy plan, deploy verify, deploy rollback, deploy pull, README Tags and redundant pairs]

actuals:
  tokens: 87761
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Qualified write-half key (.core/<path>) parsed once at every boundary by apply_key_problem"
    - "plan_with(.., Redundancy) beside plan(..) so every standalone caller stays byte-identical"

key-files:
  created:
    - .planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md
    - .planning/quick/261102-cef-deploy-tag-units-to-core-option-a/deferred-items.md
  modified:
    - crates/ignition-core/src/deploy/layout.rs
    - crates/ignition-core/src/deploy/target.rs
    - crates/ignition-core/src/deploy/plan.rs
    - crates/ignition-core/src/deploy/ledger.rs
    - crates/ignition-core/src/deploy/pair.rs
    - crates/ignition-core/src/deploy/apply/mod.rs
    - crates/ignition-core/src/deploy/apply/host.rs
    - crates/ignition-core/src/deploy/apply/gates.rs
    - crates/ignition-core/src/deploy/apply/journal.rs
    - crates/ignition-core/src/deploy/apply/converge.rs
    - crates/ignition-core/src/deploy/apply/preflight.rs
    - crates/ignition-core/src/deploy/apply/rollback.rs
    - crates/ignition-core/src/deploy/transport/mod.rs
    - crates/ignition-core/src/deploy/transport/batch.rs
    - crates/ignition-core/src/deploy/transport/local.rs
    - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
    - crates/ignition-core/src/deploy/transport/winps.rs
    - crates/ignition-core/src/deploy/transport/scripts/commit.sh
    - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
    - crates/ignition-core/src/actions/deploy.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/common/fake_deploy.rs
    - crates/ignition-core/tests/deploy_plan.rs
    - crates/ignition-core/tests/deploy_apply.rs
    - crates/ignition-core/tests/deploy_host_conformance.rs
    - crates/ignition-core/tests/deploy_pair.rs
    - crates/ignition-core/tests/deploy_pull.rs
    - crates/ignition-core/tests/live_deploy_pair.rs
    - README.md

key-decisions:
  - "P1: a core-collection path is the qualified key .core/<collection path>; mode paths stay bare"
  - "P2: Redundancy moves to deploy/target.rs; layout::route qualifies only the two tag types on a Pair"
  - "P5: deploy_pair_tags_refused is kept for residue only (a mode file the apply cannot delete), plus any unrouted bare write on a pair"
  - "P7: core is never created; emptied core dirs stop at <module>/<type> (floor 3)"
  - "P8: unary-resource.json is compared as parsed JSON; udts.json needed no row (byte-equal on gw-b live)"
  - "O1: a migration is destructive and needs --allow-destructive"

requirements-completed: [QUICK-cef]

duration: 2h02m
completed: 2026-11-02
status: complete
---

# Quick 261102-cef Plan 01: Deploy tag units to core on redundant pairs (A2) Summary

**On a target with `nodes.backup`, `ign deploy` now plans, writes, ledgers, rolls back and verifies `tag-definition` and `tag-type-definition` units in `config/resources/core` under a `.core/` apply key. A pair's ledgered mode-collection tag units move to `core` in one `--allow-destructive` apply, and the live 8.3.6 pair converged with no snapshot mismatch.**

## Performance

- **Duration:** about 2 h (2026-11-02T19:31Z to 21:33Z), including about 30 min to repair the pair rig (see Deviations)
- **Tasks:** 3 of 3
- **Files modified:** 30 source, test and doc files, plus this SUMMARY and deferred-items.md

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 | `24ed482` | feat(261102-cef): route pair tag units to the core collection (engine, plan, local host) |
| 2 | `2835ed3` | feat(261102-cef): core collection writes on every apply host, write_host core probe, pair rollback rule |
| 3 | the commit that adds this file (`git log -1 -- .planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md`) | feat(261102-cef): routed pair verify, README, live pair gates |

## Orchestrator resolutions (O1-O5)

- **O1.** A migration stays destructive: mode deletes count in `deleted_here`, so the entry is `destructive` (`tag_data_loss`) and apply needs `--allow-destructive`. Pinned by `pair_core_migration_needs_allow_destructive`. README says so.
- **O2.** On a pair, `deploy verify` adds one `data.warnings` line naming the tag files still in the master's mode collection. No new check id. Pinned by `pair_verify_warns_on_mode_tag_files`.
- **O3.** The live gate covers a tag-definition unit and a tag-type-definition (`udts.json`) unit, plus the migration of both from a wedged mode collection (`live_pair_core_tag_units_converge`, `live_pair_core_migration_converges`).
- **O4.** The Docker rig cannot exercise the Windows deploy account's ACL. `write_host` probes the core tag folders and fails closed with text that names the folder. The Windows ACL capture is an open item (below).
- **O5.** The `.core/<path>` key is adopted. Standalone and tag-free plans, ledgers, journals, records, work lists and fingerprints are byte-identical. `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE`, `RENDER_GOLDEN` (`7c8b41b1...`) and the fingerprint `GOLDEN` (`ca50ebbf...`) are unchanged, and all three gates grep them.

## Planner decisions (P1-P15), as built

- **P1.** In `layout.rs`: `CORE_KEY_PREFIX = ".core/"`, `KeyCollection`, `split_key` and `core_key`. `transport::apply_key_problem` splits the qualifier once, admits only the two tag types and then runs `mirror_path_problem` on the rest. The legacy mirror (`for_mirror`, `validate_mirror_*`) stays bare-only. An older build refuses a `.core/` key at its dot component.
- **P2.** `Redundancy` and `Redundancy::of` now live in `target.rs`; `gates.rs` re-exports them with `pub use`. `layout::route(redundancy, path)` qualifies a path only on a pair and only for the two tag types.
- **P3.** `plan_with(.., redundancy)`; `plan(..)` is `plan_with(.., Standalone)`. A routed unit's status is judged against its core copy. Its rows are the `.core/` rows over render and core, plus bare rows for the mode copy. Its core relation is never `hidden`. A `removed` unit is `shadows_removal` unless every file of its core copy is in the delete set. `tag_data_dropped` still judges mode over core.
- **P4.** The delete set is computed per collection. "Rendered" means the routed render. A `.core/` ledger entry is processed against `observed.core`. Case folding compares within one collection. A pair turned standalone deletes its `.core/` ledger files and writes the unit bare, with a `.core/` row carrying each core pre-image.
- **P5.** `pair_tag_keys` keeps its signature. It now lists a routed-type entry (not `unmanaged`) that has a bare row with a live sha outside the delete set (the residue), or a bare row with a desired sha. The second case is an unrouted plan on a pair; it was added under Rule 2 (see Deviations).
- **P6.** `write_set`: a `.core/` row is written with the core sha as its `pre`. A `.core/` delete takes its pre-image from its own row. Tree bytes are read by collection path. `new_ledger` is unchanged.
- **P7.** Hosts treat `config/resources/core` as a second root that must already be a real directory and is never created. Snapshot and staging use `core/` slots; the local restore staging uses a `restore-core/` slot. `emptied_dir_floor` is 3 on the collection path for a `.core/` key. Removed-dir reports count levels on the collection path and name core dirs with their `.core/` key.
- **P8.** `FILE_NORMALIZATION` gains the row `("unary-resource.json", &[])`. `converge` and `preflight::backup_blocked_units` map deletes to their collection path. The live run found `udts.json` byte-equal on gw-b, so it needs no row.
- **P9.** `pair_tags_restore_refusal`'s logic is unchanged. A core-only run is rollbackable (`rollback_core_only_run_allowed_on_pair`). A migration run is refused (`rollback_migration_run_refused_on_pair`). The hint says to apply again.
- **P10.** The local host overrides `probe_paths`. The sftp host's probe work list carries one `P` record per core tag type touched (`WorkList::for_probe_paths`), so `commit.sh` and `commit.ps1` check `core` (with `-w` and `CanAddFile` respectively). The codes map through `transport::core_probe_problem`.
- **P11.** Both verify plan calls use `plan_with(.., Redundancy::of(target))`, and a pair gets the `pair_mode_tags_warning` line. `--node both` carries it once; `--node backup` does not carry it.
- **P12.** A migration is destructive with `tag_data_loss`. `hold_budget` counts each moved file twice, with no code change.
- **P13.** `pull.rs` is unchanged. `pull_pair_core_tags_fixpoint` pins that a pulled pair tree is a `plan_with(Pair)` fixpoint.
- **P14.** `apply_base` strips `.core/` before the base lookup. `render_diffs` reads `.core/` rows from `observed.core`.
- **P15.** The `deploy_pair_tags_refused` message, hint and plan warning are narrowed to the residue and to mode restores. The slug is unchanged. README exit-table row 6 now reads "a tag unit a deploy would leave or restore in a redundant pair's mode collection".

## RED evidence

- **Task 1.** The tests were written alongside the implementation. RED was then taken by switching routing off inside `plan_with` (`let redundancy = Redundancy::Standalone;`), running the new tests and restoring the file.
  - Failed (9 plan tests): `core_route_added_unit_rows`, `core_route_same_against_core`, `core_route_changed_never_hidden`, `core_route_migration_rows_and_deletes`, `core_route_removed_core_unit`, `core_route_case_fold_per_collection`, `core_route_drift_on_core_copy`, `core_route_standalone_ledger_core_entry`, `core_route_tag_type_definition_unit`.
  - Failed (7 engine tests): `pair_core_added_unit_lands_in_core`, `pair_core_changed_unit_replaces_core_copy`, `pair_core_migration_moves_mode_unit_in_one_commit`, `pair_core_migration_needs_allow_destructive`, `pair_core_auto_rollback_restores_both_collections`, `pair_core_missing_core_root_writes_nothing`, `pair_prepare_refuses_a_tag_unit_and_writes_nothing`.
  - Passed even with routing off, as expected: `core_route_residue_refused`, `core_route_ambiguous_removal_shadows`, `core_route_standalone_equals_plan`, `core_route_tag_free_pair_equals_standalone` and `pair_core_removed_unit_deletes_core_copy`. P4 deletes a `.core/` ledger entry on a standalone target too, which is why the last one passes.
- **Task 2.** RED was genuine: the conformance rows ran before the script hosts and the local probe were implemented.
  - Failed: `conformance_local_core_probe_checks_core_root` and every `conformance_sh_core_*` except `core_root_absent_never_created`.
  - `conformance_local_core_replace_checks_pre_image` also failed, but from a bug in the test itself (a second commit after a restore met the restoring journal). The row was reordered, not loosened.
- **Task 3.** With routing off in verify, `pair_verify_warns_on_mode_tag_files` failed and `pair_verify_core_units_green` and `standalone_verify_no_pair_warning` passed. `pull_pair_core_tags_fixpoint` passed at once, as it pins unchanged code (P13).

## Gates

- `T1-OK` printed at `24ed482`.
- `T2-OK` printed at `2835ed3`. This includes `cargo xwin clippy` for windows-msvc and the pwsh parse of `commit.ps1`.
- `FULL-GATE-OK` was printed before the Task 3 commit (see the final report).
- `cargo test --workspace`: every result `ok`.

## Live

The rig was the Docker 8.3.6 pair (compose project `ign-pair-rig`, gw-a on `http://localhost:18300`, gw-b on `http://localhost:18301`).

The rig was brought up from `crates/ignition-core/tests/fixtures/live-rig-pair`:

- `./setup-pair.sh up && ./setup-pair.sh mint && ./setup-pair.sh pair` (2026-11-02T20:53Z to 20:57Z). `pair` hung under bash 5.3 (see Deviations).
- `docker compose -p ign-pair-rig restart gw-b` and `/bin/bash ./setup-pair.sh link` gave `peerConnected true after 41 s`.
- `/bin/bash ./setup-pair.sh mode` took 205 s and ended 2026-11-02T21:24:40Z.
- `setup-pair.sh status` then read: gw-a Master Active, gw-b Backup Cold, both Good, mode production.

The gates ran from 2026-11-02T21:25:12Z to 21:32:06Z. Values came from `~/.cache/ign-pair-rig/tokens.env` and were never printed:

```text
IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 \
  IGN_PAIR_TOKEN_A=<tokens.env> IGN_PAIR_TOKEN_B=<tokens.env> \
  IGN_PAIR_DATA_A=$HOME/.cache/ign-pair-rig/data-a IGN_PAIR_DATA_B=$HOME/.cache/ign-pair-rig/data-b \
  PATH=<scratchpad>/sysbash:$PATH \
  cargo test -p ignition-core --test live_deploy_pair -- --ignored --show-output --test-threads=1 \
  2>&1 | tee <scratchpad>/261102-cef-live.log
```

The log has one `LIVE-RAN <gate>` line and one `test <gate> ... ok` line for each of the eight gates:

```text
test live_pair_converges ... ok
test live_pair_converges_with_a_stale_signature ... ok
test live_pair_core_migration_converges ... ok
test live_pair_core_tag_units_converge ... ok
test live_pair_refuses_a_tag_unit ... ok
test live_pair_secret_names ... ok
test live_pair_tag_cleanup_converges ... ok
test live_pair_unsynced_then_reconverge ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 394.47s
```

### Observations

- **`live_pair_core_tag_units_converge`** (21:27:40Z to 21:28:06Z):
  - The core apply ended `Applied` in 6 s and wrote the four `.core/` files plus the tracer. The backup converged with `waited_s 0`, no resync and `files_checked 8`.
  - On gw-b, `tags.json` and `udts.json` were byte-equal. Both `unary-resource.json` files were parsed-equal, not byte-equal: gw-b re-serialized them.
  - gw-b logged no snapshot mismatch and was never sync-exhausted. Both nodes read Good.
  - The non-tag change converged with `waited_s 0` and no resync.
  - The removal apply deleted the four `.core/` keys and converged with `waited_s 0` and no resync. gw-b dropped its copies and both nodes read Good.
- **udts.json (P8):** gw-b keeps `udts.json` byte-equal, so no `("udts.json", &[])` row was added. gw-b re-serializes `unary-resource.json` in both tag types, which the new row covers.
- **Floor 3 (A3):**
  - After the core removal, `core/ignition/tag-definition` and `core/ignition/tag-type-definition` stayed on both nodes.
  - Their `default/` provider folders stayed too, because the rig's `default` provider holds its own provider-root unit there. So only the unit folders were emptied and removed live.
  - The pair kept syncing afterwards (no mismatch, no resync, both Good).
  - Removing an emptied provider folder in `core` is covered only by the fake pair (`pair_core_removed_unit_deletes_core_copy`) and the conformance rows, not live.
- **`live_pair_core_migration_converges`** (21:26:18Z to 21:27:40Z):
  - The pre-A2 seed wedged the pair: the snapshot mismatch was logged after 2 s and TEC-5105 after 12 s, with both nodes in SyncError. This proves the mismatch line is visible to the log check.
  - The one migration apply ended `Applied` in 57 s. It wrote the four `.core/` keys and deleted the four bare mode files, and both mode tag type folders were gone. The backup converged after the one resync (`waited_s 50`, resync ok) with `files` pass.
  - The on-box ledger lists the `.core/` keys and no bare tag path.
  - The following non-tag change converged with `waited_s 0`, no resync and both nodes Good, and gw-b logged no mismatch afterwards.
- **`live_pair_refuses_a_tag_unit`** (residue):
  - The plan warned once. The apply was refused `deploy_pair_tags_refused` (exit 6) with "1 tag unit(s) would leave files in, or restore files to, the production collection of a redundant pair ...; nothing was written".
  - gw-a's mode collection and `core` were unchanged, gw-b received no request, and the pair stayed settled. The cleanup removed the file and sent a resync.
- **Teardown:** `docker compose -p ign-pair-rig down -v` at 2026-11-02T21:32:44Z. No `ign-pair-rig` container is left.

Live verdict: pass

## Deviations from Plan

### Execution-shape deviations

1. **All three tasks in one executor run.** The plan asks for one run per task. The orchestrator's instruction asked for all three tasks in order. Each task still ended in its own commit, after its own gate.
2. **`transport/batch.rs` validation moved into Task 1** (Rule 3). The local host builds its work list through `WorkList::from_spec`, and the T1 gate greps `batch_core_key_round_trip`. W/D/P paths and restore paths are validated with `apply_key_problem` from Task 1 on; the mirror stays on `mirror_path_problem`. The script hosts still refused `.core/` keys in their own `check_work` until Task 2.
3. **RED for Task 1 was taken after the code** (see RED evidence), by switching routing off rather than strictly test-first. Task 2's RED was taken before its implementation.
4. **Live run flag: `--show-output` instead of `--nocapture`.** Under `--nocapture`, libtest prints `test X ... ` before the gate's own output and `ok` after it, so the full gate's grep for `^test X ... ok$` can never match. `--show-output` keeps that line whole and still prints every gate's captured `LIVE-RAN` line in the successes section.
5. **The migration gate waits for the snapshot mismatch and then for TEC-5105** (the plan says "or"). This keeps the apply from starting while gw-b is still retrying, which could otherwise exhaust inside the apply's backup wait. It is stricter, not looser.

### Auto-fixed and added

1. **[Rule 2] An unrouted plan on a pair is refused.** `pair_tag_keys` also lists a routed-type entry that has a bare row with a desired sha. A plan made for a standalone target (the WR-04 engine-guard case) would otherwise write tag units into a pair's mode collection, because the residue rule alone does not catch it. Pinned by `pair_tags_unrouted_plan_is_refused_on_a_pair` and by the retargeted `pair_run_apply_refuses_a_tag_plan_that_skipped_the_gates` (both the residue plan and the unrouted plan are refused).
2. **[Rule 3] New helpers in `transport/mod.rs`:** `core_tag_types` and `core_probe_problem`, which the local and sftp hosts share. The file is not in Task 2's list.
3. **`plan::pair_mode_tag_files`** landed in the Task 1 commit for Task 3's verify warning.
4. **Interpretation of `windows_long_path_problems` for core keys.** A `.core/` key is measured below its real `core\` snapshot and staging slot, using its collection path. The live `config\resources\core\` path length is not checked here, the same as for mode paths today; the read side reports such a unit unreadable.
5. **A local restore of a core key when `core` is missing is a reported failure,** not a silent skip, the same as the scripts' `the core collection does not exist`.

### Environment (pair rig, out of scope; see deferred-items.md)

- **bash 5.3 deadlock.** `setup-pair.sh pair` deadlocked under Homebrew bash 5.3.15: a sample showed a here-string `write` blocked on its own pipe. gw-b was restarted, the pair was linked and moded with `/bin/bash`, and the live gates ran with a PATH shim whose `bash` is `/bin/bash` (the gates call `bash setup-pair.sh link|cut`). No repo file was changed for this.
- **Leftover process.** The first `setup-pair.sh pair` process tree stays blocked in that write, and the agent session was not allowed to stop it. Stop it with `pkill -f "setup-pair.sh pair"`.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. The new surface (core writes, the core probe and the `.core/` key in on-box documents) is T-cef-01 to T-cef-07, each mitigated as planned.

## Open items (not done here)

- **Windows ACL capture (O4, A4).** A Windows deploy account with `Modify` on `config\resources\core\ignition\tag-definition` and `...\tag-type-definition` has not been proven, on a real host, to replace and delete the gateway-owned core files. Capture it before production use. The `write_host` probe refuses before any write when the grant is missing.
- **The production pair still holds mode-collection tag paths.** Move them with a migration apply (`--allow-destructive`), or follow runbook T6e, under the operator's prod-backups-first rule: a VM snapshot and a `.gwbk` of both nodes first.
- **Resync-only convergence warning,** carried from the debug doc: warn when a backup converged only after the apply's resync while `projectState` reads SyncError.
- **The `conformance_ps_core_*` rows** compile under `cargo xwin clippy` here and run on the Windows CI leg only.
- **A3 live gap.** Removing an emptied core provider folder was not exercised live, because the rig's `default` provider folder is never empty.
- **Rig driver fix.** Make `setup-pair.sh` safe under bash 5.3 (no large here-strings) and stop the leftover process (deferred-items.md).
