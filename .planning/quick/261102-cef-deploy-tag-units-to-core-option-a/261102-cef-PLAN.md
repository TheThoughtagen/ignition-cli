---
phase: quick-261102-cef
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-core/src/deploy/layout.rs
  - crates/ignition-core/src/deploy/target.rs
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/deploy/ledger.rs
  - crates/ignition-core/src/deploy/pair.rs
  - crates/ignition-core/src/deploy/pair/tests.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/local.rs
  - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/batch.rs
  - crates/ignition-core/src/deploy/transport/sftp.rs
  - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/deploy/transport/scripts/commit.sh
  - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
  - crates/ignition-core/src/deploy/apply/gates.rs
  - crates/ignition-core/src/deploy/apply/host.rs
  - crates/ignition-core/src/deploy/apply/journal.rs
  - crates/ignition-core/src/deploy/apply/record.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/apply/converge.rs
  - crates/ignition-core/src/deploy/apply/preflight.rs
  - crates/ignition-core/src/deploy/apply/rollback.rs
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
  - .planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md
autonomous: true
requirements: [QUICK-cef]

estimate:
  tokens: 650000
  raw_tokens: 650000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "On a target with `nodes.backup`, `ign deploy plan` routes every `ignition/tag-definition` and `ignition/tag-type-definition` file to a `.core/<collection path>` row and compares it with the core copy. `deploy apply` then writes those files into the master's `config/resources/core/` in the one staged commit and scan. The mode collection gains no path of either type, and the backup converges (per D1)"
    - "A tag unit already in a pair's mode collection, and listed in the ledger, moves to core in one apply. The core copy is written, and the mode files and the emptied mode type folder are deleted, in the same commit. The new ledger lists the `.core/` paths. The move is destructive and needs `--allow-destructive` (per D3, O1)"
    - "The slug `deploy_pair_tags_refused` (exit 6) is unchanged. It now fires only for residue: a mode-collection file of a routed unit that this apply cannot delete. `added`, `changed` and `same` units that land in core pass the gate, the engine guard and the plan warning (per D3)"
    - "A Designer edit inside an ign-managed core tag folder shows as `changed` with core `differs`. With `--base` it is `drift: true`, and apply refuses it without `--allow-drift`. A core file the ledger does not list as `.core/` is never deleted: an ambiguous removal stays `shadows_removal` / `deploy_blocked` (per D2)"
    - "Standalone targets, and pair plans without tag units, keep byte-identical plans, `plan_sha256`, delete sets, ledgers, journals, records, work lists and fingerprints. `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE`, `RENDER_GOLDEN` and the fingerprint `GOLDEN` are unchanged (per O5)"
    - "Every apply host (local, `commit.sh`, `commit.ps1`) writes, deletes, snapshots, restores and fingerprints `.core/` keys in `config/resources/core`. No host ever creates that directory or removes `core/ignition/<tag type>`, and every host refuses a `.core/` key outside the two tag types. The `write_host` preflight fails closed, with a message naming the folder, when a core tag folder the run touches is not writable (per D1, O4)"
    - "`ign deploy verify` on a pair judges tag units against core. It adds one `data.warnings` line naming tag files left in the mode collection, with no new check id. The pair `files` check accepts gw-b's re-serialized `unary-resource.json` as parsed-equal JSON. A pulled pair tree is a plan fixpoint (per D1, O2)"
    - "On the Docker 8.3.6 pair rig, an apply of a tag-definition unit plus a tag-type-definition (udts.json) unit ends `applied`: the backup converges with no resync, gw-b logs no config-resource snapshot mismatch, and a following non-tag change converges without a resync. A migration of both unit types pre-seeded in the mode collection converges in one apply (per O3)"
  artifacts:
    - path: "crates/ignition-core/src/deploy/layout.rs"
      provides: "`CORE_KEY_PREFIX`, `KeyCollection`, `split_key`, `core_key`, `route` beside `is_pair_unsynced_tag_path`"
      contains: "CORE_KEY_PREFIX"
    - path: "crates/ignition-core/src/deploy/target.rs"
      provides: "`Redundancy` and `Redundancy::of`, moved here from `apply/gates.rs` (re-exported there)"
      contains: "pub enum Redundancy"
    - path: "crates/ignition-core/src/deploy/plan.rs"
      provides: "`plan_with(.., redundancy)`, routed rows, the per-collection delete set, routed `CoreRelation`, and the residue rule in `pair_tag_keys`"
      contains: "pub fn plan_with"
    - path: "crates/ignition-core/src/deploy/transport/mod.rs"
      provides: "`apply_key_problem` (split the qualifier, two tag types only, then `mirror_path_problem` on the rest) and the core-aware `emptied_dir_floor`"
      contains: "fn apply_key_problem"
    - path: "crates/ignition-core/src/deploy/transport/scripts/commit.sh"
      provides: "the core root (`${CO%/*}/core`, never created), the `.core/` key mapping, the `core/` snapshot and staging slots, the floor-3 `rmdir_up` for core, and the core probe codes"
      contains: ".core/"
    - path: "crates/ignition-core/src/deploy/transport/scripts/commit.ps1"
      provides: "the same core-root handling as commit.sh"
      contains: ".core/"
    - path: "crates/ignition-core/src/deploy/pair.rs"
      provides: "the `unary-resource.json` FILE_NORMALIZATION row"
      contains: "\"unary-resource.json\""
    - path: "crates/ignition-core/tests/deploy_host_conformance.rs"
      provides: "the `core_*` rows run against the local, sh and ps hosts"
      contains: "core_migrate_mode_unit_in_one_commit"
    - path: "crates/ignition-core/tests/deploy_apply.rs"
      provides: "the `pair_core_*` engine tests on the fake pair"
      contains: "pair_core_migration_moves_mode_unit_in_one_commit"
    - path: "crates/ignition-core/tests/live_deploy_pair.rs"
      provides: "the live A2 gates and the core-aware harness sweep"
      contains: "live_pair_core_migration_converges"
    - path: "README.md"
      provides: "the rewritten Tags and redundant pairs section, the `.core/` spelling, the migration, the core grant, and the exit-table row-6 prose"
      contains: ".core/"
  key_links:
    - from: "actions::deploy::deploy_plan (and both verify plan calls)"
      to: "plan::plan_with(.., Redundancy::of(&declared.target))"
      via: "one routing signal (`nodes.backup`) for plan, apply and verify"
      pattern: "plan_with("
    - from: "apply::write_set / new_ledger"
      to: "layout::split_key"
      via: "tree bytes are read by the unqualified path; ops and ledger keep the qualified key"
      pattern: "split_key"
    - from: "apply::host::BatchSpec::check, journal::check_touched, batch.rs work-list parse, the local host"
      to: "transport::apply_key_problem"
      via: "every touched-key boundary validates the same way"
      pattern: "apply_key_problem"
    - from: "apply::gates::pair_tags_refusal, the run_apply guard, the deploy plan warning"
      to: "plan::pair_tag_keys (residue rule)"
      via: "one definition, so the three cannot drift"
      pattern: "pair_tag_keys"
    - from: "apply::converge (absent) and preflight::backup_blocked_units"
      to: "layout::split_key"
      via: "deletes are compared on the effective (mode over core) view by bare path"
      pattern: "split_key"
---

<objective>
**Execution: one executor run per task, in order.** Task 1, then Task 2, then Task 3. Each run starts with a fresh context and ends in its own commit. An executor reads this objective, the shared `<context>` below, and ONLY its own task. Do not start a later task in the same run.

Make tag deploys to redundant pairs work again (option A2 of debug deploy-tag-redundancy-sync). Ignition 8.3.6 counts tag paths in mode collections in its config-resource sync digest but never sends them, and it syncs tag folders only in `core`. Today `deploy_pair_tags_refused` therefore blocks every tag unit on a pair. After this plan:

- **D1 (D-01), routing.** On a target with `nodes.backup`, `tag-definition` and `tag-type-definition` units are written into `core`. Plan, apply and the hosts, the ledger, rollback, pull and verify all use the one routing rule. Standalone targets are unchanged.
- **D2 (D-02), drift.** A Designer edit to an ign-managed core tag folder is drift by the existing mechanism, never a silent overwrite.
- **D3 (D-03), migration.** A pair's tag units already in the mode collection move to core in the same apply. The refusal stays only for residue that the apply cannot delete.

Task 1 is the tracer. It wires a pair tag unit end to end into core through plan, gates, the engine, the ledger, the local host and the fake pair's convergence, then expands that slice to changed units, the migration, removals and auto-rollback on the local host. Task 2 brings the script hosts (`commit.sh`, `commit.ps1`) and the sftp host to parity, using the shared conformance rows. It also adds the `write_host` core probe and the rollback rules. Task 3 makes verify routed and adds its warning, pins the pull fixpoint, rewrites the README, runs the live gates on the Docker 8.3.6 pair rig and the full gate, and writes the SUMMARY.

Purpose: one binary that deploys a pair's tags without wedging its redundancy sync, and recovers a wedged pair by moving the tags.

Scope note (checker W1): the task is large but stays one quick task because routing must land in plan, hosts and verify together to be safe on a pair; each task commits only when its gate is green.

Output: routing code, host and script changes, offline, conformance and live tests, README, SUMMARY. There is no version bump, tag, release, push, PR, worktree or production call.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-CONTEXT.md
@.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-RESEARCH.md
@.planning/debug/resolved/deploy-tag-redundancy-sync.md

RESEARCH.md holds the full file:line integration map (rows 1-25) and the pitfalls (1-12). Line numbers here and there are as of commit 1e9482d on branch `feat/deploy-tags-core`. Task 1 shifts most deploy files, so Tasks 2 and 3 grep the quoted anchor first and read around the hit.

**Orchestrator resolutions (binding; do not revisit):**
- **O1 (Q1).** A migration stays destructive. Mode deletes count in `deleted_here`, so the entry is `destructive` and apply needs `--allow-destructive`, as any entry with delete-set files does today. README says so.
- **O2 (Q2).** `deploy verify` on a pair warns about tag files left in the mode collection: one `data.warnings` line, no new check id.
- **O3 (Q3).** The live pair gate includes a tag-type-definition (`udts.json`) unit as well as a tag-definition unit. It covers a migration case, a unit pre-seeded in the pair's mode collection, converging in one apply.
- **O4 (Q4).** The Docker rig cannot exercise the Windows deploy account's write access on core tag folders, so that is out of scope here. The `write_host` probe checks write access to the core tag type folders on a run that touches them, and fails closed with a clear message. The Windows ACL capture is recorded in SUMMARY as an open item before production use. It is not implemented here.
- **O5.** The `.core/<path>` qualified key from RESEARCH is adopted (P1). Standalone and tag-free plans, ledgers, journals, records, work lists, fingerprints and all goldens (`PLAN_GOLDEN*`, `RENDER_GOLDEN`, fingerprint `GOLDEN`) stay byte-identical.

**Planner decisions (binding for all three tasks; state them in SUMMARY):**
- **P1 (representation).** In the write half, a core-collection path is spelled as the qualified key `.core/<collection path>` (`layout::CORE_KEY_PREFIX = ".core/"`). Mode paths stay bare.
  - Only a path under `ignition/tag-definition` or `ignition/tag-type-definition` (`PAIR_UNSYNCED_TAG_TYPES`) may be qualified.
  - The key carries the collection in: `PlanFile.path`, `DeleteSet.files`, ledger `files`, `WriteOp.path` / `DeleteOp.path`, `BatchSpec::touched_paths`, journal and record `touched`, fingerprint keys, and the work.tsv W/D/P path field.
  - Render and `render_sha256` are unchanged.
  - New pure helpers in `layout.rs`: `CORE_KEY_PREFIX`, `enum KeyCollection { Mode, Core }`, `split_key(&str) -> (KeyCollection, &str)` and `core_key(&str) -> String`.
  - New validator in `transport/mod.rs`: `apply_key_problem(&RelPath) -> Option<String>`. It splits the qualifier, refuses a core key outside the two tag types, then runs the unchanged `mirror_path_problem` on the remainder.
  - The legacy mirror path (`validate_mirror_inputs` / `validate_mirror_paths`, the test-only mirror) stays bare-only and unchanged.
  - An older ign meeting a `.core/` key fails closed, because its validators refuse a dot component (`deploy_ledger_invalid` / `deploy_run_invalid`).
- **P2 (routing rule, D1).** `Redundancy { Standalone, Pair }` and `Redundancy::of(&Target)` move to `deploy/target.rs`. `apply/gates.rs` re-exports them with `pub use`, so `gates::Redundancy` keeps compiling.
  - `layout::route(redundancy, path) -> String` returns `core_key(path)` when `redundancy == Pair && is_pair_unsynced_tag_path(path)`, and `path` otherwise.
  - Plan, apply and verify derive the redundancy with `Redundancy::of(&declared.target)`. That is the same signal option C uses.
- **P3 (plan, `plan_with`).** Add `pub fn plan_with(rendered, observed, ledger, scope, excluded, redundancy) -> Plan`. `plan(..)` becomes `plan_with(.., Redundancy::Standalone)`, so every existing caller and test compiles unchanged. A "routed unit" is a unit of a routed type on a Pair. For a routed unit:
  - **Status** compares the desired files with the **core copy**:
    - no core copy: `added`;
    - equal, and no delete-set file in the unit: `same`;
    - otherwise `changed`;
    - undeclared with delete-set files: `removed`;
    - else `unmanaged`.
  - **`files`** are the `.core/`-qualified rows over desired ∪ core copy (`live_sha256` = core sha), plus one bare row per mode-copy file (`live_sha256` = mode sha, `desired_sha256` null).
  - **`live_in`** is unchanged.
  - **`tag_data_dropped`** keeps judging the runtime view (mode over core), because that is what the gateway runs now (RESEARCH Pitfall 5).
  - **`CoreRelation`** is never `hidden`. A `removed` unit with a core copy is `shadows_removal`, unless every core-copy file is in the delete set; then it gets the computed relation. Everything else is computed as today (`none` / `identical` / `differs`).
  - **Everything else is unchanged.** Non-routed units and every Standalone plan take the existing code path.
- **P4 (delete set, D3).** A ledger entry is processed per collection.
  - **A bare entry** goes through today's rules. "Rendered" means the routed render set, so on a pair a bare tag path whose unit now renders to `.core/` IS deleted when `observed.mode` holds it: that is the migration. The case-fold skip compares only with rendered paths that land in mode.
  - **A `.core/` entry** has its remainder go through the same dot, gateway-owned, exclusion, scope and unreadable skips (unreadable checked against the core key layer). It is kept when the routed render holds the same key, and skipped when `observed.core` lacks the remainder. Its case-fold skip compares only with rendered paths that land in core.
  - **Counting.** Deleted counts go to the unit key from the matching key layer.
  - **Pair turned standalone.** On a Standalone target, a `.core/` ledger entry is deleted from core when the render no longer routes it there, and the unit is then written into mode. ign deletes only what its ledger says it wrote. Without `.core/` ledger entries, standalone output is unchanged.
- **P5 (residue, D3).** `pair_tag_keys(entries, delete)` keeps its signature. It lists a routed-type entry that is not `unmanaged` and has a bare-path (mode) file with a live sha that is not in the delete set. `added`, `changed` and `same` no longer refuse on their own. `gates::pair_tags_refusal`, the `run_apply` guard (`apply/mod.rs` around line 832) and the `deploy plan` warning keep sharing it.
- **P6 (write set).**
  - **Writes.** A `.core/` row with a desired sha becomes a `WriteOp` with `pre` = that row's `live_sha256` (the core sha, or `None` when core lacks it). Bare rows keep today's rule (`pre` only when `live_in` has `mode`). Tree bytes are looked up by the remainder (`split_key`).
  - **Deletes.** A `.core/` delete takes its pre-image from the `.core/` row's live sha. A bare delete takes its pre-image from the bare row as today.
  - **Ledger.** `new_ledger` is unchanged over the qualified strings.
- **P7 (hosts, D1/D3).**
  - **Core root.** The second root is `config/resources/core`. It must already exist as a real directory (not a symlink) and is never created. Directories below it are created as needed.
  - **Slots.** Snapshot and staging slots are `snapshots/<run>/core/<remainder>` and `staging/<run>/core/<remainder>`, beside the existing `files/` slots.
  - **Emptied-dir floor.** `emptied_dir_floor(key)` returns `MIRROR_MIN_COMPONENTS` (3) for a `.core/` key, counted on the remainder, so `core/ignition/<type>` is never removed. A bare key keeps today's rule: 2 for the two tag types, 3 otherwise. Removed-dir reports `[index, levels]` count levels on the remainder.
  - **Live evidence.** The floor-3 choice is RESEARCH assumption A3. The live removal step in Task 3 records the evidence.
- **P8 (files check and convergence).** `pair::FILE_NORMALIZATION` gains the row `("unary-resource.json", &[])`: parsed JSON, every member counts. gw-b pretty-prints the core marker (debug Evidence C1).
  - `udts.json` gets a row only if the live gate shows that gw-b re-serializes it. Record the capture either way.
  - In `converge.rs`, `absent` and in `preflight::backup_blocked_units`, deletes are mapped to their remainder. A migrated mode path stays out of `absent`, because the master's effective view still holds it from core (the existing filter). `present` stays the bare render keys.
- **P9 (rollback).** `pair_tags_restore_refusal` keeps its logic. A `.core/` key is not a mode tag path, so a run that only wrote core can be rolled back. A migration run's mode pre-image restore stays refused. Only the message, hint and doc change.
  - An automatic rollback of a failed migration restores the mode copy: the pre-apply state, which is a wedged pair. README documents it, and a re-apply recovers.
  - Interrupted-run recovery is never refused.
- **P10 (`write_host`, O4).** When `touched` holds a `.core/` key, `probe_paths` also checks the core root: it must exist as a real directory. Problem codes: `core_missing`, `core_symlink`, `core_not_dir`.
  - For each core tag type touched, it checks the deepest existing directory of `core/ignition/<type>`: the type folder, else `core/ignition`, else the core root. That directory must be writable, else `core_tags_unwritable`.
  - Rust maps each code to text naming the folder and pointing at the README grant.
  - Without a `.core/` key, the probe is byte-for-byte today's.
  - Hosts: the local host uses the same access check as its mode check, `commit.sh` uses `-w`, and `commit.ps1` uses `CanAddFile`, as for the mode collection.
- **P11 (verify, O2).** `deploy verify` plans with `plan_with(.., Redundancy::of(target))` in both of its plan calls. On a pair it adds one `data.warnings` line naming every bare-path file of a routed-type entry with a live sha, whatever the entry's status, unmanaged included. Keys and paths only, capped like `pair_tags_warning` with `APPLY_LIST_SHOWN`.
  - `--node both` emits it once.
  - No check id is added.
  - `deploy plan` keeps its refusal-preview warning (P5), with residue wording.
- **P12 (migration, O1).** A migration entry is `destructive`. `destructive_reason` keeps its existing type mapping (`tag_data_loss`), so no new reason word enters the contract. The `hold_budget` preflight counts each moved file twice (write plus delete), which needs no code change.
- **P13 (pull).** `deploy/pull.rs` is unchanged. Core files pull to the base `tags/` and mode files to `modes/<env>/`. A test pins that a pulled pair tree is a `plan_with(Pair)` fixpoint.
- **P14 (drift, D2).** `apply_base` strips `.core/` before the base lookup; the base render is unrouted. `render_diffs` maps `.core/` rows to `observed.core`. The gate stays `--base` plus `--allow-drift` (`gates.rs` around lines 120-183).
- **P15 (texts).** The slug `deploy_pair_tags_refused` is unchanged, so the four-place rule does not apply. Its message and hint (`error.rs` around lines 903 and 1908), the `deploy plan` warning (`actions/deploy.rs` `pair_tags_warning`), the rollback refusal doc and hint, and the README prose (including exit-table row 6) narrow to residue and to mode restores.

**Hard rules (H1):**
- No production calls of any kind: no request, ssh, sftp or `ign` call to any production gateway or host. Live work uses the Docker pair rig only.
- No production host names or IP addresses in any committed file (public repo).
- No worktrees. Run every cargo command sequentially on the main checkout (`.claude/CLAUDE.md`). If phantom E0432 errors or vanished tests appear, rebuild in an isolated `CARGO_TARGET_DIR` before debugging code.
- Stage files by explicit path only. Never stage `.planning/config.json`, `.planning/research/.cache/`, or the untracked `svc-ignition-deploy@*-uat4.ps1` at the repo root.
- No co-author lines. No Cargo.toml or Cargo.lock change. Do not push or open a PR.
- Match the surrounding comment density and idiom: doc comments state the rule and cite the capture (debug Evidence C1/T6, CD5). No narration.
- Each task ends in ONE commit containing its tests and code, so every commit builds and tests green on its own. Tests come first: write them, run them, and record the failing test names (RED) for SUMMARY before implementing.
- Follow superpowers test-driven-development for each task, and superpowers systematic-debugging for any failure. Never weaken an assertion or ignore a test to get green.
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1 (tracer): a pair tag unit lands in core end to end on the local host, then changed units, migration, removal and auto-rollback</name>
  <files>crates/ignition-core/src/deploy/layout.rs, crates/ignition-core/src/deploy/target.rs, crates/ignition-core/src/deploy/plan.rs, crates/ignition-core/src/deploy/ledger.rs, crates/ignition-core/src/deploy/pair.rs, crates/ignition-core/src/deploy/pair/tests.rs, crates/ignition-core/src/deploy/transport/mod.rs, crates/ignition-core/src/deploy/transport/local.rs, crates/ignition-core/src/deploy/transport/local/apply_ops.rs, crates/ignition-core/src/deploy/apply/gates.rs, crates/ignition-core/src/deploy/apply/host.rs, crates/ignition-core/src/deploy/apply/journal.rs, crates/ignition-core/src/deploy/apply/record.rs, crates/ignition-core/src/deploy/apply/mod.rs, crates/ignition-core/src/deploy/apply/converge.rs, crates/ignition-core/src/deploy/apply/preflight.rs, crates/ignition-core/src/actions/deploy.rs, crates/ignition-core/src/error.rs, crates/ignition-core/tests/common/fake_deploy.rs, crates/ignition-core/tests/deploy_plan.rs, crates/ignition-core/tests/deploy_apply.rs</files>
  <precondition>Branch `feat/deploy-tags-core` is checked out at 1e9482d or a descendant. `git status --short` shows only the H1 files and this task directory as non-HEAD changes.</precondition>
  <read_first>
    - `crates/ignition-core/src/deploy/layout.rs` 1-130 (`RelPath`, `PAIR_UNSYNCED_TAG_TYPES`, `is_pair_unsynced_tag_path`).
    - `crates/ignition-core/src/deploy/apply/gates.rs` 30-80 (`Redundancy`, `pair_tags_refusal`) and 110-190 (`evaluate_gates` order, drift). Then grep `fn pair_tags_` in its tests module.
    - `crates/ignition-core/src/deploy/plan.rs`:
      - 100-137 (canonical text grammar) and 196-274 (`CoreRelation`, `PlanFile`, `PlanEntry`);
      - 465-513 (`is_tag_folder`, `pair_tag_keys`) and 536-678 (`tag_data_dropped`, `in_scope`, `KeyLayers`, `delete_set`);
      - 737-900 (`plan`), 1250-1310 (`apply_base`, `render_diffs`) and 1471-1515 (`canonical_text`).
    - `crates/ignition-core/src/deploy/ledger.rs`: grep `fn check_files` and read it.
    - `crates/ignition-core/src/deploy/transport/mod.rs` 155-166 (`DeleteSet`) and 585-700 (`mirror_mode_problem`, `mirror_path_problem`, `MIRROR_MIN_COMPONENTS`, `emptied_dir_floor`).
    - `crates/ignition-core/src/deploy/apply/host.rs` 55-240 (`WriteOp`, `DeleteOp`, `BatchSpec`, `touched_paths`, `check`).
    - `crates/ignition-core/src/deploy/apply/journal.rs` 355-370 (`check_touched`). Grep `record.rs` for any touched-path validation.
    - `crates/ignition-core/src/deploy/apply/mod.rs` 264-420 (`write_set`, `new_ledger`, `batch_spec`), around 832 (the pair guard) and 1680-1700 (`snapshot_warnings`).
    - `crates/ignition-core/src/deploy/apply/converge.rs` 300-360 and `apply/preflight.rs` 912-945 (`backup_blocked_units`).
    - `crates/ignition-core/src/deploy/pair.rs` 690-760 (`effective_files`, `FILE_NORMALIZATION`, `compare_files`) and 850-875 (`same_document`).
    - `crates/ignition-core/src/deploy/transport/local/apply_ops.rs` 60-260 (`Layout`, `collection`, `live_file`), 716-1110 (`commit_moves`, `remove_emptied`, `restore_one`, `copy_back`) and 1330-1400 (`copy_snapshot_dir`). Grep `local.rs` for `fn prepare_blocking`, `fn apply_commit_blocking`, `fn fingerprint` and `fn fetch_snapshot`.
    - `crates/ignition-core/src/actions/deploy.rs` 338-480 (`deploy_plan`, `pair_tags_warning`). Grep `error.rs` for `DeployPairTagsRefused` (the variant, its text and its hint near 1908, and the `deploy_pair_tags_refused_four_places` test).
    - `crates/ignition-core/tests/common/fake_deploy.rs` 1071-1110 and 1290-1430 (`PairLink`, `sync_now`, `copy_collection`).
    - `crates/ignition-core/tests/deploy_apply.rs` 5558-5600 (`with_pair`) and 7291-7600 (the five `pair_*` tests and their helpers).
    - `crates/ignition-core/tests/deploy_plan.rs` around 560-600 (the `PLAN_GOLDEN` test and its tree helpers), plus one ledger-based test near 1084.
  </read_first>
  <behavior>
    - core_key_route_qualifies_only_pair_tag_types: `route(Pair, "ignition/tag-definition/default/Deploy/tags.json")` is `.core/...`; `route(Pair, "ignition/tag-group/default/G/config.json")` is bare; `route(Standalone, <tag path>)` is bare. `split_key` round-trips both spellings.
    - core_key_apply_key_problem: refuses `.core/ignition/database-translator/X/config.json`, `.core/ignition/tag-provider/p/config.json`, `.core/` alone, `.core/.core/ignition/tag-definition/a/b` and `.core/ignition/tag-definition/a/.hidden`. Accepts `.core/ignition/tag-type-definition/default/T/udts.json`. A bare `.core/...` still fails `mirror_path_problem`, which is the downgrade guarantee.
    - core_key_ledger_*: `Ledger::parse` accepts a `.core/ignition/tag-definition/...` file and refuses a `.core/` file of any other type. The case-variant check runs per collection, so `.core/x/A` beside the bare `x/a` is fine, and `.core/x/A` beside `.core/x/a` is refused.
    - core_key_batch_spec_*: `BatchSpec::check` accepts a write to `.core/X` together with a delete of the bare `X`, and refuses a `.core/` key outside the two types. `journal::check_touched` accepts `.core/` tag keys and refuses the rest.
    - core_key_files_check_unary_parsed: a compact versus a pretty-printed `unary-resource.json` passes `compare_files`; a changed member fails (in `pair/tests.rs`).
    - core_route_added_unit_rows: a pair plan with a declared Deploy tag unit and no live copy has status `added`, core `none`, and `.core/` F rows. Its `plan_sha256` differs from the Standalone plan of the same tree, and `pair_tag_keys` is empty.
    - core_route_same_against_core: the core copy is byte-equal to the render, there is no mode copy, and the status is `same`.
    - core_route_changed_never_hidden: a core copy holds an extra member file. The status is `changed`, core is `differs` (not `hidden`), and the write pre-images are the core shas.
    - core_route_migration_rows_and_deletes: the ledger lists the bare paths, the mode copy equals the render and there is no core copy. The status is `added`, `destructive: true`, with `.core/` desired rows plus bare mode rows. The delete set is the bare files, and `pair_tag_keys` is empty.
    - core_route_residue_refused: a mode copy NOT in the ledger, with the core copy equal. The status is `same`, and `pair_tag_keys` names the unit.
    - core_route_removed_core_unit: the ledger lists `.core/` paths, the unit is undeclared and core holds it. The delete set has the `.core/` files, the status is `removed`, core is `differs` (not `shadows_removal`) and the unit is not refused.
    - core_route_ambiguous_removal_shadows: the ledger lists bare paths, the mode copy is live, a Designer core copy is not ledgered and the unit is undeclared. The result is `shadows_removal`, and `evaluate_gates` refuses `deploy_blocked`.
    - core_route_case_fold_per_collection: the rendered `Deploy` is routed to core and the ledger has the bare `deploy` in mode, so the bare copy is deleted with no case-fold warning. The ledger `.core/.../deploy/...` with the rendered core `Deploy` gives the case-fold skip and its warning.
    - core_route_drift_on_core_copy (D2): `--base` equals the render and the core copy was edited. Then `drift: true`, and `evaluate_gates` refuses without `--allow-drift` and passes with it.
    - core_route_tag_free_pair_equals_standalone and core_route_standalone_equals_plan: a pair plan of a tag-free tree, and `plan_with(Standalone)` of any tree, serialize and hash byte-identical to `plan(..)`. `PLAN_GOLDEN*` stay unchanged.
    - core_route_standalone_ledger_core_entry: a Standalone target whose ledger lists `.core/` paths deletes them from core and writes the unit bare into mode (P4).
    - The pair gate tests in `gates.rs` are rewritten. On a pair, `added`, `changed` and `same` tag units are not refused; residue is refused; a Standalone target is never refused.
    - pair_core_added_unit_lands_in_core (the tracer): on the fake pair with the local host, apply ends `Applied` and the backup converges. gw-a `config/resources/core/ignition/tag-definition/default/Deploy/{tags.json,unary-resource.json}` holds the repo bytes. The mode collection has no `ignition/tag-definition`. The ledger and the record's touched list hold the `.core/` keys. The backup's core copy, with a pretty-printed marker, passes the `files` check.
    - pair_core_changed_unit_replaces_core_copy: core holds an older copy, the apply replaces it, and the snapshot slot `core/<path>` holds the old bytes.
    - pair_core_migration_moves_mode_unit_in_one_commit: a ledgered mode copy, applied with `allow_destructive`, ends `Applied` with core written. The mode files and `ignition/tag-definition` are gone, the ledger has only the `.core/` keys for the unit, there is one commit and one scan, and the backup converges.
    - pair_core_migration_needs_allow_destructive: the same plan without the flag is `destructive_change_refused`, and nothing is written.
    - pair_core_removed_unit_deletes_core_copy: the ledger lists `.core/` keys and the unit is undeclared. The core files and the emptied provider dir are removed, and `core/ignition/tag-definition` stays.
    - pair_core_auto_rollback_restores_both_collections: a migration whose verify turns red (the `scenario_verify_red` hook pattern) ends `rolled_back`. The mode copy is back byte-equal, the created core files are removed and the core type folder stays.
    - pair_core_missing_core_root_writes_nothing: with no `config/resources/core` on gw-a, the apply fails before any move. No core directory is created, and the mode collection and ledger are unchanged.
    - The five existing `pair_*` tests in `deploy_apply.rs` are retargeted to residue where they relied on `added`/`kept` refusals: the refusal, the kept-unit refusal and the engine guard now seed an unledgered mode file. The removed-with-mode-file and emptied-type-folder tests stay as they are.
  </behavior>
  <action>
    This task implements D1, D2 and D3 for the pure layers, the engine and the local host (P1-P8, P12, P14, P15). The work goes in three phases: RED, the tracer slice, then the expansion.

    1. **RED.**
       - Write every test above.
       - Add the new public items with real signatures so the tests compile: `CORE_KEY_PREFIX`, `KeyCollection`, `split_key`, `core_key`, `route`, `Redundancy` in `target.rs` (re-exported from `gates.rs`), `plan_with`, and `apply_key_problem`. While RED, `plan_with` ignores the redundancy.
       - Fake pair: extend `PairLink` in `tests/common/fake_deploy.rs` so each sync also copies the master's `config/resources/core/ignition/tag-definition` and `.../tag-type-definition` trees to the backup's core (modelling tag-config-resource, debug Evidence C1). Re-serialize each copied `unary-resource.json` with `serde_json::to_vec_pretty`, as gw-b does. Leave the existing mode-collection copy as it is.
       - The fixtures for the `pair_core_*` tests create `config/resources/core` on gw-a, as a real gateway has it, except the missing-root test.
       - Run the new tests and record the failing names for SUMMARY.
    2. **Tracer slice: one `added` pair tag unit, end to end.** Implement in this order, re-running `pair_core_added_unit_lands_in_core` until it is green:
       - `layout.rs` helpers and `route` (P1, P2).
       - `plan_with` routed rows and status against the core copy (P3), plus the residue rule in `pair_tag_keys` (P5).
       - `deploy_plan` in `actions/deploy.rs` calls `plan_with(.., Redundancy::of(&declared.target))`. Leave the verify calls for Task 3.
       - `apply_key_problem` used by `BatchSpec::check`, `journal::check_touched` and any record touched-path validation, and `Ledger::check_files` accepting `.core/` tag keys (P1).
       - `write_set` and `new_ledger` (P6).
       - The local host: a core root opened read-write only when it exists as a real directory, never created, with the symlink and real-dir checks of the mode collection. It applies the `core/` snapshot and staging slots in prepare, commit, fingerprint, finalize, discard and `fetch_snapshot`. A missing core root refuses before the first move and creates nothing (P7).
       - `snapshot_warnings` maps `.core/` keys to the `core/` slot.
       - `converge.rs` `absent` and `preflight::backup_blocked_units` map deletes to their remainder (P8).
       - The `("unary-resource.json", &[])` row in `FILE_NORMALIZATION` (P8).
    3. **Expansion, within this task.**
       - The per-collection delete set (P4) and routed `CoreRelation` (P3).
       - The `.core/` pre-images for replaced and deleted core files (P6).
       - The local host's core deletes with `emptied_dir_floor(key)` returning 3 for `.core/` keys on the remainder (P7).
       - Restore of `.core/` touched entries: put back from the `core/` slot, or remove a created file and its emptied dirs down to floor 3.
       - `apply_base` and `render_diffs` strip the qualifier (P14).
    4. **Texts (P15).**
       - The `DeployPairTagsRefused` message and hint name the residue: mode-collection files of tag units the apply cannot delete. The hint points at removing them on the master (runbook T6e) or listing them in the ledger by a deploy that declares them.
       - `pair_tags_warning` uses the same residue wording.
       - Doc comments on `pair_tag_keys`, `emptied_dir_floor`, `PlanFile.path` (a `.core/` prefix names the core collection) and `Redundancy` state the A2 rule.
       - Keep `deploy_pair_tags_refused_four_places` green, adjusting only text assertions.
    5. **Script hosts.** Do not touch `commit.sh`, `commit.ps1`, `batch.rs` or `sftp/apply_ops.rs` here; Task 2 adds the core root to them. Their existing dot-component validation refuses a `.core/` key, so they fail closed in between.
    6. **Commit.**
       - Run the verify command. Fix any regression at its cause.
       - Confirm the four plan/render goldens and the fingerprint `GOLDEN` are untouched in the diff.
       - Commit `feat(261102-cef): route pair tag units to the core collection (engine, plan, local host)`, staging every file of this task by explicit path (H1).
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --lib core_key 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=8)}' && cargo test -p ignition-core --test deploy_plan core_route 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=12)}' && cargo test -p ignition-core --test deploy_apply pair_core_ 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=7)}' && cargo test -p ignition-core --lib 2>&1 | grep -E '^test result' && cargo test -p ignition-core --test deploy_plan --test deploy_apply --test deploy_pair --test deploy_verify --test deploy_pull --test deploy_host_conformance --test deploy_transport 2>&1 | grep -E '^test result' && cargo test -p ignition-cli --test contract_deploy --test deploy_family_coverage 2>&1 | grep -E '^test result' && cargo clippy --workspace --all-targets -- -D warnings && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && grep -q 'pub fn plan_with' crates/ignition-core/src/deploy/plan.rs && grep -q 'plan_with(' crates/ignition-core/src/actions/deploy.rs && grep -rqE 'fn batch_core_key_round_trip\(' crates/ && grep -rqE 'fn core_key_apply_key_problem\(' crates/ && grep -rqE 'fn core_key_files_check_unary_parsed\(' crates/ && grep -rqE 'fn core_key_outside_tag_types_refused\(' crates/ && grep -rqE 'fn core_key_route_qualifies_only_pair_tag_types\(' crates/ && grep -rqE 'fn core_route_added_unit_rows\(' crates/ && grep -rqE 'fn core_route_ambiguous_removal_shadows\(' crates/ && grep -rqE 'fn core_route_case_fold_per_collection\(' crates/ && grep -rqE 'fn core_route_changed_never_hidden\(' crates/ && grep -rqE 'fn core_route_drift_on_core_copy\(' crates/ && grep -rqE 'fn core_route_migration_rows_and_deletes\(' crates/ && grep -rqE 'fn core_route_removed_core_unit\(' crates/ && grep -rqE 'fn core_route_residue_refused\(' crates/ && grep -rqE 'fn core_route_same_against_core\(' crates/ && grep -rqE 'fn core_route_standalone_equals_plan\(' crates/ && grep -rqE 'fn core_route_standalone_ledger_core_entry\(' crates/ && grep -rqE 'fn core_route_tag_free_pair_equals_standalone\(' crates/ && grep -rqE 'fn pair_core_added_unit_lands_in_core\(' crates/ && grep -rqE 'fn pair_core_auto_rollback_restores_both_collections\(' crates/ && grep -rqE 'fn pair_core_changed_unit_replaces_core_copy\(' crates/ && grep -rqE 'fn pair_core_migration_moves_mode_unit_in_one_commit\(' crates/ && grep -rqE 'fn pair_core_migration_needs_allow_destructive\(' crates/ && grep -rqE 'fn pair_core_missing_core_root_writes_nothing\(' crates/ && grep -rqE 'fn pair_core_removed_unit_deletes_core_copy\(' crates/ && echo T1-OK</automated>
  </verify>
  <done>`T1-OK` is printed.
- **Tests.** At least 8 `core_key*` lib tests, 12 `core_route*` plan tests and 7 `pair_core_*` engine tests pass. The rewritten pair gate tests pass, and every other lib, deploy and CLI contract test passes. The goldens and the fingerprint `GOLDEN` are byte-identical.
- **Behaviour on the fake pair.** A tag unit lands in core, a ledgered mode copy migrates in one commit, and a removal deletes the core copy and keeps `core/ignition/<type>`. Auto-rollback restores both collections. A missing core root writes nothing.
- **Commit.** One commit on `feat/deploy-tags-core`, staged by explicit path, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: the core root on the script hosts and the sftp host, conformance rows for all three hosts, the write_host core probe, and the rollback rule</name>
  <files>crates/ignition-core/src/deploy/transport/batch.rs, crates/ignition-core/src/deploy/transport/scripts/commit.sh, crates/ignition-core/src/deploy/transport/scripts/commit.ps1, crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs, crates/ignition-core/src/deploy/transport/sftp.rs, crates/ignition-core/src/deploy/transport/winps.rs, crates/ignition-core/src/deploy/transport/local.rs, crates/ignition-core/src/deploy/transport/local/apply_ops.rs, crates/ignition-core/src/deploy/apply/preflight.rs, crates/ignition-core/src/deploy/apply/rollback.rs, crates/ignition-core/tests/deploy_host_conformance.rs</files>
  <precondition>Task 1's commit is HEAD, and `cargo test -p ignition-core --test deploy_apply pair_core_` passes.</precondition>
  <read_first>
    Grep each anchor first (Task 1 shifted lines), then read around it.
    - `transport/batch.rs`: grep `fn to_lines`, `fn parse_lines`, `mirror_path_problem` (around 481-510) and `POSIX_STUB`.
    - `scripts/commit.sh`, the functions:
      - `check_work`, `check_roots`, `collection_path`, `depth_of`;
      - `snapshot_one` / `snapshot_all`, `make_parents`, `ensure_collection`, `move_one`, `moves`, `rmdir_up`, `deletes`;
      - `restore_one`, `op_restore`, `op_fingerprint`, `op_probe`.
      In each, note how `$CO`, `$MD` and the `files/` slot are used.
    - `scripts/commit.ps1`, the functions: `CheckWork`, `CheckRoots`, `CollectionPath`, `DepthOf`, `SnapshotOne` / `SnapshotAll`, `MakeParents`, `EnsureCollection`, `MoveOne`, `RmdirUp`, `Deletes`, `RestoreOne`, `OpFingerprint`, `OpProbe` (grep `CanAddFile`).
    - `transport/sftp/apply_ops.rs`:
      - 95-145 (`windows_long_path_problems`, `first_components`);
      - the staging upload in prepare (grep `staging`);
      - the removed-dir report parsers (around 681-687 and 959);
      - the probe code texts (around 1005-1020) and `probe_paths` (around 1231).
    - `transport/sftp.rs`: grep `mirror_report_names_the_type_folder_only_for_the_pair_tag_types` and the probe-code test around 7522.
    - `transport/winps.rs`: grep `the_shipped_script_runs_every_operation_commit_sh_has_and_keeps_the_hardening`.
    - `transport/local/apply_ops.rs`: grep `fn probe_blocking`. `local.rs`: grep `async fn probe` and `probe_paths`.
    - `apply/preflight.rs`: grep `async fn write_host_finding`.
    - `apply/rollback.rs`: grep `fn pair_tags_restore_refusal` and its tests (grep `pair_rollback_refuses`).
    - `tests/deploy_host_conformance.rs`:
      - 40-200 (`scenarios`, `Ctx`, `seed`, `listing`) and `fn batch`;
      - the rows `delete_emptying_a_pair_tag_type_removes_its_type_folder` (around 797), `restore_removes_a_created_pair_tag_type_folder`, `gateway_owned_file_refused` (around 1022) and `probe_reports_writable_without_touching_the_collection`;
      - the `conformance_local_*` / `run_sh` / `run_ps` wiring.
  </read_first>
  <behavior>
    These rows go in `scenarios()`, each wired as `conformance_local_core_*`, `conformance_sh_core_*` (unix) and `conformance_ps_core_*` (windows).
    - core_migrate_mode_unit_in_one_commit: setup:
      - the mode collection holds `ignition/tag-definition/default/Deploy/{tags.json,unary-resource.json}`;
      - the core root exists, with a Designer sibling `ignition/tag-definition/default/Other/tags.json`.
      One batch writes `.core/.../Deploy/*` and deletes the bare `.../Deploy/*` with their pre-images. After commit:
      - core holds the bytes;
      - the mode collection has no `ignition/tag-definition` (floor 2);
      - the sibling is untouched;
      - the snapshot `files/` slot holds the mode pre-images;
      - `fingerprint` over the touched keys equals the post hashes;
      - the record's touched list holds the `.core/` keys.
    - core_restore_migration_puts_mode_back: after the migration, `restore` puts the mode copy back byte-equal. It removes the created core files and their emptied dirs down to floor 3, keeps `core/ignition/tag-definition` and leaves the sibling alone. A second restore changes nothing.
    - core_replace_checks_pre_image: core holds Deploy. A write with the right core pre-image replaces it, and the `core/` snapshot slot holds the old bytes. A wrong pre-image is stale before any move, and neither root changes.
    - core_delete_keeps_core_type_folder: deleting `.core/.../Deploy/*` removes Deploy and its emptied provider dir, and keeps `core/ignition/tag-definition`.
    - core_root_absent_never_created: with no `config/resources/core`, a batch with a `.core/` key is refused. No core directory exists afterwards and the mode collection is unchanged. On unix, a symlinked core root is refused the same way.
    - core_key_outside_tag_types_refused: `prepare` refuses `.core/ignition/tag-provider/p/config.json` and `.core/ignition/database-translator/X/config.json` with `transport_failed`, and nothing is written. `restore` refuses a record whose touched path was rewritten to such a key; that is the script-side check on the sh and ps hosts.
    - core_probe_checks_core_root:
      - `probe_paths` with a `.core/` key and no core root reports not writable, with a problem naming the core collection;
      - with the core root present, it reports writable;
      - without a `.core/` key it ignores a missing core root, as standalone does today;
      - on unix, a `0555` `core/ignition/tag-definition` gives a problem naming that folder.
    - Unit tests (`batch.rs`, `sftp.rs`, `rollback.rs`):
      - batch_core_key_round_trip: W/D/P lines carry `.core/` keys and round-trip, and a bare-only work list renders byte-identical to today (the existing format tests pass unchanged). A `.core/` key outside the two types fails `parse_lines`.
      - the sftp report parser accepts a core removed-dir report at floor 3 on the remainder, and refuses floor 2 for a core key.
      - `windows_long_path_problems` checks a `.core/` key against its real `core\\` slot and its `config\\resources\\core\\` live path.
      - the probe codes `core_missing` / `core_symlink` / `core_not_dir` / `core_tags_unwritable` map to their texts.
      - rollback_core_only_run_allowed_on_pair: a run that only touched `.core/` keys can be rolled back on a pair. rollback_migration_run_refused_on_pair: a run with a bare tag key carrying a pre-image is refused with `deploy_pair_tags_refused`, naming the folder.
  </behavior>
  <action>
    This task brings every host to the P7 rules, implements P10 (O4) and P9, and keeps the work list format (P1).

    1. **RED.**
       - Add the conformance rows above to `scenarios()` and wire all three host variants. ps rows are `#[cfg(windows)]`, like their neighbours.
       - Give `Ctx` `core_root()`, `seed_core(rel, bytes)` and `core_listing()`. Let `Ctx::batch` accept `.core/` keys, putting the remainder into the `RenderedTree`.
       - Add the unit tests.
       - Run the local and sh rows plus the unit tests, and record the failures.
    2. **`batch.rs`.** Validate W/D/P paths and touched paths with `apply_key_problem` instead of `mirror_path_problem`. The format is unchanged; the path field carries the key.
    3. **`commit.sh`.**
       - Derive the core root `CR="${CO%/*}/core"`. `check_roots` already pins `$CO` to `*/config/resources/$MD`.
       - Verify `CR` as a real directory, never a symlink, only when the work list holds a `.core/` key, and never create it.
       - A `.core/` path is accepted only as `.core/ignition/tag-definition/*` or `.core/ignition/tag-type-definition/*`. Its remainder must pass the same component checks as a bare path. It maps to `$CR/<remainder>`, with snapshot and staging slots `core/<remainder>`.
       - `rmdir_up` uses floor 3 on the remainder for core keys. Bare keys keep today's case: floor 2 for the two tag types.
       - `restore_one`, `op_fingerprint` and `op_probe` handle core keys. The probe adds the P10 codes.
       - Removed-dir reports keep `[index, levels]`, with levels counted on the remainder.
    4. **`commit.ps1`.** The same rules in its own idiom (`Sub`, `CompOk`, `CanAddFile`, `ResetAcl` on moved files as for mode). Ordinal string tests, as the existing floor check uses.
       - Parse-check the script on this Mac with pwsh, by creating a `[ScriptBlock]` from its text.
       - Extend the structural test in `winps.rs`, or add a sibling test, to pin that the script names the core root and the floor-3 core branch.
       - The `conformance_ps_core_*` rows execute on the Windows CI leg. SUMMARY says so.
    5. **`sftp/apply_ops.rs`.**
       - Upload `.core/` writes to the staging `core/` slot.
       - `windows_long_path_problems` checks core keys against their real slot and live path.
       - The removed-dir report parsers re-derive names with `emptied_dir_floor(key)` on the remainder.
       - Map the P10 probe codes to text naming the folder and "see README Tags and redundant pairs". `probe_paths` passes what the script needs to probe the core folders the touched keys fall under: follow how the work list already reaches `op_probe`.
    6. **Local host probe (P10).** Override `probe_paths` in the local host. It runs the core root and core tag folder checks when `touched` holds a `.core/` key, using the same writability check the mode collection uses. Otherwise it answers `probe()` as today.
    7. **`preflight.rs` `write_host_finding`.**
       - Pass detail when `.core/` keys are touched: "the mode collection, the core tag folders and the ign-deploy root are writable".
       - Unchanged otherwise.
       - Fail detail: the problems, as today.
    8. **`rollback.rs` (P9).**
       - Keep `pair_tags_restore_refusal`'s logic.
       - Update its doc: a core-only run is rollbackable; a migration run's mode restore is refused, because it would wedge the pair, and re-applying is the recovery.
       - Update the error hint text where the rollback refusal reaches the user.
    9. **Commit.**
       - Run the verify command.
       - Commit `feat(261102-cef): core collection writes on every apply host, write_host core probe, pair rollback rule`, staging this task's files by explicit path (H1).
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --test deploy_host_conformance _core_ 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=14)}' && cargo test -p ignition-core --test deploy_host_conformance 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib transport:: 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib rollback:: 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib core_key 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --test deploy_apply --test deploy_transport --test deploy_plan 2>&1 | grep -E '^test result' && sh -n crates/ignition-core/src/deploy/transport/scripts/commit.sh && pwsh -NoProfile -NonInteractive -Command '$null = [ScriptBlock]::Create([IO.File]::ReadAllText("crates/ignition-core/src/deploy/transport/scripts/commit.ps1")); "PS1-PARSE-OK"' | grep -q PS1-PARSE-OK && grep -q '\.core/' crates/ignition-core/src/deploy/transport/scripts/commit.sh && grep -q '\.core/' crates/ignition-core/src/deploy/transport/scripts/commit.ps1 && cargo clippy --workspace --all-targets -- -D warnings && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && grep -rqE 'fn core_delete_keeps_core_type_folder\(' crates/ && grep -rqE 'fn core_probe_checks_core_root\(' crates/ && grep -rqE 'fn rollback_core_only_run_allowed_on_pair\(' crates/ && grep -rqE 'fn rollback_migration_run_refused_on_pair\(' crates/ && echo T2-OK</automated>
  </verify>
  <done>`T2-OK` is printed.
- **Rows.** At least 14 local and sh `_core_` conformance rows pass. The ps rows compile under the windows-msvc `cargo xwin clippy`, `commit.ps1` parses under pwsh, and the existing conformance rows pass unchanged.
- **Host rules.** No host creates `config/resources/core` or removes `core/ignition/<type>`, and every host refuses a `.core/` key outside the two tag types.
- **Probe and rollback.** `write_host` fails closed, naming the core folder. A core-only run is rollbackable on a pair, and a migration run's manual rollback is refused.
- **Commit.** One commit, staged by explicit path, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: routed verify with the mode-residue warning, the pull fixpoint, README, the live pair gates on the 8.3.6 rig, and the full gate</name>
  <files>crates/ignition-core/src/actions/deploy.rs, crates/ignition-core/tests/deploy_pair.rs, crates/ignition-core/tests/deploy_pull.rs, crates/ignition-core/tests/live_deploy_pair.rs, README.md, .planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md</files>
  <precondition>Task 2's commit is HEAD. Docker can run: `docker info` succeeds, or `open -a Docker` brings it up within 180 s.</precondition>
  <read_first>
    Grep each anchor first, then read around it.
    - `actions/deploy.rs`: grep `async fn deploy_verify_planned`, the second `plan(` call inside the `--node backup|both` path (around 1568), `fn pair_tags_warning` and how `warnings` reach `DeployVerifyResult`.
    - `tests/deploy_pair.rs` (or wherever `VerifyNode::Both` is tested; grep it): the harness for a pair verify with files.
    - `tests/deploy_pull.rs`: one pull test that builds a live tree and pulls it into a temp repo.
    - `tests/live_deploy_pair.rs`:
      - 1-115 (env table, constants, `TAG_TYPE_DIRS`) and 440-540 (`units_on_b`, `sweep_a`, `pair()`);
      - 559-660 (`cleanup`, `guarded`) and 852-935 (`run_options`, `apply_over_pair`, `verify_over_pair`);
      - 1487-1550 (canary constants, `repo_with_tags`, `files_below`, `project_states`);
      - all of `live_pair_refuses_a_tag_unit` and `live_pair_tag_cleanup_converges`, which hold the model for the wedge wait, the settle and no-resync assertions, and the non-tag follow-up change;
      - `crates/ignition-core/src/deploy/pair.rs` 150-200 (`sync_exhausted`, `wall_clock_ms`).
    - README: grep `#### Tags and redundant pairs` (around 2626) and read to the end of runbook T6e, the exit table row `| 6 ` (around 271), the `ign deploy apply` row (around 585), and `deploy_pair_tags_refused` at around 3014, 3624, 4002 and 4038. Also read the host permission notes (grep `**Windows hosts.**` around 3893, and the Linux note after it) and the plan output contract (grep `"delete_set"` or `files\[\]`).
  </read_first>
  <behavior>
    - pair_verify_core_units_green (offline): on a pair whose tag unit lives in core, byte-equal to the repo, `deploy_verify` and `--node both` are green. The `files` check passes with the backup's pretty marker, and there is no pair warning.
    - pair_verify_warns_on_mode_tag_files (O2): the same pair plus a mode copy of a tag unit. Verify's checks and exit are the same as without the residue. `data.warnings` holds exactly one line naming the mode file paths. The set of check ids is unchanged, and `--node both` carries the line once.
    - standalone_verify_no_pair_warning: a standalone target with a mode-collection tag unit verifies as before, with no pair warning.
    - pull_pair_core_tags_fixpoint (P13): a pair live tree with a tag-definition and a tag-type-definition unit in core pulls to the base `tags/`. `plan_with(Pair)` of the pulled repo against the same live tree has only `same` entries, an empty delete set and no write.
    - The live gates (`#[ignore]`, quiet green skip without `IGNITION_LIVE_PAIR=1`), all under `PAIR_LOCK` and `guarded`:
      - (a) live_pair_core_tag_units_converge:
        - The repo declares the canary tag-definition unit `default/Deploy` and a tag-type-definition unit `default/DeployTypes` (one `udts.json` with one UdtType and its `unary-resource.json` listing `udts.json`).
        - The apply ends `Applied`, with `backup.converged`, no resync, and `files` pass with at least 4 checked.
        - gw-a's mode collection has neither type folder, and both nodes' core hold both units.
        - Since the gate's start, gw-b's logs carry no `did not match remote system snapshot` message and `sync_exhausted` is false.
        - Both nodes settle on projectState Good.
        - A following non-tag change (the alarm-settings model from `live_pair_tag_cleanup_converges`) converges with waited 0 s and no resync.
        - A removal apply (both units undeclared, `allow_destructive`) deletes the core copies on gw-a. gw-b's core copies go, it converges without a resync and both nodes are Good.
        - Record whether gw-b re-serialized `udts.json` (P8).
      - (b) live_pair_core_migration_converges:
        - Pre-seed both units and a bare-path ledger in gw-a's mode collection, then scan.
        - Wait until gw-b's logs show the snapshot-mismatch message or TEC-5105. This proves the log check of gate (a) can see the line.
        - One apply of the declaring repo with `allow_destructive` ends `Applied`. Both mode type folders are gone on gw-a, core holds both units on both nodes, and the on-box ledger lists the `.core/` keys of both units. The backup converges; the one apply resync is allowed, because the pre-state was wedged.
        - A following non-tag change converges with waited 0 s and no resync, and both nodes are Good.
      - (c) live_pair_refuses_a_tag_unit, retargeted to residue: the canary is declared, and gw-a's mode collection holds an unledgered canary file, written without a scan. `prepare` refuses `deploy_pair_tags_refused` (exit 6). Nothing ign writes appears: gw-a's core and ledger are unchanged, and gw-b is not contacted for writes. The cleanup removes the file and recovers as `sweep_a` does.
  </behavior>
  <action>
    This task implements P11 (O2), P13 and the README for D1-D3 and O1-O4, runs the live gate (O3), and closes out.

    1. **RED.** Write the offline tests above, plus the live gates and harness changes (the live gates are `#[ignore]`). Run the offline ones and record the failures.
    2. **`actions/deploy.rs` (P11).**
       - Both verify plan calls use `plan_with(.., Redundancy::of(&declared.target))`.
       - On a pair, add the one mode-residue warning (keys and paths only, capped with `APPLY_LIST_SHOWN`), emitted once per verify result.
       - Doc-comment the rule on `deploy_verify` and `deploy_verify_node`.
    3. **README (D1-D3, O1-O4, P1, P4, P9, P10, P12, P15).**
       - **Rewrite `#### Tags and redundant pairs`.**
         - Routing on a pair.
         - The `.core/` spelling in plan JSON (`entries[].files[].path`, `delete_set.files`), the apply `written` / `deleted` lists and the on-box ledger. Older ign refuses such a ledger (`deploy_ledger_invalid`).
         - The migration: one apply; destructive, so `--allow-destructive`.
         - Residue refusal and its cases.
         - D2 drift on core folders through `--base` / `--allow-drift`, and `shadows_removal` for an ambiguous removal.
         - Rollback: a core-only run is rollbackable; a migration run is refused; an automatic rollback of a failed migration restores the wedged pre-state, and re-apply recovers it.
         - The `files` check's `unary-resource.json` parsed comparison.
         - The verify warning.
         - The pair-to-standalone case (P4).
         - Keep runbook T6e for a pair wedged by an older ign or by hand.
       - **Host permissions.** The deploy account needs write access on `config/resources/core/ignition/tag-definition` and `.../tag-type-definition` on both nodes (failover symmetry): `Modify` on Windows, rwx on Linux. `write_host` refuses before any write otherwise. The Windows grant is not yet captured on a real host.
       - **The other places.** Update the apply command row, the plan paragraph, the apply-order step, the rollback paragraph and exit-table row 6. Row 6 prose: "a tag unit a deploy would leave or restore in a redundant pair's mode collection". The slug list is unchanged.
    4. **Live (O3, Docker pair rig only).**
       - **Bring the rig up.** `cd crates/ignition-core/tests/fixtures/live-rig-pair && ./setup-pair.sh up && ./setup-pair.sh mint && ./setup-pair.sh pair && ./setup-pair.sh mode`, then source `~/.cache/ign-pair-rig/tokens.env`.
       - **Harness.** `sweep_a` also removes exactly the gates' unit directories from gw-a's core, never a whole core type folder, because the rig holds System provider folders there. It then scans and waits for gw-b's core copies to go. Add a core-aware variant of `units_on_b`.
       - **Run.** With the env table of the file header (`IGNITION_LIVE_PAIR=1`, the A/B URLs, tokens and data dirs; values named, never printed), run `cargo test -p ignition-core --test live_deploy_pair -- --ignored --nocapture --test-threads=1`. This runs the three gates above and the existing gates (`live_pair_converges`, `_with_a_stale_signature`, `_secret_names`, `_unsynced_then_reconverge`, `_tag_cleanup_converges`). Each of the eight live gates prints exactly one stderr line `LIVE-RAN <fn name>` after it passes its skip guard and before its first assertion (never on the quiet skip). Run with `2>&1 | tee /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad/261102-cef-live.log` (scratchpad, never committed; values stay unprinted). The full gate fails unless that log shows every gate's `LIVE-RAN` line and `... ok`, so a run without `IGNITION_LIVE_PAIR=1` cannot pass it.
       - **udts.json.** If gw-b re-serializes `udts.json` (a `file_differs` on `udts.json` whose parsed JSON is equal), add `("udts.json", &[])` to `FILE_NORMALIZATION` with a unit test in `pair/tests.rs`, and re-run. Record the capture either way.
       - **Failures.** If a gate fails, debug it with superpowers systematic-debugging. Never loosen an assertion.
       - **Tear down.** `docker compose -p ign-pair-rig down -v`.
    5. **SUMMARY.** Write `261102-cef-SUMMARY.md` in the task directory. It records:
       - O1-O5 and P1-P15;
       - the RED evidence per task (failing names) and the commit SHAs;
       - a `## Live` section with each command (env named, never printed), its UTC time and every `test result:` line, the `udts.json` and floor-3 (A3) observations, then exactly one line `Live verdict: pass`;
       - open items, not done here:
         - the Windows deploy-account ACL capture on core tag folders before production use (O4, A4);
         - the production pair still holding mode-collection tag paths, to be moved by a migration apply or runbook T6e under the operator's prod-backups-first rule;
         - the recommended warning when a backup converged only after the resync while projectState reads SyncError, carried from the debug doc;
       - no production host names or IPs.
    6. **Full gate and commit.**
       - Run the verify command.
       - Commit `feat(261102-cef): routed pair verify, README, live pair gates`, staging every changed file of this task by explicit path, including the SUMMARY (H1).
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && S=.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md && test "$(grep -cE '^Live verdict: pass$' "$S")" = 1 && test "$(grep -c 'test result: ok' "$S")" -ge 1 && grep -q 'live_pair_core_migration_converges' crates/ignition-core/tests/live_deploy_pair.rs && grep -q 'live_pair_core_tag_units_converge' crates/ignition-core/tests/live_deploy_pair.rs && grep -q 'tag-type-definition' crates/ignition-core/tests/live_deploy_pair.rs && grep -q '\.core/' README.md && test "$(grep -c 'plan_with(' crates/ignition-core/src/actions/deploy.rs)" -ge 3 && cargo test -p ignition-core --test deploy_pair pair_verify_ 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=2)}' && cargo test -p ignition-core --test deploy_pull pull_pair_core_tags_fixpoint 2>&1 | grep -E '^test result: ok\. 1 passed' && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep -E '^test result' | tail -3 && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && git diff --quiet 1e9482d -- Cargo.toml Cargo.lock crates/ignition-cli/Cargo.toml crates/ignition-core/Cargo.toml crates/ignition-tui/Cargo.toml && ! git log --name-only --format= 1e9482d..HEAD | grep -qE '^\.planning/config\.json$|^\.planning/research/\.cache/|-uat4\.ps1$' && ! git diff 1e9482d..HEAD | grep -E '^\+' | grep -vE '127\.0\.0\.1|0\.0\.0\.0|192\.0\.2\.' | grep -qE '(^|[^0-9.])([0-9]{1,3}\.){3}[0-9]{1,3}([^0-9.]|$)' && ! git log --format=%B 1e9482d..HEAD | grep -qi 'co-authored-by' && test -z "$(docker ps -q --filter label=com.docker.compose.project=ign-pair-rig)" && grep -rqE 'fn pair_verify_core_units_green\(' crates/ && grep -rqE 'fn pair_verify_warns_on_mode_tag_files\(' crates/ && grep -rqE 'fn pull_pair_core_tags_fixpoint\(' crates/ && grep -rqE 'fn live_pair_core_tag_units_converge\(' crates/ && grep -rqE 'fn live_pair_core_migration_converges\(' crates/ && L=/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad/261102-cef-live.log && test -s "$L" && ! grep -qE '^test result: .*[1-9][0-9]* failed' "$L" && ( for t in live_pair_core_tag_units_converge live_pair_core_migration_converges live_pair_refuses_a_tag_unit live_pair_converges live_pair_converges_with_a_stale_signature live_pair_unsynced_then_reconverge live_pair_secret_names live_pair_tag_cleanup_converges; do grep -qx "LIVE-RAN $t" "$L" && grep -qE "^test (.*::)?$t \.\.\. ok$" "$L" || exit 1; done ) && echo FULL-GATE-OK</automated>
  </verify>
  <done>`FULL-GATE-OK` is printed.
- **Full gate.** fmt, workspace clippy, every `cargo test --workspace` result at 0 failed, and the windows-msvc `cargo xwin clippy` are green.
- **Goldens and scope.** The four plan/render goldens and the fingerprint `GOLDEN` are byte-identical. Since 1e9482d there is no Cargo.toml or Cargo.lock change, no H1 file committed, no IP-shaped literal added beyond the loopback and documentation ranges, and no co-author line.
- **Verify, pull and README.** Verify is routed and warns on mode residue. The pull fixpoint holds. README documents the A2 rules, the migration, the core grant and the `.core/` spelling.
- **Live.** SUMMARY carries `Live verdict: pass`, with the pair rig's `test result: ok` lines for the new and existing pair gates. The rig is torn down.
- **Commits.** Three commits on `feat/deploy-tags-core` (one per task). Nothing is pushed.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| engine -> on-box work list, journal, record, ledger | qualified keys cross into files that a host script and later ign runs parse |
| host script -> gateway data dir (two roots) | writes now land in `config/resources/core` as well as the mode collection |
| repo/ledger -> gateway core tag folders | ign-managed and Designer-managed tag content share one collection |
| deploy account -> host ACLs | a new write grant on core tag type folders |
| live tests -> pair rig | tokens and bind-mounted data dirs during the Docker gate |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-cef-01 | Tampering | `.core/` key parsing (batch.rs, host.rs `BatchSpec::check`, `journal::check_touched`, `Ledger::check_files`, commit.sh `check_work`, commit.ps1 `CheckWork`) | high | mitigate | `apply_key_problem` splits the qualifier once, admits only `ignition/tag-definition` / `ignition/tag-type-definition`, and runs the unchanged component rules on the remainder. Both scripts refuse any other `.core/` key, and the core root is derived as `${CO%/*}/core`, never taken from input. The work list and script stay hash-pinned by the stub. Covered by `core_key_apply_key_problem`, the `core_key_outside_tag_types_refused` row (prepare and restore, all hosts) and `batch_core_key_round_trip` |
| T-cef-02 | Tampering (downgrade) | older ign reading a newer ledger, journal or record | medium | mitigate | A leading dot component fails today's validators (`deploy_ledger_invalid` / `deploy_run_invalid`), so an older binary fails closed instead of writing `<mode>/.core/...`. `core_key_apply_key_problem` pins that a bare `.core/...` still fails `mirror_path_problem` |
| T-cef-03 | Tampering / Repudiation (silent overwrite) | ign writes into Designer-managed core tag folders (D2) | high | mitigate | Status compares against the core copy (`changed`, core `differs`). With `--base`, drift refuses without `--allow-drift`. ign deletes a core file only when its ledger lists it as `.core/`, and an ambiguous removal stays `shadows_removal` -> `deploy_blocked`. Covered by `core_route_drift_on_core_copy` and `core_route_ambiguous_removal_shadows` |
| T-cef-04 | Denial of service (pair wedge) | mode-collection residue on a pair | high | mitigate | The residue rule in `pair_tag_keys` drives the gate, the engine guard and the plan warning. The migration deletes the mode copy and its emptied type folder (floor 2) in the same commit, and verify warns (O2). The live gates assert no snapshot mismatch and a following change converging without a resync |
| T-cef-05 | Elevation of privilege | new deploy-account grant on core tag folders | medium | mitigate | The grant is the narrowest that works (the two core tag type folders), and README documents it. The `write_host` probe fails closed before any write (P10, `core_probe_checks_core_root`). The Windows ACL behaviour is an open item in SUMMARY before production use (O4) |
| T-cef-06 | Tampering | core root missing, symlinked or replaced | medium | mitigate | No host ever creates `config/resources/core`. Every host applies the symlink and real-dir checks, and `core/ignition/<type>` is never removed (floor 3). Covered by `core_root_absent_never_created` and `pair_core_missing_core_root_writes_nothing` |
| T-cef-07 | Denial of service | rollback of a migration run re-wedges the pair | medium | mitigate | A manual rollback of a migration run is refused (`deploy_pair_tags_refused`), and a core-only run stays rollbackable. An automatic rollback restores the pre-apply state, which README documents along with re-apply as the recovery |
| T-cef-08 | Information disclosure | plan JSON, warnings, run files | low | accept | Only paths, keys and hashes are added, never file content. This is unchanged from today's contract |
| T-cef-09 | Elevation / Tampering | an accidental production call | critical | mitigate | H1: no production target, host name or address is used or committed. The live tests keep their loopback URL guard, and the full gate greps the diff for IP-shaped literals |
| T-cef-SC | Tampering | package installs | low | accept | No new crate. The full gate asserts no Cargo.toml or Cargo.lock change since 1e9482d |
</threat_model>

<verification>
- **Task 1:** the routing helpers, `plan_with`, the per-collection delete set, the residue rule, the engine and the local host pass their tests. The fake pair shows a tag unit landing in core, a one-commit migration, a core removal, auto-rollback and the missing-root refusal. The goldens are byte-identical.
- **Task 2:** the `_core_` conformance rows pass on the local and sh hosts and compile for ps. `commit.ps1` parses under pwsh. The `write_host` core probe fails closed, and the rollback rule holds.
- **Task 3:**
  - Verify is routed and warns on mode residue, and the pull fixpoint holds. README is updated.
  - The live gates on the Docker 8.3.6 pair rig pass: tag-definition plus tag-type-definition convergence with no resync and no snapshot mismatch, the migration converging in one apply, the residue refusal and the existing pair gates.
  - The full gate is green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings`.
- **Scope:** no worktree, version bump, push, PR or production call. The H1 files are never staged.
</verification>

<success_criteria>
- A pair accepts tag-definition and tag-type-definition deploys again. They land in core, the backup converges without a resync, and later changes keep syncing.
- A pair already wedged by mode-collection tag units recovers with one `--allow-destructive` apply that moves them to core.
- Standalone deploys are byte-for-byte what they were.
- Designer edits in ign-managed core tag folders are drift, never silently overwritten.
- Every host writes core the same way, fails closed without the core grant, and the conformance rows prove it.
- The full gate and the live pair gate are green, with three commits on `feat/deploy-tags-core` and the open items recorded in SUMMARY.
</success_criteria>

<output>
Create `.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md` when done (Task 3, step 5). It records:
- O1-O5 and P1-P15;
- the RED evidence per task and the commit SHAs;
- the `## Live` section with `Live verdict: pass`, the `udts.json` capture and the floor-3 observation;
- the open items: the Windows ACL capture, the production pair migration under prod-backups-first, and the resync-only convergence warning.
</output>
