---
phase: quick-261102-kus
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-core/src/deploy/plan.rs
  - crates/ignition-core/tests/deploy_plan.rs
  - README.md
autonomous: true
requirements: [QUICK-kus]

estimate:
  tokens: 60000
  raw_tokens: 60000
  tasks: 2
  confidence: low

must_haves:
  truths:
    - "On a redundant pair, a migration planned with `--base` whose live mode copy equals the base marks every migrated tag unit `drift: false` (`summary.drifted` 0), and `evaluate_gates` returns Ready with `--allow-destructive` alone, with no `--allow-drift` (closes 261102-cef VERIFICATION warning W1, the W4 follow-up; keeps cef D2 drift authoritative)"
    - "A migration whose live mode copy differs from the base is still `drift: true` and refused without `--allow-drift` (`destructive_change_refused`, exit 2). This covers an edited mode file and a base file missing from the mode copy (cef D2)"
    - "A routed unit with any `core` copy is judged on its own core shas, so an unledgered or partial core copy that differs from the base is still `drift: true`. An ambiguous removal stays `shadows_removal` and `deploy_blocked` (exit 6) with `--base` and both allow flags (cef D2)"
    - "Standalone plans and every non-migration drift verdict are unchanged: `core_route_drift_on_core_copy` and every existing deploy_plan test pass, `apply_base` leaves `plan_sha256` untouched, and the five goldens (`PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE`, `RENDER_GOLDEN`, fingerprint `GOLDEN`) are byte-identical"
    - "README's migration bullet under Tags and redundant pairs says how `--base` judges a migration"
  artifacts:
    - path: "crates/ignition-core/src/deploy/plan.rs"
      provides: "`apply_base` migration rule: a `.core/` row with no live sha, in an entry with no core copy, is judged by the mode sha at the same path when the plan deletes that mode file"
      contains: "pub fn apply_base"
    - path: "crates/ignition-core/tests/deploy_plan.rs"
      provides: "core_route_base_migration_clean_no_drift, core_route_base_migration_mode_edit_drifts, core_route_base_migration_core_copy_drifts"
      contains: "core_route_base_migration_clean_no_drift"
    - path: "README.md"
      provides: "one sentence in the migration bullet on `--base`"
      contains: "judged by the mode copy the apply deletes"
  key_links:
    - from: "crates/ignition-core/src/actions/deploy.rs deploy_plan"
      to: "crates/ignition-core/src/deploy/plan.rs apply_base"
      via: "`apply_base(&mut planned, base)` runs after `plan_with(.., Redundancy::of(..))`, so `planned.delete_set` already holds the migration's bare mode deletes"
      pattern: "apply_base\\(&mut planned"
    - from: "crates/ignition-core/src/deploy/plan.rs apply_base"
      to: "Plan.delete_set.files"
      via: "the bare delete paths decide whether a `.core/` row takes the mode sha as its live side"
      pattern: "delete_set"
    - from: "crates/ignition-core/src/deploy/apply/gates.rs evaluate_gates"
      to: "PlanEntry.drift"
      via: "the drift gate refuses `drift == Some(true)` without `--allow-drift` (unchanged)"
      pattern: "e.drift == Some\\(true\\)"
---

<objective>
Stop a pair migration planned with `--base` from reporting false drift, without losing real drift.

Today, on a redundant pair, each migrated tag unit gets `.core/` rows whose live sha is null, because `core` has no copy yet. `apply_base` looks the base up at the bare path, finds the old mode-path sha, sees `None != Some(base)`, and marks the unit `drift: true`. The apply then needs `--allow-drift` on top of `--allow-destructive`. The fix: for a migrated unit (no `core` copy at all, and the plan deletes the mode file at the same path), judge the `.core/` row by the mode copy's sha. The base was rendered from that copy, and the gateway runs it now. Bare rows keep their current comparison. A core copy of any shape keeps its own shas.

Purpose: closes the 261102-cef VERIFICATION follow-up (warning W1, tracked as W4). A clean migration then needs `--allow-destructive` and nothing more. D2 from 261102-cef still holds, so drift stays authoritative: an edited mode copy, a base file missing from the mode copy, an unledgered or partial core copy, and an ambiguous removal (`shadows_removal`, `deploy_blocked`) are all still reported.

Output: the `apply_base` change and its doc in plan.rs, three offline tests in deploy_plan.rs, and one README sentence.
</objective>

## Source coverage audit

| Source | Item | Covered by |
|--------|------|------------|
| GOAL | `apply_base` uses the mode-path base entry when the core copy is absent and the plan deletes the mode copy | Task 1 |
| GOAL | test (a): migration with `--base`, mode copy equal to base, no drift | Task 1 (`core_route_base_migration_clean_no_drift`) |
| GOAL | test (b): migration with `--base`, live mode copy edited, drift | Task 2 (`core_route_base_migration_mode_edit_drifts`) |
| GOAL | test (c): core copy present and different from base, drift still reported | Task 2 (`core_route_base_migration_core_copy_drifts`) |
| GOAL | real drift kept: mode copy differs from base; unledgered core copy (`shadows_removal` / `deploy_blocked`) | Task 2 (tests b and c, mutants M1 to M3) |
| GOAL | README sentence only if existing text needs it | Task 2 (needed; rationale in the action) |
| REQ | QUICK-kus | Tasks 1 and 2 |
| CONTEXT | cef D2: Designer edits to ign-managed core tag folders are drift | Task 2 (test c), Task 1 (rule keeps core rows on their own shas) |
| RESEARCH | none (no research phase) | n/a |

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-VERIFICATION.md
@.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-SUMMARY.md
@.planning/quick/261102-cef-deploy-tag-units-to-core-option-a/261102-cef-CONTEXT.md

Source ranges the executor needs. Read only these ranges, not whole files:
- crates/ignition-core/src/deploy/plan.rs lines 56-68 (module doc, "Redundant pairs"), 560-590 (`routed_rows`), 870-1090 (`plan_with`), 1443-1465 (`apply_base`)
- crates/ignition-core/tests/deploy_plan.rs lines 241-340 (shared helpers `rendered`, `observed`, `ledger_of`, `deleted`, `entry`) and 2778-3240 (the `core_route` module; `core_route_drift_on_core_copy` at 3105 shows the inline `BaseTree` construction and the gate assertions)
- README.md lines 2669-2697 (the migration bullet, "What is still refused", "Designer edits are drift") and 3330-3345 (Drift section)

Interfaces (as they are at HEAD 8d7bf60):
- `pub fn apply_base(plan: &mut Plan, base: &BaseTree)`. It sets `entry.drift` and `plan.summary.drifted`, and never touches `plan_sha256`.
- `Plan { entries: Vec<PlanEntry>, summary, delete_set: DeleteSet, plan_sha256, .. }`. `DeleteSet { files: Vec<RelPath> }` holds mode deletes as bare paths and core deletes as `.core/<path>`.
- `PlanEntry { live_in: Vec<String> /* ordered subset of "core","mode" */, files: Vec<PlanFile>, drift: Option<bool>, .. }`
- `PlanFile { path: String, live_sha256: Option<String>, desired_sha256: Option<String> }`
- `layout::split_key(&str) -> (KeyCollection, &str)` with `KeyCollection::{Mode, Core}`. Both are already imported in plan.rs.
- A routed unit's rows (`routed_rows`): a `.core/<p>` row per path of the render and the core copy (live = core sha), then a bare `<p>` row per mode file (live = mode sha, desired = null).
- `BaseTree { dir: String, render_sha256: String, files: BTreeMap<RelPath, String> }`
- `GateFlags { plan_sha256: Option<String>, allow_destructive: bool, allow_drift: Option<String> }`, and `evaluate_gates(&DeployPlanResult, &GateFlags, Redundancy)`.
- core_route test helpers: `unit(tags)`, `strs`, `pair(tree, live, ledger)`, `with(redundancy, ..)`, `sha`, `rows`, `as_result(tree, plan)` (consumes the plan), `allow_destructive()`, and the constants `KEY`, `TAGS`, `MARK`, `C_TAGS`, `C_MARK`, `MARKER`. `observed(core, mode)` takes core first.

Constraints that bind every task:
- Branch feat/deploy-tags-core, base HEAD 8d7bf60. Work on the main checkout only, with no worktree. Make no production calls and use no Docker.
- Never stage .planning/config.json, .planning/research/.cache/, svc-ignition-deploy@52.154.75ace-uat4.ps1 or .planning/debug/tg-pg-table-check.md. Stage by explicit path only.
- Commit messages carry no co-author trailer, no IP address and no host name. Do not bump any version.
- Do not push or open a PR. Do not commit SUMMARY.md, STATE.md or PLAN.md; the quick orchestrator commits the docs.
- Scratchpad directory for logs: /private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1 (tracer): end to end, a clean pair migration with --base plans drift-free and passes the gates with --allow-destructive alone</name>
  <files>crates/ignition-core/tests/deploy_plan.rs, crates/ignition-core/src/deploy/plan.rs</files>
  <behavior>
    - `core_route_base_migration_clean_no_drift` covers two cases. In both, the live mode copy is `unit("[1]")`, the ledger lists `[TAGS, MARK]` bare, there is no core copy, and the base is the render of `unit("[1]")`.
      - Case 1: the render is `unit("[1]")`.
      - Case 2: the render is `unit("[2]")`, a migration that also ships a change.
    - For each case, `pair(..)` then `apply_base(..)` gives:
      - `entry(KEY).drift == Some(false)` and `summary.drifted == Some(0)`
      - `deleted(&p) == [TAGS, MARK]`
      - the same `plan_sha256` before and after `apply_base`
      - `evaluate_gates(.., &allow_destructive(), Redundancy::Pair)` returns `GateOutcome::Ready`
    - RED: at 8d7bf60 the test fails on the drift assertion, because every `.core/` row reads `None` against the base sha.
  </behavior>
  <action>
RED first, per the task's TDD rule. In the `core_route` module of crates/ignition-core/tests/deploy_plan.rs, add the module-private helper `base_of(tree: &RenderedTree) -> BaseTree`. Build it the same way `core_route_drift_on_core_copy` builds its inline base: `dir` "base", `render_sha256` from `tree.render_sha256()`, and `files` mapping each path to `sha256_hex` of its bytes. Do not edit that existing test.

Then add `core_route_base_migration_clean_no_drift` as `<behavior>` specifies, looping over the two render cases. Put each new item after `core_route_tag_type_definition_unit`, before the module's closing brace, and add only lines; no existing line of the file may change. Give the test a short doc comment: a pair migration planned with `--base`, mode copy equal to the base, is not drift (quick 261102-kus).

Before touching plan.rs, run `cargo test -p ignition-core --test deploy_plan core_route_base_migration_clean_no_drift 2>&1 | tee <scratchpad>/261102-kus-red.log`. The log must show the test compiled, ran and failed on the drift assertion. A compile error is not RED, so fix the test and rerun.

GREEN, in `apply_base` (crates/ignition-core/src/deploy/plan.rs), implementing the task description's rule:
- Before the entry loop, collect the bare (mode-collection) paths of `plan.delete_set.files` into a `BTreeSet<&str>`. Borrow `plan.delete_set` and `plan.entries` as disjoint fields.
- For each entry, compute two values. `core_absent` is true when `entry.live_in` has no `"core"`, meaning the unit has no core copy at all. The mode-sha map maps each bare row's path (`split_key(..).0 == KeyCollection::Mode`) to its `live_sha256`.
- In the per-file test, keep the base lookup exactly as it is (`RelPath::new(split_key(&file.path).1)` into `base.files`), then choose the live side:
  - Take the bare row's live sha at the same collection path only when all of these hold: the row is a `.core/` row, its `live_sha256` is `None`, `core_absent` is true, its collection path is in the bare delete set, and the entry has a bare row at that path. That bare sha is the mode copy the base was rendered from and the gateway runs now.
  - Every other row keeps its own `live_sha256`.
- A row drifts when its chosen live side differs from both the base sha and `desired_sha256`. Compare as `Option<&String>`.
- Bare rows are not changed. They still compare the mode sha with the base against a null desired, so any mode copy that differs from the base is drift, even one that equals the render. That is the conservative reading of "a live mode copy that differs from the base" in the task.
- A `.core/` row whose bare path is missing from the mode copy keeps `None`. A base file deleted from the mode copy outside the pipeline therefore stays drift, as the README Drift rule says.
- Do not change `plan_with`, `routed_rows`, `delete_set`, `pair_tag_keys`, `plan_sha256` or anything under deploy/apply or actions.
- A standalone plan never has a `.core/` row with a null live sha. Its only `.core/` rows are core-delete pre-images, and those carry a live sha. Standalone drift is therefore unchanged by construction.

Docs in plan.rs:
- Extend the `apply_base` doc comment with one paragraph on the migration rule, citing quick 261102-kus and cef D2. Say that a core copy of any shape keeps every `.core/` row on its own sha, and that bare rows are always judged by the mode sha.
- Add one sentence to the module doc's "Redundant pairs" paragraph, citing quick 261102-kus: under `--base` a migrated unit is judged by its mode copy. (The T1 gate requires two added plan.rs lines citing 261102-kus.)

Run `cargo fmt --all`. Commit plan.rs and deploy_plan.rs by explicit path with the message `fix(261102-kus): judge a pair migration's core rows by its mode copy under --base`, and no co-author trailer.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && SP=/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad && test -s "$SP/261102-kus-red.log" && grep -qE '^test core_route::core_route_base_migration_clean_no_drift \.\.\. FAILED$' "$SP/261102-kus-red.log" && grep -qE 'fn core_route_base_migration_clean_no_drift\(' crates/ignition-core/tests/deploy_plan.rs && grep -qE 'fn base_of\(' crates/ignition-core/tests/deploy_plan.rs && cargo test -p ignition-core --test deploy_plan core_route_base_migration_clean_no_drift 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_plan core_route 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=14)}' && cargo test -p ignition-core --test deploy_plan core_route_drift_on_core_copy 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --test deploy_plan 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --test deploy_apply pair_core_ 2>&1 | grep -E '^test result: ok\.' && cargo test -p ignition-core --lib 2>&1 | grep -E '^test result: ok\.' && cargo fmt --all --check && cargo clippy -p ignition-core --all-targets -- -D warnings && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && git diff --quiet 8d7bf60 -- crates/ignition-core/src/deploy/apply crates/ignition-core/src/actions crates/ignition-core/src/deploy/layout.rs && test "$(git diff --minimal --numstat 8d7bf60 -- crates/ignition-core/tests/deploy_plan.rs | cut -f2)" = 0 && test "$(git diff 8d7bf60 -- crates/ignition-core/src/deploy/plan.rs | grep -c '^+.*261102-kus')" -ge 2 && echo T1-OK</automated>
  </verify>
  <done>`T1-OK` is printed.
- **RED.** <scratchpad>/261102-kus-red.log shows `core_route_base_migration_clean_no_drift` failing at the pre-fix `apply_base`.
- **GREEN.** That test passes, and 14 or more `core_route` tests pass, `core_route_drift_on_core_copy` among them. The whole deploy_plan binary, the `pair_core_` engine tests and the ignition-core lib tests pass. fmt and ignition-core clippy are clean.
- **Scope.** The five golden hashes are present and unchanged. Nothing under deploy/apply, actions or layout.rs changed. deploy_plan.rs only gained lines.
- **Commit.** One commit on feat/deploy-tags-core, staged by explicit path, with no co-author trailer.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: pin real drift on pair migrations (mode edit, missing mode file, core copy, ambiguous removal), prove the guards with mutants, README sentence, full gate</name>
  <files>crates/ignition-core/tests/deploy_plan.rs, README.md</files>
  <precondition>Task 1's commit is HEAD, and `cargo test -p ignition-core --test deploy_plan core_route_base_migration_clean_no_drift` passes.</precondition>
  <behavior>
    - `core_route_base_migration_mode_edit_drifts`. Ledger `[TAGS, MARK]` bare, no core copy, base `base_of(&rendered(&strs(&unit("[1]"))))`, render `unit("[1]")`.
      - (b1) Mode copy `unit("[edited]")`. Expect `drift == Some(true)`, `summary.drifted == Some(1)` and `deleted == [TAGS, MARK]`. `evaluate_gates` with `allow_destructive()` returns `destructive_change_refused`, exit 2. With `allow_destructive: true` plus `allow_drift: Some(reason)` it returns Ready.
      - (b2) The mode copy holds only `MARK`, because tags.json was deleted outside the pipeline. Expect `deleted == [MARK]` and `drift == Some(true)`.
    - `core_route_base_migration_core_copy_drifts`. Mode `unit("[1]")` ledgered `[TAGS, MARK]` bare, base the render of `unit("[1]")`.
      - (c1) Render `unit("[1]")`, plus an unledgered core copy `unit("[designer]")`. Expect status `Changed`, core `Differs`, `deleted == [TAGS, MARK]` and `drift == Some(true)`. Gates with `allow_destructive()` return `destructive_change_refused`, exit 2. With both flags they return Ready.
      - (c2) Render `unit("[1]")`. The core copy is only `MARK` with `MARKER` bytes, equal to the base: a partial copy still counts as present. Expect `drift == Some(true)`.
      - (c3) Render empty (`rendered(&[])`), core `unit("[designer]")`, an ambiguous removal. Expect core `ShadowsRemoval` and `drift == Some(true)`. Gates with `allow_destructive: true` and `allow_drift: Some(reason)` still return `deploy_blocked`, exit 6.
    - Both tests pass at Task 1's HEAD. They also pass at 8d7bf60, because they guard verdicts the fix must keep. Their RED proof is three mutants of `apply_base`, each of which makes a guard fail:
      - M1: an entry with no core copy whose bare rows are all in the delete set is never drift. `core_route_base_migration_mode_edit_drifts` (b1) fails.
      - M2: the `core_absent` condition is dropped, so the mode sha replaces any `.core/` row whose bare path is deleted, whatever the row's own sha. `core_route_base_migration_core_copy_drifts` fails.
      - M3: a skip replaces the substitution, so every `.core/` row with a null live sha is ignored. `core_route_base_migration_mode_edit_drifts` (b2) fails.
  </behavior>
  <action>
Add the two tests from `<behavior>` to the `core_route` module of crates/ignition-core/tests/deploy_plan.rs. Reuse `base_of`, `unit`, `strs`, `pair`, `entry`, `deleted`, `as_result` and `allow_destructive`. If both flags are needed more than once, add a small helper `allow_destructive_and_drift()` that sets `allow_destructive: true` and an `allow_drift` reason. Add lines only. Give each test a one-line doc comment that names the real drift it pins and cites quick 261102-kus and cef D2. Capture `entry(..)` fields and `summary` before `as_result` consumes the plan.

Mutant proof (RED for the guards). For each of M1, M2 and M3 in `<behavior>`, in turn:
1. Make the mutation in `apply_base` with Edit.
2. Run `cargo test -p ignition-core --test deploy_plan core_route_base_migration 2>&1 | tee <scratchpad>/261102-kus-mN.log`, where N is 1, 2 or 3. The run must report the named test as failed.
3. Revert the mutation with Edit, not git checkout or stash, and confirm that `git diff --quiet HEAD -- crates/ignition-core/src/deploy/plan.rs` holds.

If a mutant does not make its test fail, the test is too weak. Strengthen the test, not the mutant, and rerun. plan.rs must end this task byte-identical to Task 1's commit.

README. One sentence is needed. Before Task 1, a migration with `--base` also needed `--allow-drift`, and the migration bullet never said so. The Drift section's rule ("in the base and the desired tree, not live, so `added`, is drift") would still predict drift for the `.core/` rows of a moved unit. In README.md's "Moving units out of the mode collection (the migration)" bullet, directly after the sentence ending "the apply needs `--allow-destructive`.", insert this sentence verbatim and wrap it at the bullet's existing width:

With `--base`, a moved unit's `core` rows are judged by the mode copy the apply deletes, so the migration is `drift: true` only when that mode copy differs from the base or a `core` copy already exists that differs from it; a clean migration needs no `--allow-drift`.

Change no other README line, and leave the exit table alone.

Run `cargo fmt --all`, then the full gate in `<verify>`. Commit deploy_plan.rs and README.md by explicit path with the message `test(261102-kus): pin real drift on pair migrations under --base; README note`, and no co-author trailer. Then write the SUMMARY described in `<output>`, quoting the RED and mutant log lines and the gate token.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && SP=/private/tmp/claude-501/-Users-pmannion-whiskeyhouse-ignition-cli/32a1256c-0a50-4687-89f8-cc7664a6d90f/scratchpad && grep -qE '^test core_route::core_route_base_migration_mode_edit_drifts \.\.\. FAILED$' "$SP/261102-kus-m1.log" && grep -qE '^test core_route::core_route_base_migration_core_copy_drifts \.\.\. FAILED$' "$SP/261102-kus-m2.log" && grep -qE '^test core_route::core_route_base_migration_mode_edit_drifts \.\.\. FAILED$' "$SP/261102-kus-m3.log" && git diff --quiet HEAD -- crates/ignition-core/src/deploy/plan.rs && test "$(git log --format=%h 8d7bf60..HEAD -- crates/ignition-core/src/deploy/plan.rs | wc -l | tr -d ' ')" = 1 && grep -qE 'fn core_route_base_migration_mode_edit_drifts\(' crates/ignition-core/tests/deploy_plan.rs && grep -qE 'fn core_route_base_migration_core_copy_drifts\(' crates/ignition-core/tests/deploy_plan.rs && cargo test -p ignition-core --test deploy_plan core_route_base_migration 2>&1 | grep -E '^test result: ok\. 3 passed' && cargo test -p ignition-core --test deploy_plan core_route 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=16)}' && grep -qF 'judged by the mode copy the apply deletes' README.md && test -z "$(git diff 8d7bf60 -- crates/ignition-core/tests/deploy_plan.rs | grep -E '^-' | grep -vE '^---|^-[[:space:]]*\}?[[:space:]]*$')" && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ca50ebbf57b1933c2022ffd4b9ebb8ff8b0d8b10cedbcf98e1ea34f9e4b63e79' crates/ignition-core/src/deploy/apply/fingerprint.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | tee "$SP/261102-kus-workspace.log" | grep -E '^test result' | tail -3 && ! grep -qE '^test result: FAILED' "$SP/261102-kus-workspace.log" && ! grep -qE '^test result: FAILED' "$SP/261102-kus-workspace.log" && test "$(grep -c '^test result: ok' "$SP/261102-kus-workspace.log")" -ge 96 && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && git diff --quiet 8d7bf60 -- Cargo.toml Cargo.lock crates/ignition-cli/Cargo.toml crates/ignition-core/Cargo.toml crates/ignition-tui/Cargo.toml && ! git log --name-only --format= 8d7bf60..HEAD | grep -qE '^\.planning/config\.json$|^\.planning/research/\.cache/|-uat4\.ps1$|^\.planning/debug/tg-pg-table-check\.md$' && ! git diff --cached --name-only | grep -qE '^\.planning/config\.json$|^\.planning/research/\.cache/|-uat4\.ps1$|^\.planning/debug/tg-pg-table-check\.md$' && ! git log --format=%B 8d7bf60..HEAD | grep -qi 'co-authored-by' && ! git diff 8d7bf60 -- crates README.md | grep -E '^\+' | grep -vE '127\.0\.0\.1|0\.0\.0\.0|192\.0\.2\.' | grep -qE '(^|[^0-9.])([0-9]{1,3}\.){3}[0-9]{1,3}([^0-9.]|$)' && test "$(git worktree list | wc -l | tr -d ' ')" = 1 && echo FULL-GATE-OK</automated>
  </verify>
  <done>`FULL-GATE-OK` is printed.
- **Guards.** `core_route_base_migration_mode_edit_drifts` and `core_route_base_migration_core_copy_drifts` pass, and all three `core_route_base_migration*` tests pass together. 16 or more `core_route` tests pass.
- **Mutants.** M1, M2 and M3 each made their named test fail, and the logs are in the scratchpad. plan.rs equals Task 1's commit, and exactly one commit since 8d7bf60 touches it.
- **README.** The migration bullet carries the `--base` sentence, and no other README line changed.
- **Full gate.** fmt, workspace clippy, `cargo test --workspace` (no failed result) and the windows-msvc `cargo xwin clippy` are green. The five goldens are unchanged.
- **Hygiene.** Cargo manifests and the lock file are unchanged. No protected file is committed or staged. There is no co-author trailer, no IP-shaped literal in added lines and only the main worktree.
- **Commit.** One commit for this task. Nothing is pushed, and SUMMARY is written but not committed.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| live gateway files → plan | Mode and core files on the master can be changed outside the pipeline (Designer, tag import, hand edits). `--base` drift is the operator's signal for that. |
| plan → apply gates | `evaluate_gates` trusts `entry.drift` to decide whether `--allow-drift` is required. |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-kus-01 | Tampering | `apply_base` migration rule (plan.rs) | medium | mitigate | The substitution fires only for a `.core/` row with a null live sha, in an entry with no core copy, whose bare path is in this plan's delete set. Bare rows keep mode-vs-base. Tests (b1), (b2), (c1) and (c2) pin that an edited mode file, a missing mode file and an unledgered or partial core copy stay drift. Mutants M1 to M3 prove each guard can fail. |
| T-kus-02 | Elevation of Privilege | drift gate bypass (gates.rs, unchanged) | medium | mitigate | Gates are not edited (Task 1's verify diff-checks deploy/apply and actions). A migration still needs `--allow-destructive` and the plan hash. Test (c3) pins that `deploy_blocked` (exit 6) still fires on an ambiguous removal with `--base` and both allow flags. |
| T-kus-03 | Repudiation | `--allow-drift` reason in the run record | low | accept | A clean migration no longer needs a drift reason because nothing drifted. `--allow-destructive` and the reviewed plan hash still gate and record the apply. |
| T-kus-04 | Information Disclosure | test fixtures, README | low | accept | The changes hold paths and sha256 values only. The full gate refuses IP-shaped literals in added lines, and no host names are used. |
</threat_model>

<verification>
- `T1-OK`, then `FULL-GATE-OK`.
- RED evidence: <scratchpad>/261102-kus-red.log (pre-fix failure of the tracer test), plus 261102-kus-m1.log, 261102-kus-m2.log and 261102-kus-m3.log (each mutant fails its guard).
- Goldens: `PLAN_GOLDEN` a89de504..., `PLAN_GOLDEN_LEDGER` 5084afcc..., `PLAN_GOLDEN_UNREADABLE` ef3832a4..., `RENDER_GOLDEN` 7c8b41b1... and fingerprint `GOLDEN` ca50ebbf... are each present and unchanged.
- Standalone and non-migration drift are unchanged: `core_route_drift_on_core_copy` and the whole deploy_plan binary pass, and deploy/apply and actions show no diff.
</verification>

<success_criteria>
- On a redundant pair, a migration planned with `--base` whose mode copy equals the base reports `drift: false` and applies with `--allow-destructive` alone.
- An edited mode copy, a base file missing from the mode copy, an unledgered or partial core copy, and an ambiguous removal are still drift (and `deploy_blocked` for the removal).
- No golden, gate, slug, exit code, manifest or version changes. The full gate is green, including windows-msvc clippy.
- Two commits on feat/deploy-tags-core, staged by explicit path, with no co-author trailer and no push.
</success_criteria>

<output>
Create `.planning/quick/261102-kus-base-drift-on-pair-tag-migration/261102-kus-SUMMARY.md` when done. It must give:
- the rule as built
- the RED line from 261102-kus-red.log and the failing line from each mutant log
- the gate tokens (`T1-OK`, `FULL-GATE-OK`)
- the two commit hashes
- the README sentence

Do not commit it; the quick orchestrator commits PLAN, SUMMARY and the STATE row.
</output>
