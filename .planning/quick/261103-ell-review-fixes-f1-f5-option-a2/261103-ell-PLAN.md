---
phase: quick-261103-ell
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-core/src/deploy/apply/rollback.rs
  - crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/scripts/commit.sh
  - crates/ignition-core/src/deploy/transport/scripts/commit.ps1
  - crates/ignition-core/src/deploy/transport/local/apply_ops.rs
  - crates/ignition-core/src/deploy/transport/mod.rs
  - crates/ignition-core/src/deploy/transport/winps.rs
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/src/deploy/apply/gates.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/error.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/deploy_host_conformance.rs
  - crates/ignition-core/tests/deploy_plan.rs
  - crates/ignition-core/tests/deploy_pair.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - README.md
  - .planning/quick/261103-ell-review-fixes-f1-f5-option-a2/261103-ell-SUMMARY.md
autonomous: true
requirements: [QUICK-ell]

estimate:
  tokens: 500000
  raw_tokens: 500000
  tasks: 5
  confidence: low

must_haves:
  truths:
    - "F1: an interrupted pair apply that replaced one `.core/` file and deleted others recovers through `ign deploy rollback --run <id>`, both with its run record and with the record lost (journal pre-fingerprint path): outcome `restored`, every core file byte-equal to before the run, the ledger back, `core/ignition/tag-definition` kept. Recovery reads a `.core/` pre-image from the snapshot's `core/` slot, never `files/`"
    - "F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests). The sftp host's prepare stages every write in the slot `apply::snapshot_slot` names, so there is one slot rule in the crate"
    - "F4: on every host (local, `commit.sh`, `commit.ps1`), a restore whose `.core/` records are all files the run created (pre `-`) succeeds when `config/resources/core` is missing: those records are `Removed`, the mode pre-images are put back, and no host creates `core`. A `.core/` record with a pre-image still fails naming the core collection, and a symlinked core root is still refused"
    - "F5: on every host, the `write_host` probe of a run touching a `.core/` key reports not writable, naming the folder, when `core/ignition/<tag type>` or `core/ignition` is a symlink, not a directory, or (unix) a directory the deploy account cannot open or search. It never probes a parent past such a folder, so the run refuses at preflight instead of failing mid-commit"
    - "F3: on a redundant pair, an `ignition/tag-definition` or `ignition/tag-type-definition` folder in the master's mode collection under which the plan read no file (empty, only empty folders, or only files of undeclared providers) is residue: `deploy plan` names it in its one pair warning, `deploy apply` refuses `deploy_pair_tags_refused` (exit 6) naming it before anything is written (gate and engine guard), and `deploy verify` names it in its pair warning. This holds when the folder is the only residue: `pair_tags_refusal`, the `run_apply` guard and both pair warnings test the combined list (units or files plus folders) for emptiness. Standalone targets, `plan_sha256` and the plan JSON key set are unchanged"
    - "F2: the Docker 8.3.6 pair rig check (Task 3a) is recorded (scratch log and SUMMARY) with exactly one line `F2 rig verdict: core|mode|inconclusive|blocked`. Only on `core`, a mode-collection copy of a declared tag unit on a pair fails the master `files` check with a `files_differ` finding naming the mode paths, and the pair warning stops naming those paths. On any other verdict, verify keeps today's warning and no F2 code lands"
    - "README's gateway-tag-write sentence (about line 2643) matches the verdict, in exactly one commit: on `core` it says edits of an existing core tag were checked and go to `core`; on `mode` it says gateway edits land in the mode collection and re-wedge a pair; on `inconclusive` it says where edits land was not established; on `blocked` it is unchanged"
    - "No slug, exit code, Cargo.toml or Cargo.lock changes. `PLAN_GOLDEN*`, `RENDER_GOLDEN` and the fingerprint `GOLDEN` are byte-identical, checked by presence and by `git diff -U0 9a2e268` showing no changed golden or 64-hex line. fmt, workspace clippy, `cargo test --workspace` and the windows-msvc xwin clippy are green. The uncommitted SUMMARY holds no IP-shaped literal and no production host name"
  artifacts:
    - path: "crates/ignition-core/src/deploy/apply/rollback.rs"
      provides: "`snapshot_key` delegating every non-ledger key to `snapshot_slot`, and its unit test"
      contains: "snapshot_slot("
    - path: "crates/ignition-core/tests/deploy_apply.rs"
      provides: "the torn pair run recovery tests (F1) and the empty mode type folder engine tests (F3)"
      contains: "pair_core_torn_run_recovers_core_keys"
    - path: "crates/ignition-core/tests/deploy_host_conformance.rs"
      provides: "the `core_restore_created_only_without_core_root` (F4) and `core_probe_refuses_unsafe_type_folder` (F5) rows for the local, sh and ps hosts"
      contains: "core_probe_refuses_unsafe_type_folder"
    - path: "crates/ignition-core/src/deploy/transport/mod.rs"
      provides: "the `core_tags_unsafe:<tag type>` probe code text in `core_probe_problem`"
      contains: "core_tags_unsafe"
    - path: "crates/ignition-core/src/deploy/transport/scripts/commit.sh"
      provides: "the created-only core restore rule and the probe walk that refuses an unsafe folder"
      contains: "core_tags_unsafe"
    - path: "crates/ignition-core/src/deploy/transport/scripts/commit.ps1"
      provides: "the same two rules in PowerShell"
      contains: "core_tags_unsafe"
    - path: "crates/ignition-core/src/deploy/plan.rs"
      provides: "`pair_mode_tag_dirs`, the pure F3 rule"
      contains: "pub fn pair_mode_tag_dirs"
    - path: "crates/ignition-core/src/actions/deploy.rs"
      provides: "the pair-only `list_types` read in `deploy_plan` and `deploy_verify_planned`, the folded pair warnings, and the hidden `DeployPlanResult.pair_mode_tag_dirs`"
      contains: "pair_mode_tag_dirs"
    - path: "README.md"
      provides: "Tags and redundant pairs: the empty-folder residue, the probe and restore rules, the verify rule per the rig verdict, and the corrected gateway-tag-write sentence"
      contains: "Tags and redundant pairs"
  key_links:
    - from: "apply::rollback::snapshot_record (record check and journal-fingerprint path)"
      to: "apply::snapshot_slot"
      via: "snapshot_key maps every non-ledger key through the one slot rule"
      pattern: "snapshot_slot\\("
    - from: "transport::sftp::apply_ops prepare staging"
      to: "apply::snapshot_slot"
      via: "the staged copy of a write lands in the slot the host scripts read"
      pattern: "snapshot_slot\\("
    - from: "actions::deploy::deploy_plan (pair only)"
      to: "TreeReader::list_types(mode) -> plan::pair_mode_tag_dirs -> DeployPlanResult.pair_mode_tag_dirs"
      via: "the residue folders ride the plan to the gate and the engine guard"
      pattern: "pair_mode_tag_dirs"
    - from: "apply::gates::pair_tags_refusal (evaluate_gates and the run_apply guard)"
      to: "pair_tag_keys plus DeployPlanResult.pair_mode_tag_dirs"
      via: "one refusal definition for units and folders"
      pattern: "pair_mode_tag_dirs"
    - from: "local probe_paths_blocking, commit.sh op_probe, commit.ps1 OpProbe"
      to: "transport::core_probe_problem"
      via: "the same `core_tags_unsafe` / `core_tags_unwritable` codes and texts on every host"
      pattern: "core_tags_unsafe"
---

<objective>
**Execution: five tasks in order; prefer one executor run per task with a fresh context.** Each finding ends in its own commit (test and code together), so every commit builds and tests green.

| Task | Content | Gate token |
|------|---------|------------|
| 1 (tracer) | F1 + F7 | `T1-OK` |
| 2 | F4, then F5 | `T2-OK` |
| 3a | the F2 rig check (no repo change); its verdict decides 3c | `T3A-OK` |
| 3b | F3 | `T3B-OK` |
| 3c | F2 (only on a `core` verdict), the README sentence on gateway tag writes, the live pair suite, SUMMARY, the full gate | `FULL-GATE-OK` |

Fix the code-review findings F1-F5 (plus F7, which rides F1) on branch `feat/deploy-tags-core` for option A2 (quick 261102-cef and 261102-kus). **For each of F1-F5, write a failing offline test that reproduces it first and record the failing test names and messages (RED) for SUMMARY.** If a finding's test passes before any code change, the finding does not reproduce: make no code change for it, keep the test as a pin, and record `F<n>: not reproduced` with the reason in SUMMARY.

**F7 is exempt from RED (orchestrator decision).** It is a behaviour-preserving refactor: sftp prepare already builds the right slot, and the existing sftp staging tests cover it. Its SUMMARY line is exactly `F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests)`.

- **F1 (confirmed).** Interrupted-run recovery looks for every snapshot copy under `files/`, but `.core/` keys are stored under `core/` (`apply::snapshot_slot`). A torn pair run that touched a core tag unit with a pre-image fails `rollback_failed` ("the snapshot is missing or damaged"; without a record, "the snapshot is incomplete").
- **F7 (fixed, refactor, exempt from RED; orchestrator decision; behaviour already correct, covered by existing sftp staging tests).** sftp prepare builds the staging slot with its own inline match instead of `snapshot_slot`.
- **F4.** All three hosts fail a whole restore when `config/resources/core` is missing, even when every `.core/` record is a created file whose removal is a no-op, so the mode pre-images are never put back.
- **F5.** All three hosts' `write_host` probes skip a `core/ignition/<type>` (or `core/ignition`) folder that is a symlink, not a directory or unopenable, and probe its parent instead, so the run passes preflight and fails mid-commit.
- **F3.** On a pair, an empty mode-collection tag type folder alone wedges the backup (debug T6) but plan, apply and verify never report it, because they look only at file rows.
- **F2 (user decision: verify fails, but rig-check first).** On a pair, a mode-collection copy of a declared tag unit (which the gateway runs over core, and which blocks backup sync) passes the `files` check and is only a warning. First observe on the Docker pair rig where the gateway writes an edit of an existing core tag. Make verify fail only if the edit lands in `core`.

Out of scope, never planned here: F6 (typed `ApplyKey` refactor), F8 (helper dedupe).

Source coverage (every item has a task):

| Item | Source | Task |
|------|--------|------|
| F1 torn pair run recovery | orchestrator, confirmed | 1 |
| F7 one slot rule in sftp prepare (exempt from RED) | orchestrator | 1 |
| F4 created-only core restore | review | 2 |
| F5 probe refuses an unsafe core folder | review | 2 |
| F2 rig precondition (Docker only) | user decision | 3a |
| F3 empty mode type folder as residue | review, scoped by the planner (see context) | 3b |
| F2 verify fails on a mode copy (conditional), README sentence on gateway tag writes | user decision | 3c |
| Gates, goldens, hygiene | orchestrator constraints | every task, full gate in 3c |

Purpose: a torn pair run is recoverable, a restore never strands the mode copies, preflight refuses what the commit would trip over, and a pair whose mode collection still blocks backup sync is refused or reported instead of passing silently.

Output: code, offline and conformance tests, README, a scratch rig log and SUMMARY. No version bump, tag, push, PR, worktree or production call.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-VERIFICATION.md
@.planning/quick/261102-kus-base-drift-on-pair-tag-migration/261102-kus-SUMMARY.md

Option A2's binding rules (P1-P15: the `.core/` key, routing, slots, emptied-dir floors, probe codes, rollback, verify) are in `.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-PLAN.md`, lines 150-215. It is a 75 KB file: read only that range, and only when a rule below is unclear. README section `#### Tags and redundant pairs` (grep it, about line 2626) describes the shipped behaviour.

Line numbers below are as of commit 9a2e268. Grep the quoted anchor first, then read around it.

**Planner findings (binding; do not re-investigate):**
- **F1 mechanics.** `fetch_snapshot` on the local and sftp hosts returns hashes keyed by the path below `snapshots/<run>/`: `files/<path>` for mode keys, `core/<rest>` for `.core/` keys, `ledger.json`, `record.json`. `rollback.rs` `snapshot_key` (about line 869) maps every non-ledger path to `files/<path>`, and both of its call sites in `snapshot_record` (about lines 931 and 949) use it. `apply/mod.rs` `pub fn snapshot_slot` (about line 1691) is the correct rule and is already used by `snapshot_warnings`.
- **F4 locations.** `commit.sh` `op_restore` (about line 1104) refuses when `HAVE_CORE` is set and the core root is not a real directory. `commit.ps1` `OpRestore` (about line 1054) does the same with `$S.HaveCore`. On the local host, `restore_blocking` (about line 1205) calls `core_for`, which fails on a missing core root, before `restore_one`, whose own `None if pre.is_none()` branch already returns `Removed`. The scripts' `restore_one` / `RestoreOne` already treat an absent created file as removed.
- **F5 locations.** Local `probe_paths_blocking` (about line 418) tries `root.open` on each level and falls through to the parent on any error. `commit.sh` `op_probe` (about line 1186) and `commit.ps1` `OpProbe` (about line 1172) skip a level that is not a real directory. The probe codes are mapped to text by `transport::core_probe_problem` (about line 683), which the local host and both sftp mapping sites (`sftp/apply_ops.rs` about lines 602 and 1061) share.
- **F3 scoping.** `TreeReader::read` returns files only (`ObservedFiles` has no directories), so plan and verify cannot see an empty folder from it. `TreeReader::list_types(collection)` lists the `<module>/<type>` directories of a collection whatever they hold, on the local and sftp (Linux and Windows) readers, and the test wrappers (`MarkUnreadable`, `Counting`) delegate it. It lists nothing below the type folder. So the fix covers a tag type folder that is listed while the plan read no file under it: empty, holding only empty folders, or holding only files of providers outside the read scope. It does **not** cover a type folder that holds in-scope files the migration deletes **plus** such other content: seeing that before the commit needs a recursive directory listing in every reader (a new `TreeReader` method on the local and sftp readers and three test readers). That case is recorded as an open item in SUMMARY, not built. After such an apply, the folder holds no read file, so the post-apply verify reports it in its pair warning.
- **F3 contract.** `contract_deploy.rs` pins the plan JSON key set exactly (about line 607). So the residue folders ride `DeployPlanResult` in a `#[serde(skip)]` field. They are reported through the existing pair warning and the refusal, and are not part of `plan_sha256` (like `excluded_tag_providers`).
- **F3 early returns (B2).** `gates.rs` `pair_tags_refusal` (about lines 46-58) returns `None` as soon as `pair_tag_keys` is empty, and `pair_tags_warning` and `pair_mode_tags_warning` in `actions/deploy.rs` return early when their own list is empty. Left as they are, folder-only residue would never refuse or warn. Every one of these guards must test the combined list (units or files plus folders), and the `run_apply` guard must reach the refusal only through `pair_tags_refusal`.
- **F3 consistency.** cef P5 keeps `unmanaged` entries out of the refusal, and that stays. The folder rule fires only when the plan read no file at all under the folder, so it never refuses a migration whose mode copy the delete set clears. A folder that holds only `unmanaged` in-scope files is unchanged: verify warns, apply does not refuse.
- **F2 prior evidence.** Debug session deploy-tag-redundancy-sync control C3 (`.planning/debug/resolved/deploy-tag-redundancy-sync.md`, about line 142) showed that a tag folder *created* through `POST /data/api/v1/tags/import?...&collisionPolicy=Abort` lands in `core` on a gateway in mode `production`. An *edit of an existing core tag* was never captured, so README line 2643 ("The gateway's own tag writes (Designer, `tags/import`) go to `core`...") rests on the create only. The rig check fills that gap.
- **Rig hazards.** `setup-pair.sh pair` deadlocked under Homebrew bash 5.3 in quick 261102-cef (a here-string write blocked on its own pipe). That run then used `/bin/bash` for `link` and `mode`, and ran the live gates with a PATH shim whose `bash` is `/bin/bash`. A blocked `setup-pair.sh pair` process tree from that session may still exist. **Never kill it; never `pkill` or `kill` any process this executor did not start.**

**Hard rules (H1):**
- No production calls of any kind. Live work uses the Docker pair rig on loopback URLs only (`http://localhost:18300` gw-a, `http://localhost:18301` gw-b).
- No production host names or IP addresses in any committed file (public repo).
- No git worktrees. Run every cargo command sequentially on the main checkout. If phantom E0432 errors or vanished tests appear, rebuild in an isolated `CARGO_TARGET_DIR` before debugging code (`.claude/CLAUDE.md`).
- Stage files by explicit path only. Never stage `.planning/config.json`, `.planning/research/.cache/`, `svc-ignition-deploy@52.154.75ace-uat4.ps1` or `.planning/debug/tg-pg-table-check.md`. `git add -p` is interactive and unavailable, so finish one finding (RED, fix, gate) and commit it before starting the next finding in the same file.
- No co-author lines. No Cargo.toml or Cargo.lock change. Do not push or open a PR. Do not commit this PLAN or the SUMMARY; the orchestrator's docs commit carries them (as in 261102-kus).
- No slug change (so the four-place rule does not apply). If one turns out to be unavoidable, stop and report instead.
- `PLAN_GOLDEN*`, `RENDER_GOLDEN` and the fingerprint `GOLDEN` stay byte-identical. No finding here requires a change.
- Tokens are never printed. Send them as `-H "X-Ignition-API-Token: $IGN_PAIR_TOKEN_A"` from the sourced env, never with `curl -v` or `set -x`, and never write them to a log.
- Match the surrounding comment density and idiom. Doc comments state the rule and cite the finding (`quick 261103-ell, F<n>`) or capture (debug T6, C3). No narration.
- Follow superpowers test-driven-development per finding and superpowers systematic-debugging for any failure. Never weaken an assertion, skip a test, or mark a test ignored to get green.
- The scratchpad is `/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad` (create it if missing). Logs go there, never into the repo.
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1 (tracer): a torn pair run that touched core keys recovers end to end (F1), and sftp prepare uses the one slot rule (F7)</name>
  <files>crates/ignition-core/src/deploy/apply/rollback.rs, crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs, crates/ignition-core/tests/deploy_apply.rs</files>
  <precondition>Branch `feat/deploy-tags-core` is checked out at 9a2e268 or a descendant, and `git status --short` shows only the four H1 files and this task directory as non-HEAD changes.</precondition>
  <read_first>
    - `crates/ignition-core/src/deploy/apply/rollback.rs`: grep `fn snapshot_key`, then read through the end of `fn snapshot_record` (about 862-990); grep `mod tests` for the helper style of its unit tests.
    - `crates/ignition-core/src/deploy/apply/mod.rs`: grep `pub fn snapshot_slot` (about 1688-1696).
    - `crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs`: grep `is staged in the` (about 1364-1374, the staging loop of prepare).
    - `crates/ignition-core/tests/deploy_apply.rs`:
      - `struct Killed`, `async fn killed_mid_write` and `async fn kill_apply` (about 4498-4545);
      - `fn unmove` and `async fn recover` (about 4705-4721), and the record-lost tests around 4876-4910 (`lose the record`);
      - `struct Fresh` with its `run` (about 3683-3770): it passes `backup: None`;
      - the A2 block from `const C_CANARY_TAGS` (about 7628) through `pair_core_removed_unit_deletes_core_copy` (about 7920-7965): `core_pair_rig`, `migration_seed`, `destructive_flags`, `backup_check`.
  </read_first>
  <behavior>
    - snapshot_key_core_key_reads_the_core_slot (lib, `rollback.rs` tests): `.core/ignition/tag-definition/default/Deploy/tags.json` maps to `core/ignition/tag-definition/default/Deploy/tags.json`. A bare collection path maps to `files/<path>`, and `ledger.json` maps to `ledger.json`.
    - pair_core_torn_run_recovers_core_keys:
      - Setup: the pair rig's gw-a `core` holds `ignition/tag-definition/default/Deploy` (an older `tags.json` plus its marker) and a second unit `default/Old` (`tags.json` plus marker). The ledger lists the `.core/` paths of both. The repo declares Deploy with new bytes and no longer declares Old.
      - Run: the apply runs with `allow_destructive` and is killed after its first move (`ApplyFault::HangAfterMove(1, ..)`).
      - Recovery: `recover(&rig, &run_id)` from a fresh process ends `Ok` with outcome `Restored`. Deploy's core `tags.json` is the old bytes again, Old's two files are back byte-equal, and the ledger is the pre-run bytes. The journal is gone, `core/ignition/tag-definition` exists, and the mode collection is unchanged.
      - Before the fix this is `rollback_failed` with "the snapshot is missing or damaged".
    - pair_core_torn_run_recovers_core_keys_without_record: the same, but `record.json` is removed from the run's snapshot directory before recovery, so it takes the journal pre-fingerprint path. It reaches the same end state. Before the fix this is `rollback_failed` with "the snapshot is incomplete".
  </behavior>
  <action>
    This task implements F1 and F7.

    1. **RED.**
       - Write the three tests above.
       - In `deploy_apply.rs`, add a faulted variant of `core_pair_rig` (a host built with `local_host(&fx).with_apply_fault(..)`).
       - Add a `GateFlags`-taking sibling of `kill_apply`, because this scenario deletes and needs `allow_destructive`. `kill_apply` itself stays as it is.
       - Keep the `PairLink` alive for the whole test.
       - If the pair recovery needs the backup handle, pass it the way the existing pair rollback tests do (grep `backup: Some` in `deploy_apply.rs`). Otherwise keep `Fresh` unchanged.
       - Run `cargo test -p ignition-core --lib snapshot_key` and `cargo test -p ignition-core --test deploy_apply pair_core_torn_run`. Record the failing names and the `rollback_failed` texts for SUMMARY.
    2. **F1 fix.**
       - `snapshot_key` in `rollback.rs` keeps its ledger case.
       - It maps every other key through `super::snapshot_slot`, the one helper that knows both slots.
       - Update its doc comment: the snapshot spells a key as `ledger.json`, or as the slot `snapshot_slot` names (`files/` for a mode path, `core/` for a `.core/` key).
       - The two call sites in `snapshot_record` need no other change.
    3. **F7 fix.**
       - In sftp prepare's staging loop, take each write's slot from `crate::deploy::apply::snapshot_slot(op.path.as_str())`.
       - Remove the local `split_key` match that built it, and any import that change leaves unused.
       - The strings are identical, so there is no behaviour change and no new test: F7 is exempt from RED by orchestrator decision, and the existing sftp staging tests cover it. Its SUMMARY line is the exact F7 form in the objective.
       - Confirm by reading the diff that sftp prepare no longer builds a slot by hand. clippy's unused-import check catches a leftover `split_key` / `KeyCollection` import.
    4. **Commit.**
       - Run the verify command.
       - Commit `fix(261103-ell): recover a torn pair run's core keys from the core snapshot slot (F1, F7)`, staging the three files by explicit path.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --lib snapshot_key_core_key_reads_the_core_slot 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_apply pair_core_torn_run 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=2)}' && cargo test -p ignition-core --lib rollback:: 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib transport::sftp 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --test deploy_apply --test deploy_transport --test deploy_host_conformance 2>&1 | grep -E '^test result' && grep -q 'snapshot_slot(' crates/ignition-core/src/deploy/apply/rollback.rs && grep -q 'snapshot_slot(' crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs && ! grep -qF 'format!("files/{path}")' crates/ignition-core/src/deploy/apply/rollback.rs && grep -rqE 'fn pair_core_torn_run_recovers_core_keys\(' crates/ && grep -rqE 'fn pair_core_torn_run_recovers_core_keys_without_record\(' crates/ && cargo clippy --workspace --all-targets -- -D warnings && echo T1-OK</automated>
  </verify>
  <done>`T1-OK` is printed.
- **Tests.** The two torn-run recoveries and the slot unit test pass, as do every rollback, sftp, apply, transport and conformance test.
- **Code.** `rollback.rs` and sftp prepare both use `snapshot_slot`. The diff review confirms that sftp prepare no longer builds a slot by hand, and clippy reports no unused import.
- **Commit.** One commit, staged by explicit path, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: a created-only core restore needs no core root (F4), then the write_host probe refuses an unsafe core folder (F5), on all three hosts</name>
  <files>crates/ignition-core/src/deploy/transport/scripts/commit.sh, crates/ignition-core/src/deploy/transport/scripts/commit.ps1, crates/ignition-core/src/deploy/transport/local/apply_ops.rs, crates/ignition-core/src/deploy/transport/mod.rs, crates/ignition-core/src/deploy/transport/sftp/apply_ops.rs, crates/ignition-core/src/deploy/transport/winps.rs, crates/ignition-core/tests/deploy_host_conformance.rs, README.md</files>
  <precondition>Task 1's commit is HEAD.</precondition>
  <read_first>
    - `scripts/commit.sh`:
      - `HAVE_CORE` (grep it; set in `check_work` around 373-405);
      - `check_roots` (about 446-475), `real_dir` (about 115), `classify`, `key_path` (about 244);
      - `rmdir_up` (about 853), `restore_one` (about 960-1055), `restore_pass` and `op_restore` (about 1074-1124);
      - `op_probe` (about 1150-1215).
    - `scripts/commit.ps1`: `KeyBase` / `KeyRel` / `KeySlot` (about 271), `RestoreOne` (about 930), `OpRestore` (about 1034-1070), `OpProbe` (about 1140-1195).
    - `transport/local/apply_ops.rs`:
      - `core_collection` and `core_for` (about 240-292);
      - `probe_paths_blocking` (about 411-460);
      - `restore_blocking` and `restore_one` (about 1146-1330);
      - grep `fn remove_emptied_keys`.
    - `transport/mod.rs`: `core_probe_problem` (about 678-702) and grep `fn core_tag_types`.
    - `transport/sftp/apply_ops.rs`: both `core_probe_problem` mapping sites (about 602 and 1061) and the probe-code unit tests (about 1835-1860).
    - `transport/winps.rs`: grep `core_tags_unwritable:` (about 1984) for the structural test of `commit.ps1`.
    - `tests/deploy_host_conformance.rs`:
      - `scenarios()` (about 62) and `Ctx` with `core_root` / `seed_core` / `core_listing` (about 197-215);
      - `rustix_is_root` (about 740);
      - the rows `core_migrate_mode_unit_in_one_commit`, `core_restore_migration_puts_mode_back` (about 1262), `core_replace_checks_pre_image`, `core_root_absent_never_created` (about 1379) and `core_probe_checks_core_root` (about 1478-1525);
      - the `run_local`, `run_sh` and `run_ps` wiring (about 1650, 1900, 2220).
    - README: grep `**Deletes and rollbacks**` and `**Host access.**` inside `#### Tags and redundant pairs`.
  </read_first>
  <behavior>
    Rows go in `scenarios()` and are wired as `conformance_local_<row>`, `conformance_sh_<row>` (`#[cfg(unix)]`) and `conformance_ps_<row>` (`#[cfg(windows)]`), like their neighbours.
    - core_restore_created_only_without_core_root (F4):
      - (a) Commit the migration of `core_migrate_mode_unit_in_one_commit`: write `.core/.../Deploy/*` as created files and delete the bare `.../Deploy/*` with their pre-images. Then remove `config/resources/core` entirely. `restore` of the run's record reports no failure:
        - the bare Deploy files are back in the mode collection, byte-equal;
        - every `.core/` index is in `deleted`;
        - `config/resources/core` still does not exist;
        - a second restore changes nothing.
      - (b) Commit a replacement whose `.core/` write carries a pre-image (the `core_replace_checks_pre_image` setup), then remove `config/resources/core`. `restore` reports a failure naming the core collection, no core directory exists afterwards, and the mode collection is unchanged.
    - core_probe_refuses_unsafe_type_folder (F5): the core root is a real, writable directory, and the probe runs over `touched` = a `.core/ignition/tag-definition/...` key plus `ledger.json`. In each case the probe reports `writable == false` with a problem containing `config/resources/core/ignition`, and it creates nothing:
      - (a) a regular file at `core/ignition/tag-definition`;
      - (b) a regular file at `core/ignition`, with no type folder;
      - (c, unix) `core/ignition/tag-definition` is a symlink to a writable directory outside core;
      - (d, unix, not root) `core/ignition/tag-definition` is a real directory with mode `0o000`, then with `0o200`. Restore `0o755` before asserting.
      `core_probe_checks_core_root` stays green unchanged.
    - Unit tests:
      - `core_probe_problem_names_an_unsafe_type_folder` in `transport/mod.rs` tests: `core_tags_unsafe:tag-definition` names `config/resources/core/ignition/tag-definition` and says symlink or not a directory, and `core_tags_unsafe:tag-group` is `None`.
      - The sftp probe-code test case for the new code.
      - The `winps.rs` structural test pins `core_tags_unsafe:` in `commit.ps1`.
  </behavior>
  <action>
    This task implements F4 and then F5. Each gets its own RED, fix, gate and commit, in that order.

    1. **F4 RED.**
       - Add the `core_restore_created_only_without_core_root` row and wire it for the three hosts.
       - Run `cargo test -p ignition-core --test deploy_host_conformance core_restore_created_only` (local and sh on this Mac). Record the failures.
    2. **F4 fix.** A restore fails for a missing (or non-directory) core root only when a `.core/` record carries a pre-image. A symlinked core root stays refused for every op on every host.
       - `commit.sh` `op_restore`: tie the core-root failure to a `.core/` P record with a pre-image (`b` not `-`). `check_roots` keeps its core symlink refusal. Confirm that `restore_one`'s absent branch and `rmdir_up` (which leaves a collection that is not a real directory alone) already handle a missing core root.
       - `commit.ps1` `OpRestore`: same rule. `IsLink` on the core root still fails; a missing or non-directory root fails only for a `.core/` record with a pre-image.
       - Local `restore_blocking`:
         - Open core strictly (`core_for`) only when a `.core/` restore carries a pre-image.
         - When only created `.core/` files are restored, open core if it is a real directory. Treat it as absent (`None`, so `restore_one` reports `Removed`) when it is missing. Still refuse a symlink or a non-directory.
         - `remove_emptied_keys` skips `.core/` keys when core is `None`.
       - README, `**Deletes and rollbacks**` bullet: add that a rollback whose `core` records are only files the run created succeeds even when `config/resources/core/` is gone, while one with a `core` pre-image to put back still needs it.
       - Run the F4 subset of the verify command (the `core_restore_created_only` filter, every conformance row, `sh -n`, the pwsh parse and workspace clippy).
       - Commit `fix(261103-ell): restore created-only core records without the core collection (F4)`.
    3. **F5 RED.**
       - Add the `core_probe_refuses_unsafe_type_folder` row (wired three ways) and the unit tests.
       - Run `cargo test -p ignition-core --test deploy_host_conformance core_probe_refuses_unsafe` and the unit tests. Record the failures.
    4. **F5 fix.** On every host the probe walks `core/ignition/<type>`, then `core/ignition`, and stops at the first level that exists:
       - **Missing:** go up one level.
       - **Real directory:** it must be writable and, where the host can tell, openable and searchable. Otherwise the code is `core_tags_unwritable:<type>`.
       - **Symlink or anything else:** the code is `core_tags_unsafe:<type>`. Never go past it to the parent.
       - **Neither level exists:** the core root, already checked to be a real directory, decides by the same writable test.

       Per host:
       - `transport/mod.rs` `core_probe_problem` maps `core_tags_unsafe:<type>`, for the two tag types only, to text naming `config/resources/core/ignition/<type>` (or the folder above it) as a symlink or not a directory, refused, and pointing at README Tags and redundant pairs. Update the doc's code list.
       - Local `probe_paths_blocking`: `root.kind` per level. A `Dir` that cannot be opened, or that is not `writable()`, is unwritable. A `Symlink` or another kind is unsafe.
       - `commit.sh` `op_probe`: `-L` gives unsafe; `-d` needs `-w` and `-x`, else unwritable; `-e` gives unsafe; otherwise go to the next level. The core root needs `-w` and `-x` too.
       - `commit.ps1` `OpProbe`: `Attr` `-1` means the next level; `IsLink` or not `IsDir` gives unsafe; otherwise `CanAddFile`, else unwritable.
       - Confirm both sftp mapping sites reach `core_probe_problem` for the new code, and add its unit case. Extend the `winps.rs` structural test.
       - README, `**Host access.**` bullet: add that a `core/ignition` or tag type folder that is a symlink or not a directory is refused there too, before any write, never probed through the folder above it.
       - Commit `fix(261103-ell): refuse a symlinked or non-directory core tag folder at the write_host probe (F5)`.
    5. The `conformance_ps_*` rows run only on the Windows CI leg. Here they must compile under xwin clippy, and `commit.ps1` must parse under pwsh. SUMMARY says so.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --test deploy_host_conformance core_restore_created_only 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=2)}' && cargo test -p ignition-core --test deploy_host_conformance core_probe_refuses_unsafe 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=2)}' && cargo test -p ignition-core --test deploy_host_conformance 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib core_probe_problem_names_an_unsafe_type_folder 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib transport:: 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --test deploy_apply --test deploy_transport 2>&1 | grep -E '^test result' && sh -n crates/ignition-core/src/deploy/transport/scripts/commit.sh && pwsh -NoProfile -NonInteractive -Command '$null = [ScriptBlock]::Create([IO.File]::ReadAllText("crates/ignition-core/src/deploy/transport/scripts/commit.ps1")); "PS1-PARSE-OK"' | grep -q PS1-PARSE-OK && grep -q 'core_tags_unsafe' crates/ignition-core/src/deploy/transport/scripts/commit.sh && grep -q 'core_tags_unsafe' crates/ignition-core/src/deploy/transport/scripts/commit.ps1 && grep -q 'core_tags_unsafe' crates/ignition-core/src/deploy/transport/mod.rs && grep -rqE 'fn core_restore_created_only_without_core_root\(' crates/ && grep -rqE 'fn core_probe_refuses_unsafe_type_folder\(' crates/ && grep -rqE 'fn conformance_ps_core_restore_created_only_without_core_root\(' crates/ && grep -rqE 'fn conformance_ps_core_probe_refuses_unsafe_type_folder\(' crates/ && cargo clippy --workspace --all-targets -- -D warnings && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && test "$(git log --format=%s 9a2e268..HEAD | grep -cE '^fix\(261103-ell\): .*\((F4|F5)\)$')" = 2 && echo T2-OK</automated>
  </verify>
  <done>`T2-OK` is printed.
- **Rows.** The two new rows pass on the local and sh hosts. The ps rows compile under windows-msvc xwin clippy, and `commit.ps1` parses under pwsh. Every existing conformance row still passes.
- **Host rules.** No host creates `config/resources/core`. A created-only core restore succeeds without it, and a core pre-image restore without it fails naming it. No host's probe looks past a symlinked or non-directory core tag folder.
- **Commits.** Two commits, F4 then F5, each staged by explicit path, with no co-author line.</done>
</task>

<task type="auto">
  <name>Task 3a: the F2 rig check on the Docker 8.3.6 pair (the verdict decides Task 3c)</name>
  <files>/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad/261103-ell-f2-rig.log (scratch only; this task changes no repo file and makes no commit)</files>
  <precondition>Task 2's two commits are HEAD.</precondition>
  <read_first>
    - `crates/ignition-core/tests/fixtures/live-rig-pair/setup-pair.sh`: `cmd_up`, `cmd_mint`, `cmd_pair`, `cmd_mode`, `cmd_status`, `cmd_link` (grep `^cmd_`).
    - `crates/ignition-core/tests/live_deploy_pair.rs` lines 1-50 (the env table) and 1521-1530 (`CANARY_TAGS_BYTES`, `CANARY_MARKER_BYTES`).
    - `.planning/debug/resolved/deploy-tag-redundancy-sync.md` lines 138-150 (controls C1 and C3).
  </read_first>
  <action>
    This task is the F2 precondition (user decision: verify fails only if the rig shows gateway edits of core tags land in `core`). It uses Docker and loopback URLs only.

    Set `SP` to the scratchpad path (H1), `RL=$SP/261103-ell-f2-rig.log` and `RIG=crates/ignition-core/tests/fixtures/live-rig-pair`. Log every command, its UTC time and its result to `$RL`, never a token value.

    1. **Docker.** Run `docker info`. If Docker is not running, `open -a Docker` and wait up to 180 s. If it never answers, append `F2 rig verdict: blocked` with the reason and end the task.
    2. **Bring the rig up.**
       - If containers of compose project `ign-pair-rig` are already running, run `docker compose -p ign-pair-rig down -v` first.
       - Create `$SP/sysbash/bash` as a symlink to `/bin/bash`.
       - From `$RIG`, run each step under a watchdog this executor starts itself: `timeout 900 /bin/bash ./setup-pair.sh <step>` for `up`, `mint`, `pair` and `mode`, in that order. `/bin/bash` sidesteps the bash 5.3 deadlock (context).
       - If a step hits the watchdog or exits non-zero, append `F2 rig verdict: blocked` naming the step and the UTC time, tear down with `docker compose -p ign-pair-rig down -v` and end the task. Never retry under another shell, and never kill any process except through that watchdog.
       - `timeout 120 /bin/bash ./setup-pair.sh status` must read: gw-a Master Active, gw-b Backup Cold, both Good, mode production. Otherwise the verdict is blocked, as above.
       - Source `~/.cache/ign-pair-rig/tokens.env` (it exports `IGN_PAIR_TOKEN_A` and `IGN_PAIR_TOKEN_B`; never echo them). The data dirs are `$HOME/.cache/ign-pair-rig/data-a` and `data-b`.
    3. **Tags live in core, the way A2 writes them.**
       - Write `config/resources/core/ignition/tag-definition/default/F2Probe/tags.json` into gw-a's data dir: the live gate's `CANARY_TAGS_BYTES` with the tag name `Probe`.
       - Write `unary-resource.json` beside it: `CANARY_MARKER_BYTES`.
       - Send `POST http://localhost:18300/data/api/v1/scan/config` with the token header.
       - Poll `GET /data/api/v1/tags/export?provider=default&type=json` on gw-a until `F2Probe/Probe` appears (up to 60 s).
       - Poll gw-b's data dir until `core/.../F2Probe/tags.json` exists (up to 90 s).
    4. **Pre-state.** Log `find <data-a>/config/resources -path '*/ignition/tag-definition*' | sort` and `shasum -a 256` of both F2Probe core files.
    5. **Edit the existing core tag through the gateway** (the closest native path to a Designer save):
       - Send `POST /data/api/v1/tags/import?provider=default&type=json&collisionPolicy=Overwrite` with `Content-Type: application/octet-stream`.
       - The body is `{"tags":[{"name":"F2Probe","tagType":"Folder","tags":[<Probe with one config property changed, e.g. "documentation":"f2-probe-edit">]}]}`.
       - Log the HTTP status and the `successCount` / `failureCount` body.
       - Poll every 5 s, for up to 90 s, until the export shows the changed property, then repeat the listing and the hashes.
    6. **Verdict.** Append exactly one line to `$RL`:
       - `F2 rig verdict: core` when the export shows the edit, the core `F2Probe/tags.json` hash changed, and no path under `<data-a>/config/resources/production/ignition/tag-definition` exists;
       - `F2 rig verdict: mode` when any such production path exists after the edit (record whether core changed too);
       - `F2 rig verdict: inconclusive` otherwise.

       Before declaring `inconclusive` (the edit was refused, or never showed), retry once with a folder the gateway created itself: `F2Probe2` through `tags/import ... collisionPolicy=Abort` (control C3), then the same Overwrite edit and observation. Decide on that.
    7. **Clean up.**
       - Remove the F2Probe folders (and F2Probe2, if made) from gw-a's core.
       - Remove any path the probe created under `production/ignition/tag-definition`, and that folder itself only if it was absent in the pre-state.
       - Scan. If anything was removed from the mode collection, send `POST /data/api/v1/redundancy/gwaction/resync` to gw-a.
       - Wait until `status` reads settled again (both Good, gw-b peerConnected) for up to 600 s. If it settles, leave the rig up for Task 3c's live suite; if not, tear it down and log that.
    8. On `mode` or `inconclusive`, F2 code is not built (Task 3c records the evidence). Tags a Designer edits on a pair would then land in the mode collection, which re-wedges the pair and conflicts with cef D2 ("Designer edits to ign-managed core tag folders are drift"); note this in `$RL` for SUMMARY.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && SP=/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad && RL=$SP/261103-ell-f2-rig.log && test "$(grep -cE '^F2 rig verdict: (core|mode|inconclusive|blocked)$' "$RL")" = 1 && ! find "$HOME/.cache/ign-pair-rig/data-a" "$HOME/.cache/ign-pair-rig/data-b" -path '*F2Probe*' 2>/dev/null | grep -q . && git diff --quiet HEAD -- crates README.md && test -z "$(git diff --cached --name-only)" && ! grep -qE 'X-Ignition-API-Token: [A-Za-z0-9_-]+:' "$RL" && echo T3A-OK</automated>
  </verify>
  <done>`T3A-OK` is printed.
- **Verdict.** The scratch log holds exactly one `F2 rig verdict:` line, with the commands, UTC times, API counts, listings and hashes that led to it.
- **Cleanup.** No probe folder is left on either node, and the rig is either settled and up (for Task 3c) or torn down.
- **Repo.** No repo file changed and nothing is staged or committed.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3b: an empty (or unseen-content) mode tag type folder on a pair is residue at plan, apply and verify (F3)</name>
  <files>crates/ignition-core/src/deploy/plan.rs, crates/ignition-core/src/actions/deploy.rs, crates/ignition-core/src/deploy/apply/gates.rs, crates/ignition-core/src/deploy/apply/mod.rs, crates/ignition-core/src/error.rs, crates/ignition-core/tests/deploy_plan.rs, crates/ignition-core/tests/deploy_apply.rs, crates/ignition-core/tests/deploy_pair.rs, README.md</files>
  <precondition>Task 2's two commits are HEAD (Task 3a makes no commit).</precondition>
  <read_first>
    - `crates/ignition-core/src/deploy/plan.rs`: `is_tag_folder`, `pair_tag_keys` and `pair_mode_tag_files` (about 490-560).
    - `crates/ignition-core/src/actions/deploy.rs`:
      - `DeployPlanResult` (about 276-312) and `deploy_plan` (about 338-455);
      - `pair_tags_warning` and `pair_mode_tags_warning` (about 457-510): both return early when their own list is empty;
      - `deploy_verify_planned` (about 1213-1300).
    - `crates/ignition-core/src/deploy/apply/gates.rs`: `pair_tags_refusal` (about 37-58), which returns `None` when `pair_tag_keys` is empty, and `fn plan_of` in its tests (about 317).
    - `crates/ignition-core/src/deploy/apply/mod.rs`: the `run_apply` guard (about 820-845) and `fn shell_plan` (about 2203).
    - `crates/ignition-core/src/error.rs`: `DeployPairTagsRefused` (about 895-910), `fn deploy_pair_tags_refused_text` (about 1128) and its hint (about 1911). Grep `deploy_pair_tags_refused_four_places`.
    - `crates/ignition-core/tests/deploy_plan.rs`: grep `DeployPlanResult {` (about 2596 and 2838).
    - `crates/ignition-core/tests/deploy_apply.rs`: the A2 block from `const C_CANARY_TAGS` (about 7628), `live_render_but_the_canary`, `core_pair_rig`, and `pair_run_apply_refuses_a_tag_plan_that_skipped_the_gates`.
    - `crates/ignition-core/tests/deploy_pair.rs`: `canary_repo`, `live_with_canary_in_core`, `make_pair`, `verify_pair`, `verify_both`, `check_ids`, `pair_warnings` (about 311-460).
    - README `#### Tags and redundant pairs` (about 2626-2740) and exit-table row `| 6 ` (about 271).
  </read_first>
  <behavior>
    Every folder test below has no residue unit and no mode tag file, so it fails if any empty guard still tests only the old list (B2).
    - pair_mode_tag_dirs_names_only_folders_with_no_read_file (`deploy_plan.rs`, pure):
      - `listed` holds `(ignition, tag-definition)`, `(ignition, tag-type-definition)`, `(ignition, tag-provider)` and `(other, x)`;
      - the entries hold a bare live row under `ignition/tag-type-definition/...`;
      - the result is exactly `["ignition/tag-definition"]`;
      - an empty `listed` gives `[]`.
    - pair_tags_refusal_names_mode_type_folders (lib, `gates.rs`): a plan whose `pair_mode_tag_dirs` holds `ignition/tag-definition` and whose entries are clean (`pair_tag_keys` empty) is refused on a pair, with that key in `keys`. The same plan on Standalone is `None`.
    - pair_core_empty_mode_type_folder_refused (`deploy_apply.rs`):
      - Setup: `core_pair_rig(true, ..)` whose seed is `live_render_but_the_canary` plus an empty `<mode>/ignition/tag-definition` directory.
      - `prepare(GateFlags::default())` is `deploy_pair_tags_refused` (exit 6), and its message names `ignition/tag-definition`.
      - The data directory is byte-identical afterwards, and the host made no commit.
      - The engine guard refuses the same: hand `run_apply` a Ready plan of the rig taken without the folder, with `ignition/tag-definition` put into its `pair_mode_tag_dirs` (follow the `pair_run_apply_refuses_a_tag_plan_that_skipped_the_gates` pattern).
    - pair_core_empty_mode_type_folder_plan_warns: `deploy_plan` on that rig has one pair warning naming `ignition/tag-definition`. Its `plan_sha256` equals that of the same rig without the folder.
    - standalone_empty_mode_type_folder_not_refused: a standalone rig with the same empty folder plans and prepares exactly as without it: same warnings, same `plan_sha256`, no pair refusal.
    - pair_verify_warns_on_empty_mode_type_folder (`deploy_pair.rs`):
      - Setup: a pair with the canary in core plus an empty `<mode>/ignition/tag-type-definition`, and no mode tag file.
      - `deploy_verify` has the same `ok` and check ids as without the folder, and `pair_warnings` holds one line naming `ignition/tag-type-definition`.
      - `--node both` carries it once.
    - The existing `pair_core_migration_moves_mode_unit_in_one_commit`, `pair_verify_core_units_green`, `pair_verify_warns_on_mode_tag_files` and `standalone_verify_no_pair_warning` stay green unchanged. The type folder is listed during a migration, but the plan read its files.
  </behavior>
  <action>
    This task implements F3.

    1. **RED.**
       - Write the tests above.
       - `pair_mode_tag_dirs` and the plan field must exist for the tests to compile, so add them first with real signatures and an empty body.
       - Run them and record the failing names and messages for SUMMARY.
    2. **`plan.rs`.** Add `pub fn pair_mode_tag_dirs(listed: &[(String, String)], entries: &[PlanEntry]) -> Vec<String>`.
       - It returns `ignition/<type>` for each listed `(ignition, <type>)` with `<type>` in `PAIR_UNSYNCED_TAG_TYPES`, when no entry of that type has a bare (mode) row with a live sha.
       - The result is byte-sorted and unique.
       - The doc states the rule, cites debug T6 and quick 261103-ell F3, and states the limit from the context: content beside in-scope files the migration deletes is not seen before the commit.
    3. **`actions/deploy.rs`.**
       - `DeployPlanResult` gains `pair_mode_tag_dirs: Vec<String>` with `#[serde(skip)]`. Its doc says what it is and that it is not serialized and not in `plan_sha256`.
       - `deploy_plan`, only when `nodes.backup` is set, reads `reader.list_types(mode)` after the read and fills the field. Standalone makes no extra call and keeps an empty field.
       - `deploy_verify_planned`, on a pair, reads `list_types(mode)` the same way.
       - **Both warnings (B2).** `pair_tags_warning` and `pair_mode_tags_warning` each take the folders. Each one's empty-return guard tests the combined list (its units or files plus the folders), not its old list alone. Each stays one line, naming units or files and then folders.
       - Fill the new field in every `DeployPlanResult` literal: `gates.rs` `plan_of`, `apply/mod.rs` `shell_plan`, and the two in `tests/deploy_plan.rs`.
    4. **`gates.rs` `pair_tags_refusal` (B2).**
       - Build the combined key list first: `pair_tag_keys(..)` together with `plan.pair_mode_tag_dirs`, byte-sorted and unique.
       - Return `None` only when that combined list is empty. The current early return on an empty `pair_tag_keys` result is what lets folder-only residue through, and it must go.
       - The `run_apply` guard in `apply/mod.rs` calls `pair_tags_refusal` and must keep no early return of its own on `pair_tag_keys` (check it).
       - Update both docs.
    5. **`error.rs`.**
       - The `DeployPairTagsRefused` text counts tag units or folders that would leave files or folders in, or restore files to, the mode collection.
       - The hint adds that an `ignition/tag-definition` or `ignition/tag-type-definition` folder with no file the deploy reads (an empty one included) counts too, and is removed by hand on the master (runbook T6e).
       - The slug and exit code are unchanged. Keep `deploy_pair_tags_refused_four_places` green, adjusting only text assertions.
    6. **README.**
       - The `**What is still refused.**` bullet names the folder case and the limit in one sentence.
       - The `**`deploy verify`**` bullet says the warning names such folders.
       - Exit-table row 6 prose reads "a tag unit or tag type folder a deploy would leave or restore in a redundant pair's mode collection". The slug list is unchanged.
       - Leave the gateway-tag-write sentence (about line 2643) alone here; Task 3c owns it (W6).
    7. **Goldens.** Add no 64-hex literal to `tests/deploy_plan.rs`, `apply/fingerprint.rs` or `crates/ignition-cli/tests/contract_deploy.rs`, and change no golden line.
    8. **Commit.**
       - Run the verify command.
       - Commit `fix(261103-ell): report an empty mode tag type folder on a pair as residue (F3)`, staging this task's files by explicit path.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --test deploy_plan pair_mode_tag_dirs_names_only_folders_with_no_read_file 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib pair_tags_refusal_names_mode_type_folders 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_apply empty_mode_type_folder 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=3)}' && cargo test -p ignition-core --test deploy_pair pair_verify_ 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=3)}' && cargo test -p ignition-core --lib 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --test deploy_plan --test deploy_apply --test deploy_pair --test deploy_verify --test deploy_pull 2>&1 | grep -E '^test result' && cargo test -p ignition-cli --test contract_deploy --test deploy_family_coverage 2>&1 | grep -E '^test result' && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && test -z "$(git diff -U0 9a2e268 -- crates/ignition-core/tests/deploy_plan.rs crates/ignition-core/src/deploy/apply/fingerprint.rs crates/ignition-cli/tests/contract_deploy.rs | grep -vE '^(\+\+\+|---) ' | grep -E '^[-+].*(GOLDEN|[0-9a-f]{64})')" && grep -q 'pub fn pair_mode_tag_dirs' crates/ignition-core/src/deploy/plan.rs && grep -q 'pair_mode_tag_dirs' crates/ignition-core/src/deploy/apply/gates.rs && grep -rqE 'fn pair_core_empty_mode_type_folder_refused\(' crates/ && grep -rqE 'fn pair_verify_warns_on_empty_mode_type_folder\(' crates/ && cargo clippy --workspace --all-targets -- -D warnings && git log --format=%s 9a2e268..HEAD | grep -qE '^fix\(261103-ell\): .*\(F3\)$' && echo T3B-OK</automated>
  </verify>
  <done>`T3B-OK` is printed.
- **Behaviour.** A pair with an empty, or unseen-content, mode tag type folder (and no residue unit) is warned at plan, refused at apply by both the gate and the engine guard with `deploy_pair_tags_refused`, and warned at verify.
- **Unchanged.** Standalone, `plan_sha256`, the plan JSON key set and every golden line are unchanged.
- **Commit.** One commit, staged by explicit path, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3c: F2 only on a `core` verdict, the gateway-tag-write README sentence, the live pair suite, SUMMARY and the full gate</name>
  <files>crates/ignition-core/src/deploy/verify.rs, crates/ignition-core/src/actions/deploy.rs, crates/ignition-core/tests/deploy_pair.rs, crates/ignition-core/tests/deploy_verify.rs, README.md, .planning/quick/261103-ell-review-fixes-f1-f5-option-a2/261103-ell-SUMMARY.md</files>
  <precondition>Task 3b's F3 commit is HEAD, and the scratch rig log `261103-ell-f2-rig.log` holds exactly one `F2 rig verdict:` line.</precondition>
  <read_first>
    - The scratch rig log `$SP/261103-ell-f2-rig.log` (its verdict line and evidence).
    - `crates/ignition-core/src/deploy/verify.rs`: the module table (lines 1-16), `pub struct Expected` (about 302), `pub struct ObservedState` (about 480-505) and `fn check_files` (about 1582-1635). Grep `ObservedState {` in `tests/deploy_verify.rs`.
    - `crates/ignition-core/src/actions/deploy.rs`: `deploy_verify_planned` and `pair_mode_tags_warning` (as changed by Task 3b), and the docs of `deploy_verify` and `deploy_verify_node`.
    - `crates/ignition-core/tests/deploy_pair.rs`: `pair_verify_warns_on_mode_tag_files` (about 502-549).
    - README: the paragraph opening `#### Tags and redundant pairs`. Its sentence starting "The gateway's own tag writes" is at about lines 2643-2644; also read the `**`deploy verify`**` bullet.
    - `crates/ignition-core/tests/live_deploy_pair.rs` lines 1-50 (the env table).
  </read_first>
  <behavior>
    F2, only on `F2 rig verdict: core`:
    - files_check_fails_a_declared_unit_with_a_mode_copy (lib, `verify.rs`): `check_files` with a declared tag unit whose status is `same` and whose mode copy holds two paths fails. It has one `files_differ` finding for that key, and its detail names the mode collection and both paths. Without a mode copy it passes.
    - pair_verify_fails_on_mode_copy_of_declared_unit (`deploy_pair.rs`):
      - Setup: the canary in core, byte-equal to the repo, plus a mode copy of the canary (the setup of today's `pair_verify_warns_on_mode_tag_files`).
      - Master verify: `ok == false`, the `files` check is `fail` with a `files_differ` finding for the canary key naming `CANARY_TAGS` and `CANARY_MARKER`, and the check ids are unchanged.
      - The pair warning no longer names those two paths.
      - `--node both` fails the master `files` check the same way and leaves the backup checks as they were.
    - `pair_verify_warns_on_mode_tag_files` is retargeted to an undeclared unit's mode copy (for example `ignition/tag-definition/default/Other/tags.json`). Verify's `ok` is unchanged and the one warning names that path.

    On any other verdict there is no F2 test or code, and `pair_verify_warns_on_mode_tag_files` stays as it is.
  </behavior>
  <action>
    This task builds F2 only if Task 3a's verdict is `core`, sets the README sentence to the verdict, runs the live pair suite, and closes out. Set `SP` (H1) and `V` from the verdict line in `$SP/261103-ell-f2-rig.log`.

    1. **F2 (only when `V` is `core`; RED, fix, commit).**
       - **RED.** Write `files_check_fails_a_declared_unit_with_a_mode_copy` and `pair_verify_fails_on_mode_copy_of_declared_unit`, and retarget `pair_verify_warns_on_mode_tag_files`. Run them and record the failures.
       - **`verify.rs`.**
         - `ObservedState` gains `mode_copies: BTreeMap<String, Vec<String>>`: a declared tag unit key mapped to its mode-collection file paths, filled only on a pair.
         - In `check_files`, a declared unit with a non-empty mode copy gets one `files_differ` finding. Its detail names the mode collection (`Expected`'s mode) and the paths, the first ten and the count, and says the copy overrides `core` and blocks the backup's sync. The finding takes the place of the status line for that unit.
         - Update the module table row 4 and the doc of `check_files`.
         - Fill the field in every `ObservedState` literal (`verify.rs` tests, `tests/deploy_verify.rs`).
       - **`actions/deploy.rs` `deploy_verify_planned`.**
         - On a pair, build `mode_copies` from the planned entries whose key is in `expected.tag_units`: their bare rows with a live sha.
         - `pair_mode_tags_warning` then names only the mode files of entries not in `expected.tag_units`, plus the F3 folders. Its guard still tests the combined list (B2).
         - Standalone passes an empty map, so a standalone verify is unchanged.
         - Update the doc comments on `deploy_verify` and `deploy_verify_node`.
       - **README `**`deploy verify`**` bullet.** On a pair, a mode-collection copy of a declared tag unit fails `files`, naming the paths, while other tag files and the folders stay a warning.
    2. **README, the gateway-tag-write sentence (W6).** This is the sentence starting "The gateway's own tag writes", at about README lines 2643-2644. It has exactly one commit location, and its marker phrase must sit on one line (the README is hard-wrapped):
       - **`core`:** rewrite it to say the gateway's own tag writes (Designer, `tags/import`) go to `core` even in a deployment mode, both a create and an edit of an existing core tag (debug C3; this rig check), so they are not affected. It must contain the phrase `both a create and an edit of an existing core tag`. It goes in the F2 commit.
       - **`mode`:** replace it to say the gateway creates a new tag folder in `core` (`tags/import`, debug C3), but an edit of an existing `core` tag lands in the mode collection and re-wedges a pair. It must contain the phrase `lands in the mode collection and re-wedges a pair`. It goes in a commit `docs(261103-ell): record where gateway edits of core tags land on a pair`.
       - **`inconclusive`:** replace it to say the gateway's own tag folder creates (`tags/import`) go to `core` even in a deployment mode, and where an edit of an existing core tag lands was not established. It must contain the phrase `where an edit of an existing core tag lands was not established`. It goes in the same `docs(261103-ell): ...` commit.
       - **`blocked`:** leave the sentence unchanged and record why in SUMMARY.
    3. **F2 commit** (`core` only): run the F2 tests, `--test deploy_pair --test deploy_verify --test deploy_apply --test deploy_plan` and workspace clippy, then commit `fix(261103-ell): fail verify on a mode-collection copy of a declared tag unit on a pair (F2)`. On `mode` or `inconclusive`, commit the README sentence alone as the docs commit of step 2.
    4. **Live pair suite (skip only on `blocked`).**
       - If `timeout 120 /bin/bash ./setup-pair.sh status` (from `crates/ignition-core/tests/fixtures/live-rig-pair`) reads settled, reuse the rig Task 3a left up. Otherwise bring up a fresh rig with Task 3a's step 2 procedure and watchdog rules. If that hangs, record `Live verdict: blocked`, tear down and go to step 5; the full gate then fails and the orchestrator decides.
       - With `tokens.env` sourced, run `IGNITION_LIVE_PAIR=1 IGN_PAIR_URL_A=http://localhost:18300 IGN_PAIR_URL_B=http://localhost:18301 IGN_PAIR_DATA_A=$HOME/.cache/ign-pair-rig/data-a IGN_PAIR_DATA_B=$HOME/.cache/ign-pair-rig/data-b PATH=$SP/sysbash:$PATH cargo test -p ignition-core --test live_deploy_pair -- --ignored --show-output --test-threads=1 2>&1 | tee $SP/261103-ell-live.log`.
       - Use `--show-output`, not `--nocapture`: under `--nocapture` the `test X ... ok` line is split and cannot be counted.
       - The eight gates must each show `LIVE-RAN <gate>` and `test <gate> ... ok`. This proves on a real 8.3.6 pair that F3 does not false-refuse and that F2, if built, does not false-fail.
       - Debug any failure with superpowers systematic-debugging. A fix is its own `fix(261103-ell): ...` commit.
       - Then tear down: `docker compose -p ign-pair-rig down -v`.
    5. **SUMMARY** (`261103-ell-SUMMARY.md` in the task directory; not committed). It contains:
       - **`## Findings`**, with exactly one line per finding:
         - F1-F5 in the form `F<n>: <outcome>`, where the outcome is `fixed`, `not reproduced`, `kept as warning` or `blocked`;
         - F7 exactly `F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests)`.

         Under each F1-F5 line: the RED test names and messages, and the commit sha. Under F7: its commit sha (the F1 commit).
       - **The F3 scope statement and its open item:** a type folder that holds in-scope files the migration deletes plus other content.
       - **`## F2 rig check`**: the commands (env named, never printed), the UTC times, the API counts, the before and after listings and hashes, and the exact line `F2 rig verdict: <word>`, matching the scratch log. On `mode` or `inconclusive`, the open item for the user (A2 and cef D2).
       - **`## Live`** (unless blocked): the command, the UTC times, every `test result:` line, and exactly one line `Live verdict: pass`.
       - The gate tokens `T1-OK`, `T2-OK`, `T3A-OK`, `T3B-OK`, `FULL-GATE-OK`.
       - Deviations.
       - The `conformance_ps_*` note: those rows run only on the Windows CI leg.
       - **Hygiene (W5).** No IP-shaped literal (beyond loopback and documentation ranges), no private-range prefix and no production host name. The gate also rejects any `<letters>vm<two digits>` VM-style host name; describe hosts by role ("the production pair") instead.
    6. **Full gate.** Run the verify command.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && SP=/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad && S=.planning/quick/261103-ell-review-fixes-f1-f5-option-a2/261103-ell-SUMMARY.md && RL=$SP/261103-ell-f2-rig.log && L=$SP/261103-ell-live.log && test "$(grep -cE '^F2 rig verdict: (core|mode|inconclusive|blocked)$' "$RL")" = 1 && V=$(sed -n 's/^F2 rig verdict: //p' "$RL") && grep -qx "F2 rig verdict: $V" "$S" && ( for f in F1 F2 F3 F4 F5; do test "$(grep -cE "^${f}: (fixed|not reproduced|kept as warning|blocked)" "$S")" = 1 || exit 1; done ) && grep -qxF 'F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests)' "$S" && ! grep -E '(^|[^0-9.])([0-9]{1,3}\.){3}[0-9]{1,3}([^0-9.]|$)' "$S" | grep -vE '127\.0\.0\.1|0\.0\.0\.0|192\.0\.2\.' | grep -q . && ! grep -qE '(^|[^0-9.])10\.[0-9]{1,3}\.' "$S" && ! grep -qiE '(^|[^a-z0-9])[a-z]{4,}vm[0-9]{2}' "$S" && cargo test -p ignition-core --test deploy_apply empty_mode_type_folder 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=3)}' && cargo test -p ignition-core --test deploy_pair pair_verify_ 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=3)}' && if [ "$V" = core ]; then grep -rqE 'fn pair_verify_fails_on_mode_copy_of_declared_unit\(' crates/ && cargo test -p ignition-core --test deploy_pair pair_verify_fails_on_mode_copy_of_declared_unit 2>&1 | grep -qE '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib files_check_fails_a_declared_unit_with_a_mode_copy 2>&1 | grep -qE '^test result: ok\. 1 passed'; else ! grep -rqE 'fn pair_verify_fails_on_mode_copy_of_declared_unit\(' crates/; fi && case "$V" in core) grep -qF 'both a create and an edit of an existing core tag' README.md && git log --format=%s 9a2e268..HEAD | grep -qE '^fix\(261103-ell\): .*\(F2\)$' ;; mode) grep -qF 'lands in the mode collection and re-wedges a pair' README.md && ! grep -qF 'even in a deployment mode, so they are not affected.' README.md && git log --format=%s 9a2e268..HEAD | grep -qE '^docs\(261103-ell\): ' ;; inconclusive) grep -qF 'where an edit of an existing core tag lands was not established' README.md && ! grep -qF 'even in a deployment mode, so they are not affected.' README.md && git log --format=%s 9a2e268..HEAD | grep -qE '^docs\(261103-ell\): ' ;; blocked) grep -qF 'even in a deployment mode, so they are not affected.' README.md ;; esac && if [ "$V" != blocked ]; then test "$(grep -cE '^Live verdict: pass$' "$S")" = 1 && test -s "$L" && ! grep -qE '^test result: .*[1-9][0-9]* failed' "$L" && ( for t in live_pair_core_tag_units_converge live_pair_core_migration_converges live_pair_refuses_a_tag_unit live_pair_converges live_pair_converges_with_a_stale_signature live_pair_unsynced_then_reconverge live_pair_secret_names live_pair_tag_cleanup_converges; do grep -qx "LIVE-RAN $t" "$L" && grep -qE "^test (.*::)?$t \.\.\. ok$" "$L" || exit 1; done ); fi && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && test -z "$(git diff -U0 9a2e268 -- crates/ignition-core/tests/deploy_plan.rs crates/ignition-core/src/deploy/apply/fingerprint.rs crates/ignition-cli/tests/contract_deploy.rs | grep -vE '^(\+\+\+|---) ' | grep -E '^[-+].*(GOLDEN|[0-9a-f]{64})')" && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | tee "$SP/261103-ell-workspace.log" | grep -E '^test result' | tail -3 && ! grep -qE '^test result: FAILED' "$SP/261103-ell-workspace.log" && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && git diff --quiet 9a2e268 -- Cargo.toml Cargo.lock crates/ignition-cli/Cargo.toml crates/ignition-core/Cargo.toml crates/ignition-tui/Cargo.toml && ! git log --name-only --format= 9a2e268..HEAD | grep -qE '^\.planning/config\.json$|^\.planning/research/\.cache/|-uat4\.ps1$|^\.planning/debug/tg-pg-table-check\.md$' && ! git diff 9a2e268..HEAD | grep -E '^\+' | grep -vE '127\.0\.0\.1|0\.0\.0\.0|192\.0\.2\.' | grep -qE '(^|[^0-9.])([0-9]{1,3}\.){3}[0-9]{1,3}([^0-9.]|$)' && ! git log --format=%B 9a2e268..HEAD | grep -qi 'co-authored-by' && ! git diff 9a2e268..HEAD -- crates | grep -E '^\+' | grep -qF '#[ignore' && test -z "$(git diff --cached --name-only)" && test -z "$(docker ps -q --filter label=com.docker.compose.project=ign-pair-rig 2>/dev/null)" && echo FULL-GATE-OK</automated>
  </verify>
  <done>`FULL-GATE-OK` is printed.
- **F2.** It is built exactly when the verdict is `core`. Otherwise it is recorded as kept as a warning (or blocked), with evidence, and no F2 code exists.
- **README.** The gateway-tag-write sentence matches the verdict and sits in its one commit location.
- **Live.** Unless blocked, the eight live pair gates pass on the 8.3.6 rig with `--show-output`, and the rig is torn down.
- **SUMMARY.** It has one line per finding, with F7 in its exempt form, and passes the hygiene greps.
- **Full gate.** fmt, workspace clippy, `cargo test --workspace` and windows-msvc xwin clippy are green. No golden line changed. There is no Cargo change, no protected file, no IP literal, no co-author line, no new `#[ignore]` and nothing staged.
- **Commits.** One commit per fixed finding, plus any docs or live-fix commit, on `feat/deploy-tags-core`. Nothing is pushed.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| on-box snapshot -> rollback engine | recovery trusts snapshot copies only when their hashes match the record or the journal fingerprint |
| host script / local host -> gateway data dir (mode and core roots) | restore and probe decide what is written where, and whether to proceed |
| live mode collection -> plan, gate, verify | directories and files the gateway's redundancy sync counts decide whether a deploy is refused or reported |
| live tests and rig check -> pair rig | tokens and bind-mounted data dirs during the Docker run |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-ell-01 | Denial of service | `rollback.rs` `snapshot_record`, a torn pair run unrecoverable (F1) | high | mitigate | `snapshot_key` delegates to `snapshot_slot`, so `.core/` pre-images are found under `core/`. The hash checks against the record and journal fingerprint are unchanged, so a damaged snapshot is still `rollback_failed`. Pinned by `pair_core_torn_run_recovers_core_keys` (with and without the record) |
| T-ell-02 | Tampering | sftp prepare staging slot diverging from the scripts' slot (F7) | medium | mitigate | One slot rule (`snapshot_slot`) for staging, snapshot and recovery, confirmed by the Task 1 positive grep, diff review and clippy's unused-import check. The existing sftp staging tests pin the staged slots |
| T-ell-03 | Denial of service | restore stranding deleted mode copies when `core` is gone (F4) | medium | mitigate | A created-only `.core/` record is `Removed` without `core`. A `.core/` pre-image still requires `core`, and a symlinked core root is refused for every op, so nothing is written through a link or into a recreated `core`. Row `core_restore_created_only_without_core_root` on all three hosts |
| T-ell-04 | Tampering / Elevation | a symlinked core tag folder redirecting writes outside `core`, or a mid-commit failure (F5) | high | mitigate | The probe stops at the first existing level and refuses a symlink or non-directory (`core_tags_unsafe`) or an unopenable or unsearchable directory (`core_tags_unwritable`) before any write. The commit's own symlink checks stay. Row `core_probe_refuses_unsafe_type_folder` on all three hosts |
| T-ell-05 | Denial of service (pair wedge) | an empty mode tag type folder passing plan, apply and verify (F3) | high | mitigate | `pair_mode_tag_dirs` from `list_types` feeds the one `pair_tags_refusal` used by the gate and the engine guard, plus both pair warnings. Every empty guard tests the combined list, so folder-only residue is refused (B2). The limit (content beside in-scope deleted files) is recorded, and the post-apply verify warns on it |
| T-ell-06 | Repudiation (false green) | verify passing while a mode copy overrides `core` (F2) | medium | mitigate | Gated by the rig verdict. On `core`, a `files_differ` finding names the mode paths. On any other verdict the warning stays, and an open item records the A2 / D2 implication |
| T-ell-07 | Information disclosure | tokens during the rig check and the live suite | medium | mitigate | Tokens come only from the sourced `tokens.env` into a request header, never echoed or logged. There is no `curl -v` and no `set -x`. Logs live in the scratchpad, never in the repo. The Task 3a gate rejects a token-shaped header in the rig log, and the full gate rejects IP literals, private-range prefixes and VM-style host names in the SUMMARY |
| T-ell-08 | Elevation / Tampering | an accidental production call | critical | mitigate | Loopback URLs only, and the live tests keep their URL and name guards. The full gate greps the diff for IP-shaped literals |
| T-ell-09 | Denial of service | killing a process the executor did not start (the leftover blocked `setup-pair.sh pair` tree) | low | mitigate | Only a `timeout` watchdog the executor starts may stop a step. `pkill` and `kill` of foreign processes are forbidden (H1) |
| T-ell-SC | Tampering | package installs | low | accept | No new crate. The full gate asserts no Cargo.toml or Cargo.lock change since 9a2e268 |
</threat_model>

<verification>
- **Task 1:** the two torn pair run recoveries pass, and both `rollback.rs` and sftp prepare use `snapshot_slot` (T1-OK).
- **Task 2:** the F4 and F5 conformance rows pass on the local and sh hosts and compile for ps, `commit.ps1` parses, and there are two commits (T2-OK).
- **Task 3a:** the rig verdict is recorded exactly once, no probe folder is left, and no repo file changed (T3A-OK).
- **Task 3b:** the F3 tests pass, including folder-only residue at the gate, the engine guard and both warnings. No golden line changed, and there is one F3 commit (T3B-OK).
- **Task 3c:** F2 follows the verdict, and the README sentence on gateway tag writes matches it in its one commit. Unless blocked, the eight live pair gates pass with `--show-output`. SUMMARY has the per-finding lines (F7 in its exempt form) and passes the hygiene greps. The full gate is green: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` (FULL-GATE-OK).
- **Scope:** no worktree, version bump, slug change, push, PR or production call. The H1 files are never staged, and the PLAN and SUMMARY stay uncommitted for the orchestrator's docs commit.
</verification>

<success_criteria>
- A torn pair run that replaced or deleted core tag files is recoverable with `ign deploy rollback --run`, with or without its record.
- A rollback never strands a pair's mode copies because `core` is missing, unless a core pre-image genuinely needs it.
- No host passes preflight over a symlinked, non-directory, unopenable or unsearchable core tag folder.
- A pair whose mode collection holds an empty (or unseen-content) tag type folder is warned at plan, refused at apply and warned at verify.
- Verify fails on a mode copy of a declared tag unit on a pair if, and only if, the rig showed that gateway edits of core tags land in `core`. README says where gateway tag edits land, matching the rig.
- Every finding has a SUMMARY line. F1-F5 carry their RED evidence and commit; F7 reads exactly `F7: fixed, refactor, exempt from RED (orchestrator decision; behaviour already correct, covered by existing sftp staging tests)`. The gates are green.
</success_criteria>

<output>
Create `.planning/quick/261103-ell-review-fixes-f1-f5-option-a2/261103-ell-SUMMARY.md` (Task 3c, step 5). Do not commit it; the orchestrator's docs commit carries the PLAN and SUMMARY.
</output>
