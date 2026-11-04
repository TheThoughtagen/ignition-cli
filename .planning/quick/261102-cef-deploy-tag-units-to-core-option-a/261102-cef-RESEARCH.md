# Quick 261102-cef: Deploy tag units to core on redundant pairs (A2) - Research

**Researched:** 2026-11-02
**Domain:** ign deploy write path (plan / apply / hosts / ledger / rollback / pair verify), codebase integration
**Confidence:** HIGH for the integration map (every file read this session); MEDIUM for the recommended representation (a design choice, not a captured fact); LOW for the two unproven live behaviours flagged in Open Questions.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
**D1 Where core routing applies: A2, pairs only**
- Route tag-definition / tag-type-definition units to `core` only when the target has `nodes.backup`. Standalone targets are unchanged.
- One routing rule keyed off the target's redundancy (the same signal option C's `Redundancy` uses), applied consistently by plan, apply/transport, ledger, rollback, pull, and verify (including the pair files check reading these units from `core` on gw-b).
- Rationale: fixes the observed Ignition limitation without changing working standalone behaviour or migrating standalone mode tags; easy to drop if Ignition later syncs mode-collection tags.

**D2 Designer edits to ign-managed core tag folders: refuse as drift**
- Same rule as every other resource: plan reports drift, apply refuses until the repo is updated or the operator overrides through the existing mechanism. Never silently overwrite.

**D3 Tag units already in a pair's mode collection: move in the same apply**
- Write the unit to `core` and delete the mode copy (and its emptied type folder, per option C's cleanup) in one staged commit + scan, so the pair converges in one step.
- `deploy_pair_tags_refused` remains only for cases the move cannot handle (e.g. residual mode files ign cannot delete, as in the 261102-9t6 F2 fix); planner decides the exact residual cases.

### Claude's Discretion
- How collection routing is represented (per-path collection on plan entries vs routing in layout).
- gw-b re-serialization of unary-resource.json in core (parsed-JSON comparison like K2, or writing the gateway's canonical form).
- CoreRelation / ShadowsRemoval semantics for pair tag units living in core.
- Keeping plan_sha256 goldens stable for trees without tag units and for standalone targets.

### Deferred Ideas (OUT OF SCOPE)
None listed in CONTEXT.md. Constraints from `<specifics>`: no production calls; no git worktrees; branch feat/deploy-tags-core; never stage .planning/config.json, .planning/research/.cache/, svc-ignition-deploy@52.154.75ace-uat4.ps1; no co-author lines; no production host names or IPs in committed files; no Cargo.toml version bump; four-place rule + README exit table for any slug change; do not push or open a PR. Gates: cargo fmt --check, workspace clippy -D warnings, cargo test --workspace, cargo xwin clippy windows-msvc.
</user_constraints>

## Project Constraints (from CLAUDE.md)

- Never share a cargo target dir between checkouts; no worktrees (`workflow.use_worktrees=false`). If phantom E0432 / vanished tests appear, rebuild in an isolated `CARGO_TARGET_DIR` before debugging code.
- `ign` on PATH is the installed binary; `cargo install --path crates/ignition-cli --force` before any manual smoke check.
- `thiserror` typed errors, stable slugs; exit codes live in `crates/ignition-core/src/error.rs` and the README exit table (pinned by `readme_exit_table_agreement`, `exit_code_mapping_enumerated`). A2 keeps the slug `deploy_pair_tags_refused` (exit 6), so no slug change is needed; only its message/hint and README prose change.
- Every subcommand non-interactive with `--json`; JSON keys are an additive contract (`crates/ignition-cli/tests/contract_deploy.rs:607-624` pins the plan data key set exactly).
- Python: never pip install globally (not relevant here).

## Summary

Today every apply write is collection-relative to ONE root, `config/resources/<mode>/`. That assumption is not a single switch: it is baked into `WriteOp`/`DeleteOp`/`BatchSpec` (`apply/host.rs:59-125`), the journal and record `touched` lists (`journal.rs:357-366` validates each entry with `mirror_path_problem`), the fingerprint keys (`fingerprint.rs:22`), the work list (`transport/batch.rs:10-25`, W/D/P records carry a bare path), both host scripts (`commit.sh check_roots` at 395-399 refuses any root but `*/config/resources/"$MD"`; `rmdir_up` at 755-760; `commit.ps1 RmdirUp` 693-698), the snapshot/staging layout (`snapshots/<run>/files/<path>`, `staging/<run>/files/<path>`), the ledger (`ledger.rs` "the mode-collection files that apply wrote"), and plan's delete set (`plan.rs:596-678`, restricted to `observed.mode`). Reading is already fine: `TreeReader::read` returns both `core` and the mode collection within scope, so plan, pull and verify see core tag folders today.

The one case that forces collection info into every touched-path structure is D3: a migration writes `core/X` and deletes `<mode>/X` for the SAME collection-relative path X in one commit. With bare paths that collides in `BatchSpec::check` ("is both written and deleted", host.rs:179-181), `validate_mirror_paths`, the sorted-unique `touched` list, the fingerprint map and the snapshot `files/X` slot. Outside migration a path never lands in both collections, because routing is a pure function of (redundancy, path type).

**Primary recommendation:** Keep render and its `render_sha256` unchanged. Add one pure routing rule next to `is_pair_unsynced_tag_path` in `layout.rs`. Spell a core-collection path everywhere in the write half as a reserved qualified key, `.core/<collection path>` (proposed; any spelling every existing validator refuses would do). That covers plan entry files, the delete set, the ledger, write/delete ops, journal/record/fingerprint keys and work.tsv paths. Mode paths stay bare. Standalone and tag-free plans, ledgers, journals, records, work lists and fingerprints are then byte-identical to today, so `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE` and the fingerprint `GOLDEN` hold. Migration falls out of the existing generic rule "ledger minus rendered, restricted to what exists": the old ledger says `X` (mode), the routed render says `.core/X`, so mode `X` is deleted and `.core/X` written.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|---|---|---|---|
| Routing rule (pair + tag type -> core) | `deploy/layout.rs` (pure) | `deploy/target.rs` (`Redundancy` source) | One definition; plan, apply, hosts, rollback, verify call it |
| Plan: status vs core copy, migration deletes, residue | `deploy/plan.rs` (pure) | `actions/deploy.rs` (passes redundancy) | Plan already owns delete set and `plan_sha256` |
| Gates (`deploy_pair_tags_refused` for residue only) | `deploy/apply/gates.rs` | `actions/deploy.rs` plan warning | Shared `pair_tag_keys` keeps gate, engine re-check and warning in agreement |
| Write ops / batch spec / ledger swap | `deploy/apply/mod.rs` (`write_set`, `new_ledger`, `batch_spec`) | `apply/host.rs` (`BatchSpec::check`) | Engine decides, host executes |
| On-box file moves in two roots | hosts: `transport/local/apply_ops.rs`, `transport/sftp/apply_ops.rs` + `scripts/commit.sh`, `scripts/commit.ps1` | `transport/batch.rs` (work.tsv) | Same conformance table (`tests/deploy_host_conformance.rs`) for all three |
| Pair `files` check (gw-b core) | `deploy/pair.rs` (`compare_files`, `FILE_NORMALIZATION`) | `apply/converge.rs`, `actions/deploy.rs` verify | gw-b is read-only; effective view already overlays core |

## Standard Stack

No new crates. Everything uses existing workspace dependencies (serde_json for the parsed-JSON comparison, sha2, the existing hosts). **Package Legitimacy Audit:** not applicable, because nothing is installed.

## Integration Map (where "mode only" is assumed)

All references below were read this session (`[VERIFIED: <path>:<lines>]`).

| # | Location | Today | A2 change |
|---|---|---|---|
| 1 | `deploy/target.rs:170-181` | refuses `mode: core`/`local` | **Keep.** The mode name stays a mode; core routing is per path, not a mode. |
| 2 | `transport/mod.rs:57` `pub const CORE_COLLECTION: &str = "core";`; `:590-600` `mirror_mode_problem` refuses `mode == CORE_COLLECTION \|\| mode == "local"` | write half never names core | Keep for the *mode* argument. Add a second, fixed core root derived from the same data dir (`config/resources/core`); never from user input. |
| 3 | `transport/mod.rs:607-627` `mirror_path_problem` (>= `MIRROR_MIN_COMPONENTS` = 3, no dot component); `:683-730` `validate_mirror_paths` ("both staged and in the delete set") | bare paths | Split the `.core/` qualifier first, validate the remainder with the unchanged rules. Only `ignition/tag-definition/...` and `ignition/tag-type-definition/...` may be qualified. |
| 4 | `transport/mod.rs:643-649` `emptied_dir_floor`: `if is_pair_unsynced_tag_path(path) { 2 } else { MIRROR_MIN_COMPONENTS }` | floor 2 for mode tag paths | Core paths: floor 3 on the unqualified remainder; never remove `core/ignition/<tag type>`. The script copies (`commit.sh:759`, `ignition/tag-definition/* \| ignition/tag-type-definition/*) _floor=2`, and `commit.ps1:695-698`) must branch on collection, and the Rust re-checks of reported dirs (`sftp/apply_ops.rs:681-687`, `:959`) must use the same function. |
| 5 | `layout.rs:98` `pub const PAIR_UNSYNCED_TAG_TYPES: [&str; 2] = ["tag-type-definition", "tag-definition"];`, `:103-109` `is_pair_unsynced_tag_path` | predicate for the refusal and the floor | Add the routing fn beside it: `route(Redundancy::Pair, p)` qualifies p when `is_pair_unsynced_tag_path(p)`; standalone returns p. |
| 6 | `apply/gates.rs:40-56` `enum Redundancy { Standalone, Pair }`, `Redundancy::of(&Target)` | in apply layer | Move it to `target.rs` (or `layout.rs`) and re-export it from gates; `plan.rs` needs it and must not depend on `apply`. |
| 7 | `plan.rs:737-` `plan(rendered, observed, ledger, scope, excluded)`; callers `actions/deploy.rs:410`, `:1195` (verify), `:1568` (verify --node both), `plan.rs:1652` (tests) | effective = mode over core (`:805`), status vs effective | Add a `redundancy` parameter. For routed units: compare desired with the **core** copy; emit `files` with `.core/` paths (core copy + desired) **plus** the live mode-copy files as bare-path rows (live sha, desired `-`). Those rows are the pre-images of the migration deletes and the residue that refuses. `tag_data_dropped` keeps using the runtime view (mode over core, `:805`, `:861`), because that is what "drops data the gateway runs now" means. |
| 8 | `plan.rs:596-678` `delete_set` | ledger minus rendered, `!observed.mode.contains_key` skips, case-fold skip vs rendered | Operate on qualified paths: a bare ledger path checks `observed.mode`, a `.core/` path checks `observed.core`. "Rendered" means the routed (qualified) render set, so a mode `X` with a routed `.core/X` render IS deleted (migration). The case-fold skip compares within one collection only. Group `deleted_here` with the matching layer (mode or core). |
| 9 | `plan.rs:821-838` `CoreRelation` (`ShadowsRemoval` if removed with core copy; `Hidden` for tag folders "a rendered tag unit always lands in the mode collection") | | Routed units: never `Hidden`. `ShadowsRemoval` keeps today's meaning (mode copy deleted while a core copy stays and is not deleted) and still gates `deploy_blocked`. That is exactly the ambiguous case of a pre-A2 ledger with a Designer core copy. A routed removed unit whose core files are all in the delete set gets the computed relation (`differs`), not `shadows_removal`. |
| 10 | `plan.rs:465-513` `pair_tag_keys(entries, delete)` | refuses added/changed/kept-in-mode, and removed with residue | New rule: a routed-type entry (not `unmanaged`) with any bare-path (mode) file that has a live sha and is not in the delete set. Same signature, so `gates.rs:64-77`, `apply/mod.rs:832` and `actions/deploy.rs:460` stay in agreement. Added/changed/same no longer refuse by themselves. |
| 11 | `plan.rs:1471-1515` `canonical_text` (E/F/D/U lines) | | **No grammar change.** `.core/` paths appear in F and D lines only for pair plans with tag units, so standalone and tag-free hashes are unchanged, and a routed plan never hashes like the standalone plan of the same tree. |
| 12 | `plan.rs:1250-1263` `apply_base` (drift, D2) | base lookup by `file.path` | Strip `.core/` before the base lookup (the base render is unrouted). Then D2 is today's mechanism unchanged: `--base` + `--allow-drift` (`gates.rs:178-183`). |
| 13 | `plan.rs:1303` `render_diffs` | live side from `observed` by path | Map `.core/` rows to `observed.core`. |
| 14 | `apply/mod.rs:264-319` `write_set` | `pre` only when `live_in` has mode; delete needs `Some((true, Some(pre)))` | Writes: `.core/` path, `pre` = the row's live sha (core). Deletes: look up the row by qualified path; a `.core/` delete's pre is the core sha. Read the tree bytes by the unqualified path. |
| 15 | `apply/mod.rs:322-350` `new_ledger` | old minus deletes plus writes | Unchanged logic over qualified strings; the ledger then records `.core/X` after a migration. |
| 16 | `ledger.rs:270-` `check_files` (dot components refused, declared type, case-variant) | bare mode paths only | Accept `.core/` + a `PAIR_UNSYNCED_TAG_TYPES` path; run the case-variant check per collection. An older binary refuses such a ledger (`deploy_ledger_invalid`), which is the fail-closed downgrade. |
| 17 | `apply/host.rs:131-240` `BatchSpec::check`, `:357-` `ApplyHost` docs; `journal.rs:357-366` `check_touched`; `record.rs:85-92` `TouchedEntry` | bare paths | Accept qualified keys (validate the remainder). `LEDGER_KEY` (`fingerprint.rs:22`, `"ledger.json"`) cannot collide. |
| 18 | `transport/batch.rs:566-627` `to_lines` / `parse_lines` (W/D/P carry `path`) | | Keep the format; the path field carries the qualified key. Scripts map `.core/*` to `${CO%/*}/core` (CO is already verified to end in `/config/resources/$MD`). |
| 19 | hosts: `local/apply_ops.rs:163-189` `collection()` opens `config/resources/<mode>` (creates it); `:876-910` `remove_emptied`; `:660` snapshot `files/<path>`; `commit.sh` `snapshot_all` 595-611 (`$DE/snapshots/$RUN/files/$d`), `move_one` 684-735, `deletes` 795-820, `restore_one` 858-950, `op_fingerprint` 1016, `op_probe` 1036; `commit.ps1` `SnapshotAll` 563, `MoveOne` 646, `Deletes` 732, `RestoreOne` 850, `OpFingerprint` 1009, `CheckRoots` 407 | one root | A second root `core` that must already exist (never create `config/resources/core`), with the same symlink/real-dir checks. Snapshot and staging slot `core/<path>` beside `files/<path>`: a dot dir under the snapshot is refused by `fetch_snapshot` (`local/apply_ops.rs:1345`, `component_problem`). Same mapping in `apply/mod.rs:1680-1694` `snapshot_warnings` and `sftp/apply_ops.rs:101-139` `windows_long_path_problems` (its prefixes hard-code `\\files\\`). |
| 20 | `apply/preflight.rs:311` `write_host` -> `probe_paths(touched)`; README 3893-3900 ("needs `Modify` on the mode collection") | probes mode + ign-deploy only | Probe core writability when `touched` holds a `.core/` key. **New host grant needed** (see Runtime State). |
| 21 | `apply/preflight.rs:918-942` `backup_blocked_units`, `apply/converge.rs:333-350` `present`/`absent`, `actions/deploy.rs:1578-1581` | bare `RelPath`s over effective (mode over core) maps | Strip `.core/` when building `absent`. A migrated mode `X` stays out of `absent`, because the master's effective map still holds `X` from core (the existing filter "a copy in another collection keeps it live", converge.rs:342-349). `present` stays the bare render keys. |
| 22 | `deploy/pair.rs:721-727` `FILE_NORMALIZATION` = one row `("resource.json", &["attributes.lastModification", "attributes.lastModificationSignature"])` | | Add the row `("unary-resource.json", &[])` (parsed JSON, every member counts). See Pitfall 4. |
| 23 | `apply/rollback.rs:317-339` `pair_tags_restore_refusal` (touched with pre + `is_pair_unsynced_tag_path`) | | Unchanged code, correct semantics: `.core/` keys do not match the predicate, so a core restore is allowed. A migration run's mode pre-image would be restored into the mode collection, so its manual rollback stays refused. Update the message/hint and README. |
| 24 | `deploy/pull.rs:222-235` `place`, `:263-` | core files go to base `tags/`, mode files to `modes/<env>/` | **No change.** After A2 a pair's tags live in core and pull to the base tree. The render is unrouted and plan routes it. |
| 25 | `apply/gates.rs:205-259` `confirmation_prose` ("delete set: N file(s)") | | Counts qualified deletes too; consider naming the core/mode split. |

## Architecture Patterns

### Data flow (pair target, routed unit)

```
repo tags/<P>/tag-definition/X/*  --render (unchanged, render_sha256 unchanged)-->  ignition/tag-definition/P/X/*
        |
        v  route(Redundancy::of(target), path)   [layout.rs, pure]
.core/ignition/tag-definition/P/X/*  (desired)        observed.core / observed.mode (TreeReader::read, unchanged)
        |                                                   |
        +---------------- plan() --------------------------+
             status vs core copy; mode-copy rows; delete set = ledger - routed render (per collection)
             -> plan_sha256 (F/D lines carry .core/ only here)
        |
gates: plan_stale -> unreadable -> pair residue (deploy_pair_tags_refused) -> noop -> shadows_removal -> destructive/drift
        |
write_set: W .core/X (pre = core sha)   D X (pre = mode sha)   D .core/Y (pre = core sha)
        |
host commit (one call, under the scan-lock): snapshot files/ + core/, move into core root, delete in both roots,
         rmdir (mode tag: floor 2, core: floor 3), swap ledger (now lists .core/X)
        |
scan/config once -> verify (plan with redundancy) -> converge: gw-b effective view, files check (+ unary-resource.json row)
```

### Pattern: one qualified key, parsed at every boundary

Add a tiny pure helper (layout.rs), and have every validator call it first:

```rust
// Proposed (not in the repo yet). The spelling is a design choice [ASSUMED]:
// a leading dot component is refused by every existing validator
// (mirror_path_problem, Ledger::check_files, journal check_touched), so an
// older ign that meets a newer ledger/journal/record fails closed instead of
// writing `<mode>/core.../` (a `core:` spelling would pass those validators).
pub const CORE_KEY_PREFIX: &str = ".core/";
pub enum Coll { Mode, Core }
pub fn split_key(key: &str) -> (Coll, &str) { /* strip CORE_KEY_PREFIX */ }
pub fn route(r: Redundancy, path: &RelPath) -> String {
    if r == Redundancy::Pair && is_pair_unsynced_tag_path(path.as_str()) {
        format!("{CORE_KEY_PREFIX}{path}")
    } else { path.as_str().to_string() }
}
```

`unit_key`, `group`, `in_scope`, `is_tag_folder` and `tag_data_dropped` keep working on bare paths. Qualify only when emitting `PlanFile.path`, the delete set and the ops.

### Alternatives considered (rejected)

| Instead of | Could use | Why not |
|---|---|---|
| qualified key | `collection` field on WriteOp/DeleteOp/TouchedEntry + work.tsv column + ledger `core_files` | More structs, serde and format surface; the journal `touched` is `Vec<String>`, so a string spelling is needed anyway |
| routing after plan | routing in render (`--out` writes core paths) | Changes `render_sha256` and every render golden; render is target-redundancy-agnostic today |
| derived ledger (bare tag paths mean core on a pair) | | Ambiguous for pre-A2 ledgers: it could delete a Designer core copy ign never wrote. The qualified ledger records where ign wrote, so a pair->standalone change (or dropping A2 later) migrates back by the same generic rule |
| resources-relative paths (`<coll>/<path>`) everywhere | | Rewrites every standalone journal/record/ledger/golden |

## Don't Hand-Roll

| Problem | Don't Build | Use Instead |
|---|---|---|
| Comparing gw-b's re-serialized marker | a whitespace/key-order normalizer | a `FILE_NORMALIZATION` row (`pair.rs:721`, `same_document` 852-873 parses with serde_json) |
| Drift on core tag folders (D2) | a new core-drift check | `apply_base` + `--allow-drift` (`plan.rs:1250`, `gates.rs:178`) over the core-compared rows |
| Migration as a special code path | a separate "move" op | the generic delete-set rule over qualified keys (old ledger `X` minus routed render `.core/X`) |
| Pair residue detection | a new host read | `pair_tag_keys` over the mode-copy rows plan already emits |

## Runtime State Inventory

| Category | Items Found | Action Required |
|---|---|---|
| Stored data | On-box ledgers `ign-deploy/<gateway>/<mode>/ledger.json` (account home on ssh nodes, `ledger.rs:36-45`) that list pre-A2 tag paths as bare (mode) entries, for example the canary `ignition/tag-definition/default/Deploy/*` on the production pair if runbook T6e removed the files by hand. | Code only: a bare ledgered tag path that exists in mode is migrated or deleted by the generic rule. A bare ledgered path whose mode file is gone is skipped (`!observed.mode.contains_key`), so a Designer core copy of that name is never deleted. No data migration script. |
| Live service config | Production pair mode collection may still hold tag paths (wedge). | Out of scope here (no prod calls); the A2 migration apply or runbook T6e handles it later. |
| OS-registered state | **Host ACLs.** README 3893 / 3260-3262: the Windows deploy account has `Modify` on the mode collection only; README 3902-3905: on Linux, rwx on each mode collection only. A2 writes `config/resources/core/ignition/tag-definition` and `.../tag-type-definition`. | Document a new grant (core tag type folders, both nodes for failover symmetry) and probe it in `write_host`. The Docker pair rig uses `local` transport with bind mounts, so it will NOT exercise this. |
| Secrets/env vars | None. Verified: the routing key is `nodes.backup`; no env or secret names change. | none |
| Build artifacts | None: no crate rename, and `include_str!` scripts are rehashed per build (`batch.rs:63`, `:80`). | none |

## Common Pitfalls

1. **Same path written and deleted (D3).** `BatchSpec::check` (host.rs:179-181), `validate_mirror_paths` and the snapshot `files/X` slot all collide on bare paths. Qualify, and give the snapshot and staging dirs their own `core/` slot.
2. **Delete-set skips that assume one collection.** `rendered.files.contains_key(&path)` and the case-fold skip (`plan.rs:660-668`) would silently keep the mode copy of a routed unit. The result is residue and a refused apply on every migration. Compare within one collection.
3. **Mode-delete pre-images.** `write_set` takes pre-images from the entry's files. If routed rows carry only core shas, a migration delete gets the wrong `pre` (`plan_stale` under the lock) or none (`CoreError::Internal`). Emit the mode-copy rows.
4. **gw-b re-serializes `unary-resource.json` in core.** The C1 control showed it: `tags.json` byte-equal, the marker "equal as parsed JSON (gw-b pretty-prints it)" (debug doc, Evidence C1). Without a row, every healthy pair tag apply fails `files` with `file_differs`. Recommend the K2-style row `("unary-resource.json", &[])` over writing the gateway's pretty form: writing that form would alter repo bytes and break the pull/render fixpoint. `udts.json` in core on a pair was never captured (C1 used tag-definition only). The live gate must include a tag-type-definition unit.
5. **Folder override semantics.** A mode copy hides its core folder whole (CD5a). On a migration, what runs before the commit is the mode copy, and after it the core copy. So `tag_data_dropped` must judge against mode over core, while status and drift judge against core. A hidden stale core copy (Designer) shows as `changed`, and with `--base` as drift (D2 working as intended).
6. **Ambiguous removal (pre-A2 ledger, mode copy + Designer core copy).** Deleting the mode copy un-hides the core copy. Keep `ShadowsRemoval` -> `deploy_blocked` here; never delete a core file the ledger does not list as `.core/`.
7. **Emptied dirs in core.** The floor-2 rule exists only for mode (an empty mode type folder wedges the pair). Applying it to core would remove `core/ignition/tag-definition`. The removed-dir reports are `[index, levels]` and Rust re-derives names (`first_components`, `sftp/apply_ops.rs:141`). Pick one convention: levels counted on the unqualified remainder.
8. **Rollback.** An automatic restore of a failed migration run puts the mode copy back. That is the pre-apply state, so the pair is wedged again; document it. A manual `rollback --run` of a migration run stays `deploy_pair_tags_refused` (rollback.rs:317). A run that only wrote core is rollbackable. Interrupted-run recovery is never refused.
9. **Case folding (Windows).** A routed write and a mode delete of the same name live in different dirs, so there is no clobber. Within core, a Designer `deploy/` versus a repo `Deploy/` resolves to one NTFS dir. The pre-image check then turns it into `plan_stale`, as it does in mode today; the ps1 floor test is `StartsWith(..., Ordinal)` on ign's own spelling.
10. **Hold budget.** A migration touches each moved file twice (write + delete) against the 1,528-file `hold_budget` preflight.
11. **Live harness cleanup.** `sweep_a` (`live_deploy_pair.rs:460-474`) removes only mode tag type dirs, and `units_on_b` (`:447-454`) checks only the mode collection. A2 gates leave `core/ignition/tag-definition/default/Deploy` on both nodes. Sweep exactly the gate's canary dirs in gw-a core (never the whole core type folder: the rig holds `System` provider folders there), scan, and wait for gw-b's core copy to go (C1: a core delete propagates via `tag-config-resource`).
12. **Probe and Windows path budget** hard-code the mode collection and the `\\files\\` prefix (`sftp/apply_ops.rs:101-139`). Core copies need the same check against their real slot.

## Code Examples

Existing residue rule to generalize (`plan.rs:489-512`, verbatim excerpt):

```rust
PlanStatus::Removed => {
    e.live_in.iter().any(|c| c == "mode")
        && e.files
            .iter()
            .any(|f| f.live_sha256.is_some() && !deleted.contains(f.path.as_str()))
}
```

A2 form: apply that predicate to any non-`unmanaged` routed-type entry, restricted to bare-path (mode) rows; `.core/` rows never count as residue.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|---|---|---|
| A1 | The `.core/` key spelling is safe and fails closed in older binaries | Patterns | Low: alternative spellings are mechanical; check against `check_touched`/`check_files` |
| A2 | gw-b also re-serializes core `unary-resource.json` of tag-type-definition units, and leaves `udts.json` byte-equal | Pitfall 4 | `files` check red on a healthy pair; may need a `udts.json` row (capture on the rig) |
| A3 | Floor 3 in core (provider dir removable when emptied) is harmless for sync | Pitfall 7 | If an emptied core provider dir matters, use floor 4 for core tag paths |
| A4 | A Windows deploy account with `Modify` on core tag folders can replace and delete gateway-owned core files | Runtime State | Apply fails `write_failed` on prod; needs a Windows capture before prod use |
| A5 | Migration deletes count as `destructive` (`deleted_here > 0`, `plan.rs:863`), so a migration needs `--allow-destructive` | Integration #8 | UX only; the planner may exempt mode deletes whose path is re-rendered to core |

## Open Questions

1. **Is a migration "destructive"?** Today any entry with delete-set files is. Recommendation: keep the rule (one more explicit flag, no special case) and say so in README. The alternative is to exempt mode deletes re-rendered into core.
2. **Should `deploy verify` on a pair fail on mode-collection tag residue?** It does not today. It would be cheap once plan takes `redundancy`. Recommendation: a warning, not a new check id (avoids a contract change).
3. **Windows ACL capture** (A4) cannot be proven on the Docker rig. Planner: README grant text plus `write_host` probe, and flag it for a later Windows capture before production.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| cargo / rustc | all gates | yes | cargo 1.94.1 | - |
| cargo-xwin | windows-msvc clippy gate | yes | 0.23.1 | - |
| pwsh | `conformance_ps_*` rows | yes | 7.5.4 | - |
| docker | live pair gate (`setup-pair.sh up/mint/pair/mode`) | yes | 29.8.2 | live gate is `#[ignore]` + `IGNITION_LIVE_PAIR=1`; run by a human or orchestrator, not by this research |

## Validation Architecture

| Property | Value |
|---|---|
| Framework | cargo test (tokio tests), live gates `#[ignore]` |
| Quick run | `cargo test -p ignition-core --lib deploy:: && cargo test -p ignition-core --test deploy_plan --test deploy_apply --test deploy_host_conformance --test deploy_pair` |
| Full suite | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` |
| Live gate | `IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=... cargo test -p ignition-core --test live_deploy_pair -- --ignored --test-threads=1` |

| Behaviour | Test type | Where |
|---|---|---|
| Standalone and tag-free goldens unchanged | unit/golden | `tests/deploy_plan.rs` `PLAN_GOLDEN` (577), `PLAN_GOLDEN_LEDGER` (1084), `PLAN_GOLDEN_UNREADABLE` (2321); `fingerprint.rs:50` |
| Routed plan: `.core/` F lines; migration D lines; residue refusal | unit | `plan.rs` tests, `apply/gates.rs` tests (rewrite 600-800: added/changed/same no longer refused on a pair) |
| Delete set per collection, incl. case-fold, ShadowsRemoval on ambiguous removal | unit | `plan.rs` tests |
| write_set/new_ledger qualified ops; ledger parse accepts `.core/` tag paths only | unit | `apply/mod.rs` tests, `ledger.rs` tests |
| Hosts: write core, delete mode same path in one commit; restore; core floor 3; core root never created; fingerprint of `.core/` keys | conformance (local, sh, ps) | `tests/deploy_host_conformance.rs` new rows, in `scenarios()` |
| work.tsv round-trip with qualified paths | unit | `transport/batch.rs` tests |
| Pair integration: prepare/run_apply on a pair writes core, migration deletes mode + emptied type folder | integration | `tests/deploy_apply.rs` 7291-7600 (rewrite the five `pair_*` tests) |
| `unary-resource.json` parsed-equal passes `files` | unit | `deploy/pair/tests.rs` |
| Rollback: core-only run allowed; migration run refused | unit | `apply/rollback.rs:1119` area |
| Live A2 gates | live | `tests/live_deploy_pair.rs`: (a) tag apply (tag-definition + tag-type-definition) ends `Applied`, backup converged with `resync == None`, `files` pass, gw-a mode has no `ignition/tag-definition`, `BackupNode::sync_exhausted(since)` false, both `projectState` Good; then a non-tag change converges with no resync. (b) migration: pre-seed canary + ledger in gw-a mode (as `live_pair_tag_cleanup_converges` step 1), one apply of the declaring repo moves it (resync allowed once, since the pre-state is wedged), then a non-tag change converges with no resync. (c) retarget `live_pair_refuses_a_tag_unit` to the residue case (an unledgered mode file). Harness: Pitfall 11. |

Wave 0 gaps: the harness helpers `sweep_a`/`units_on_b` need core-aware variants before any new live gate. The conformance `Ctx` needs a `seed_core`/`core()` accessor.

## Security Domain

| ASVS | Applies | Control |
|---|---|---|
| V5 Input validation | yes | Qualified keys parsed once; the remainder goes through the unchanged `mirror_path_problem` / `collection_path` / `CompOk`; only the two tag types may be qualified; the core root is fixed, never from input |
| V4 Access control | yes | New core write grant scoped to the two tag type folders; `write_host` probe refuses before any write |
| V2/V3/V6 | no | unchanged |

| Threat | STRIDE | Mitigation |
|---|---|---|
| Crafted work list/journal escaping to another collection | Tampering | Scripts accept only `.core/ignition/tag-(type-)definition/...`; hash-pinned script and work list (`POSIX_STUB`, batch.rs:89-102) |
| Older binary misreading a new journal/ledger | Tampering | Dot-prefixed key fails existing validators (`deploy_run_invalid` / `deploy_ledger_invalid`) |
| Silent overwrite of Designer core edits | Repudiation/Integrity | D2 via `--base` drift gate; plan shows `changed` + `differs` |

## Sources

### Primary (HIGH, read this session)
- `crates/ignition-core/src/deploy/{plan.rs, layout.rs, target.rs, ledger.rs, pull.rs, pair.rs}`; `deploy/apply/{mod.rs, gates.rs, host.rs, journal.rs, record.rs, rollback.rs, converge.rs, preflight.rs, fingerprint.rs}`; `deploy/transport/{mod.rs, batch.rs, local/apply_ops.rs, sftp/apply_ops.rs, scripts/commit.sh, scripts/commit.ps1}`; `actions/deploy.rs`; `error.rs:895-908, 3819-3846`
- `crates/ignition-core/tests/{live_deploy_pair.rs, deploy_host_conformance.rs, deploy_plan.rs}`; `crates/ignition-cli/tests/contract_deploy.rs:600-640`
- `.planning/debug/resolved/deploy-tag-redundancy-sync.md` (Evidence C1/C3/C5, fix_options.A); `.planning/milestones/v1.6-phases/24-redundant-pair/24-LIVE-CAPTURES.md` (K2, C-P2); `.planning/quick/261102-9t6-*/261102-9t6-SUMMARY.md`; README "Tags layout", "Tags and redundant pairs", host permission notes (3255-3262, 3893-3905)

### Secondary / Tertiary
- None. No external sources were needed: the domain is this codebase plus the recorded rig captures.

## Metadata

- Integration map: HIGH (every row cites a file read this session)
- Recommended representation: MEDIUM (design choice; it keeps every standalone artifact byte-identical, which is checkable by the existing goldens)
- Live behaviour (udts.json on gw-b, Windows core ACL): LOW until captured
- Research date: 2026-11-02; valid until the deploy write path changes again
