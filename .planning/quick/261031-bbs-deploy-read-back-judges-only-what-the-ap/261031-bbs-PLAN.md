---
phase: quick-261031-bbs
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/ignition-core/src/deploy/verify.rs
  - crates/ignition-core/src/actions/deploy.rs
  - crates/ignition-core/src/deploy/apply/mod.rs
  - crates/ignition-core/src/deploy/apply/converge.rs
  - crates/ignition-core/src/deploy/pair.rs
  - crates/ignition-cli/src/render.rs
  - crates/ignition-core/tests/common/fake_deploy.rs
  - crates/ignition-core/tests/deploy_verify.rs
  - crates/ignition-core/tests/deploy_apply.rs
  - crates/ignition-core/tests/live_deploy_apply.rs
  - crates/ignition-cli/tests/contract_deploy.rs
  - README.md
  - .planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-SUMMARY.md
autonomous: true
requirements: [QUICK-bbs]

estimate:
  tokens: 420000
  raw_tokens: 420000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "An `ign deploy apply` where an unwritten declared resource's REST view already differed from its file before the write ends `applied` (exit 0). That resource is one `data.warnings` line naming its key and the differing JSON paths. The post-apply `read_back` check is `warn` and its finding carries `status: \"warn\"` (offline scenario 1, per D-01/R4)"
    - "An apply during which an unwritten resource's REST view changes between the pre-apply baseline and the post-scan read, and still differs from its file, ends `rolled_back` with cause `verify_failed`. The run's `verify_checks` event names the resource key and the JSON paths, with no values (offline scenario 2, per D-01(b)/R5)"
    - "A written (`changed`) resource whose post-apply REST difference lies within the paths that differed between REST and its LIVE pre-apply file (an ancestor counts) passes with a warning (offline scenario 3). A path outside that set, or a write that did not take effect, is red and rolls back. A brand-new (`added`) resource is judged strictly, as before this change (per D-01(a)/R4)"
    - "A pair apply whose backup REST view has the same kind of pre-existing difference ends `applied`, not `applied_backup_unsynced`. The backup is judged against its own pre-write baseline. A backup whose written unit diverged still ends `applied_backup_unsynced` / `content_differs` (per R1)"
    - "Standalone `ign deploy verify` (master, and `--node backup|both`) reports a `read_back` difference or `resource_missing` as a `warn` finding plus a `data.warnings` line only when the plan status of that resource is `same`. `changed`, `added` or an unknown status stays red with exit 6. A verify whose only `read_back` findings warn is green and exits 0 (per D-02/R2)"
    - "Every run that reaches staging writes `read_back_baseline.json` before `staged`. Per node and per resource it holds only the state, a whole-config SHA-256, the basis and the uncapped differing paths, never a value. The baseline is read before `prepare` and outside the scan-lock, so the pinned event orders are unchanged. A baseline read that fails ends the run before anything is staged (per D-01/R5)"
    - "No slug and no finding code are added. A finding gains an optional `status` that is serialized only as `warn`, so existing envelopes are byte-identical. `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE` and `RENDER_GOLDEN` are unchanged. README documents the rules. The full gate is green (fmt, clippy, windows-msvc `cargo xwin clippy`, `cargo test --workspace`). There is no Cargo.toml or Cargo.lock change and no production call (per D-03)"
    - "On the Docker 8.3.6 rig, an apply that declares a resource whose REST view differs from its file before the run ends `applied`, with the warning naming that resource. A standalone verify of that tree is green with `read_back: warn`. The existing live apply gates still pass (per R6)"
  artifacts:
    - path: "crates/ignition-core/src/deploy/verify.rs"
      provides: "`ReadBackJudge` (Strict, Standalone, Baseline), `ReadBackBaseline`, `BaselineBasis`, `Written`, `evaluate_with`, `read_back_check_with`, the uncapped path sets, `compare_for`, `ReadBackBaseline::record`, and `Finding.status`"
      contains: "ReadBackJudge"
    - path: "crates/ignition-core/src/actions/deploy.rs"
      provides: "`VerifyContext` and `deploy_verify_judged`; standalone verify builds `ReadBackJudge::Standalone` from the plan statuses it already computes, for the master and for the backup"
      contains: "deploy_verify_judged"
    - path: "crates/ignition-core/src/deploy/apply/mod.rs"
      provides: "the pre-`prepare` baseline (master and backup), the live-file basis for written units, `read_back_baseline.json`, and `verify_checks` with findings"
      contains: "read_back_baseline.json"
    - path: "crates/ignition-core/src/deploy/apply/converge.rs"
      provides: "the backup content check judged against the backup's own baseline"
      contains: "read_back_backup_check_with"
    - path: "crates/ignition-core/tests/common/fake_deploy.rs"
      provides: "`FakeGateway::rest_overlay`, which models the gateway filling a default into the REST view"
      contains: "fn rest_overlay"
    - path: "crates/ignition-core/tests/deploy_apply.rs"
      provides: "the CONTEXT scenarios 1-3, the pair variant, the baseline-failure and value-free tests (`rb_apply_*`)"
      contains: "rb_apply_unwritten_moved_rolls_back"
    - path: "crates/ignition-core/tests/deploy_verify.rs"
      provides: "the pure judge tests (`rb_judge_*`) and the standalone action tests (`rb_verify_*`)"
      contains: "rb_judge_prefix_rule"
    - path: "crates/ignition-core/tests/live_deploy_apply.rs"
      provides: "the Docker rig gate for a pre-existing REST difference"
      contains: "live_preexisting_rest_difference_warns"
    - path: "README.md"
      provides: "the read_back row, the apply baseline step, the run-dir contents and secrets list, and the verdict-exception sentence"
      contains: "read_back_baseline.json"
  key_links:
    - from: "deploy::apply::drive (after `spec.problem()`, before `prepare`)"
      to: "actions::deploy::gather_read_back -> verify::ReadBackBaseline::build"
      via: "one REST sweep per node, fail-fast `?`, outside the scan-lock"
      pattern: "ReadBackBaseline::build"
    - from: "deploy::apply::confirm"
      to: "actions::deploy::deploy_verify_judged(VerifyContext::PostApply) -> verify::evaluate_with(ReadBackJudge::Baseline)"
      via: "the post-apply verify judges the master against its baseline; a red verdict still rolls back"
      pattern: "VerifyContext::PostApply"
    - from: "deploy::apply::converge::Phase::content"
      to: "pair::read_back_backup_check_with(ReadBackJudge::Baseline(backup baseline))"
      via: "`converge::Baseline` gains the backup's `ReadBackBaseline`"
      pattern: "read_back_backup_check_with"
    - from: "actions::deploy::deploy_verify / verify_backup_tick"
      to: "verify::ReadBackJudge::Standalone { file_status }"
      via: "the plan statuses of the non-tag units from the master read the action already makes"
      pattern: "ReadBackJudge::Standalone"
---

<objective>
**Execution: one executor run per task, in order.** Task 1, then Task 2, then Task 3. Each run starts with a fresh context and ends in its own commits. An executor reads this objective, the shared `<context>` below, and ONLY its own task. Do not start a later task in the same run.

Make `ign deploy apply` usable on the production pair (v1.6.3). Today `read_back` judges every declared non-tag resource against its file. The gateway's REST view of 23 production resources the canary apply never touched differs from their files through filled defaults and normalisation, so every production apply rolls back. After this plan:

- **Post-apply verify (D-01, R4).** Apply records a REST baseline before the write. `read_back` is red only for what this run changed or disturbed. Everything else that already differed is a warning.
- **Backup (R1).** The backup gets its own baseline from the backup node and the same rule.
- **Standalone verify (D-02, R2).** A `read_back` difference is a warning only when the plan says the resource is `same`.
- **Contract (D-03, R5).** Contract changes are additive: an optional finding `status`, `read_back_baseline.json`, and finding subjects in the `verify_checks` event. No slug and no finding code are added.

Task 1 is the tracer. It builds the whole pure judge, then wires it through the master apply path, from the fake gateway's REST through baseline, judge, confirm and outcome or rollback (scenarios 1 and 2). Task 2 expands it: the live-file basis for written units (scenario 3), the backup baseline and the pair variant, the baseline file, and the enriched `verify_checks` event. Task 3 does standalone verify for both nodes, the human renderer, the contract and action tests that change, README, the Docker rig gate and the full gate.

Purpose: the canary re-run on WIGNVM01/WIGNVM04 (separate, after the release) must reach `applied`. Rollback must still fire on anything this run actually broke.

Output: judge and baseline code, engine and verify wiring, offline and live tests, README, SUMMARY. No version bump, tag, release, worktree or production call.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md
@.planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-CONTEXT.md
@.planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-RESEARCH.md

RESEARCH.md holds the full file:line map. Line numbers here are as of commit 208c6bf on branch `fix/deploy-read-back-scope`. Task 1 shifts `verify.rs`, `actions/deploy.rs` and `apply/mod.rs`. Tasks 2 and 3 therefore grep the quoted anchor first and read around the hit.

**Orchestrator resolutions (binding; do not revisit):**
- **R1.** Backup convergence (`converge.rs`/`pair.rs`) gets its own baseline from the backup node before the master write, and uses the same rule.
- **R2.** For standalone verify, a `read_back` difference is a warning only when the plan status for that resource is `same`. `changed`/`added` stay red. `resource_missing` for a plan-`same` resource is also a warning.
- **R3.** Deleted units get no new `read_back` check: `read_back` never checked deletions. CONTEXT says "A deleted unit: the REST view must show it absent (as today)". That is corrected here: `read_back` has never checked deletions (`Expected` holds rendered resources only). Deletions stay judged by `files` (tag units) and the plan and ledger. SUMMARY records the correction.
- **R4.** The baseline is REST versus the LIVE file before the apply.
  - An unwritten resource is red only if its REST view changed from the baseline AND still differs from its file.
  - A written resource is red if the post-apply difference contains a path that is not in its baseline path set. A path counts as covered when it, or an ancestor of it, is in the set.
  - A brand-new resource is strict.
  - Path sets are uncapped.
- **R5.** `read_back_baseline.json` holds paths and a whole-config hash only, never values or per-path value hashes. The `verify_checks` event gains finding subjects (paths only).
- **R6.** Offline tests come first, through `FakeGateway::rest_overlay`: the three CONTEXT scenarios plus a pair variant. The live check runs on the Docker rig only (`live_deploy_apply.rs`).
- **R7.** The orchestrator's production stability check (two read-only verifies compared) runs separately. It does not block this plan.
- **Hard rules (H1).**
  - No production calls of any kind: no request, ssh, sftp or `ign` call to WIGNVM01, WIGNVM04 or any target in `whk-onprem-infra-iac`.
  - No worktrees. Run every cargo command sequentially on the main checkout (`.claude/CLAUDE.md`).
  - Stage files by explicit path only. Never stage `.planning/config.json`, `.planning/research/.cache/` or `svc-ignition-deploy@52.154.75ace-uat4.ps1`.
  - No co-author lines. No Cargo.toml or Cargo.lock change.
  - `PLAN_GOLDEN*` and `RENDER_GOLDEN` stay byte-identical.

**The judge (R4), per declared non-tag resource in `Expected.resources`.** Notation:
- r0 is the baseline REST read and r1 is the post-scan read.
- P is the uncapped path set of r1 against the declared file (the existing `Compare`).
- For an unwritten resource, B is the paths of r0 against the declared file. A `same` unit's live file equals the declared file.
- For a `changed` resource, B is the paths of r0 against the pre-apply LIVE file.
- "Moved" means the null-stripped r0 and r1 differ (`diff_exact` over the two REST values).

| Basis | Baseline | Post | Verdict |
|---|---|---|---|
| `declared_file` (unwritten) | Config(r0) | Config(r1) | P empty: pass. P non-empty and moved: red `resource_differs`, detail `differs at: <P>; its REST view changed during this apply at: <moved>`. P non-empty and not moved: warn |
| `declared_file` | Config | Missing | red `resource_missing` |
| `declared_file` | Missing | Missing | warn `resource_missing` |
| `declared_file` | Missing or System | Config(r1) | P empty: pass, else red (REST moved) |
| `live_file` (written `changed`) | Config(r0), B | Config(r1) | N = {p in P not covered by B}. N non-empty: red, detail `differs at: <N> (not different before this apply)`. N empty and P non-empty: warn. P empty: pass |
| `live_file` | any | Missing | red `resource_missing` |
| `strict` (`added`, a `changed` unit with no buildable live compare, r0 Missing or System, or no baseline entry) | any | any | as `evaluate` judges today |
| any | any | System | today's "not compared" warning |

Covered: B holds `(root)`, or B holds p, or B holds an ancestor a where p starts with a and the next character of p is `.` or `[`.

**Planner decisions (state them in SUMMARY):**
- **P1 (names).** `verify::ReadBackJudge<'a>` has the variants `Strict`, `Standalone { file_status: &'a BTreeMap<String, PlanStatus> }` and `Baseline(&'a ReadBackBaseline)`. The other names are `verify::ReadBackBaseline`, `verify::BaselineBasis { DeclaredFile, LiveFile, Strict }`, `verify::Written { Added, Changed(Option<Compare>) }`, `verify::evaluate_with`, `pub(crate) verify::read_back_check_with`, `verify::compare_for`, `pair::read_back_backup_check_with`, `actions::deploy::VerifyContext<'a> { Standalone, PostApply(&'a ReadBackBaseline) }`, `pub(crate) actions::deploy::deploy_verify_judged` and `FakeGateway::rest_overlay`. `ReadBackBaseline` avoids a clash with the existing `converge::Baseline` (the sync sample), which gains a field.
- **P2 (finding status).** `Finding` gains `status: Option<CheckStatus>`, with `#[serde(skip_serializing_if = "Option::is_none")]`. It is `Some(Warn)` only on a finding that does not count against its check. `None` means it counts, as today, so every existing envelope stays byte-identical.
- **P3 (check status).** Status is `Fail` if any finding has no warn status, else `Warn` if any finding is left, else `Pass`. On `Warn`, the read_back detail is `<n> declared resource(s) compared with the REST effective view; <w> difference(s) reported as warnings`. The `Pass` and `Fail` details are unchanged.
- **P4 (wording).** Paths use the existing `join_paths` (capped at 20, for text only; sets are always uncapped).
  - Post-apply, differs: `read_back: <key> differs from its file at: <paths> (already different before this apply)`.
  - Post-apply, missing: `read_back: <key> is not in the gateway's effective view (it was not there before this apply either)`.
  - Standalone, differs: `read_back: <key> differs from its file at: <paths> (the live files equal the repo; the gateway's REST view differs)`.
  - Standalone, missing: `read_back: <key> is not in the gateway's effective view (the live files equal the repo)`.
- **P5 (baseline file).** `run_dir/read_back_baseline.json` is `{"schema_version":1,"nodes":[{"node":"master","resources":{"<key>":{"state":"config|missing|system","rest_sha256":"<64 hex, config only>","basis":"declared_file|live_file|strict","differs_at":[<uncapped, byte-sorted paths>]}}},{"node":"backup",...}]}`. `rest_sha256` is `tree::sha256_hex` over compact JSON of the null-stripped REST config with keys sorted by an explicit recursive walk. Do not rely on serde_json's map-order feature. Write it with `RunDir::write_json` (0600).
- **P6 (timing).** Read the baseline after `spec.problem()` passes and before `ctx.host.prepare`, master first, then the backup. Any read error ends the run with `?`, like the first backup sample. Then nothing is staged, no lock is taken and the last event is `failed`.
- **P7 (event).** Each check object in `verify_checks` gains `findings: [{code, subject, detail}]`, plus `status: "warn"` where set. `id` and `status` are unchanged. The event is still emitted only on a red verdict. Details are value-free by construction (`Finding` doc: paths, codes and timestamps only).
- **P8 (backup written basis).** A written unit on the backup is judged on the backup's own r0 against the master's pre-apply live-file compare. The backup holds a synced copy, and its `files` check already holds it to the master's bytes. There is no backup tree read.
- **P9 (renderer).** In the human output, a finding with warn status prints ` (warn)` at the end of its line, in both the master renderer and the backup renderer.
- **P10 (standalone without a status).** A plan status can be missing, for example on `--node backup` against a backup with no file transport, where no master read happens. Such a resource stays strict: R2 warns only on `same`.
</context>

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1 (tracer): the pure read_back judge, wired end to end through the master apply path (scenarios 1 and 2)</name>
  <files>crates/ignition-core/src/deploy/verify.rs, crates/ignition-core/src/actions/deploy.rs, crates/ignition-core/src/deploy/apply/mod.rs, crates/ignition-core/tests/common/fake_deploy.rs, crates/ignition-core/tests/deploy_verify.rs, crates/ignition-core/tests/deploy_apply.rs</files>
  <precondition>Branch `fix/deploy-read-back-scope` is checked out at 208c6bf or a descendant. `git status --short` shows only the three H1 files, plus this task directory, as non-HEAD changes.</precondition>
  <read_first>
    - `crates/ignition-core/src/deploy/verify.rs` 150-300 (`CheckStatus`, `Finding`, `Check`, `VerifyReport`, `Compare`, `ExpectedResource`, `Expected`), 386-404 (`Remote`), 445-545 (`ObservedState`, `evaluate`, `finding`, `sort_findings`, `finished`) and 598-830 (`read_back_check`, `strip_nulls`, `child_path`, `root_or`, `diff_exact`, `diff_subset`, `diff_attributes`, `join_paths`).
    - `crates/ignition-core/src/actions/deploy.rs` 1075-1160 (`deploy_verify`) and 1420-1471 (`gather_read_back`).
    - `crates/ignition-core/src/deploy/apply/mod.rs` 1195-1260 (`batch_spec`, `spec.problem()`, `prepare`, `HeldLock::acquire`), 1300-1330 (the `confirm` call site and `roll_back`) and 1440-1520 (`confirm`).
    - `crates/ignition-core/tests/common/fake_deploy.rs` 195-250 (`reload_from_disk`) and 290-460 (constructors and knobs).
    - `crates/ignition-core/tests/deploy_apply.rs` 50-60 and 190-200 (the `DB_*`/`ALARM_*` constants), 200-245 (`Rig`), 425-460 (`changing_rig*`), 960-980 (a run ending `failed`), 1337-1372 (`scenario_verify_red` and the `host.after("commit", …)` hook) and 1493-1556 (`auto_rollback_verify_red_restores`). Also grep `Hooked::new` and `fn armed_after_commit` for the observer and fault patterns.
    - `crates/ignition-core/tests/deploy_verify.rs` 40-205 (helpers and a green `ObservedState`).
  </read_first>
  <behavior>
    - rb_judge_unwritten_preexisting_warns: `declared_file`, r0 = r1 = file plus member `filled`. `ok` is true, read_back is `Warn`, one `resource_differs` finding has status Warn, and the warnings hold the P4 post-apply line naming the key and `filled`.
    - rb_judge_unwritten_moved_and_differs_fails: r0 = file, r1 = file plus `filled`. The check is red and the detail names `filled` and "changed during this apply".
    - rb_judge_unwritten_moved_toward_file_passes: r0 = file plus `x`, r1 = file. The check passes.
    - rb_judge_unwritten_config_then_missing_fails, rb_judge_missing_both_sides_warns, rb_judge_missing_then_config_differs_fails: the Missing transitions in the table.
    - rb_judge_written_within_live_baseline_warns: `live_file`, B = {`settings`}, P = {`settings.a`}. Warn.
    - rb_judge_written_new_path_fails: B = {`settings`}, P = {`other`}. Red, and the detail names `other` only.
    - rb_judge_prefix_rule: B = {`a`} covers `a.b` and `a[n]` but not `ab`. B = {`(root)`} covers everything.
    - rb_judge_write_not_taken_effect_fails: the live file equals r0, the desired file changes member `m`, and r1 = r0. Red at `m`.
    - rb_judge_added_is_strict: `Written::Added` with a P like the warn case is red. So is `Changed(None)`.
    - rb_judge_uncapped_paths: B has 25 paths, and P is those 25 paths. Warn, not red: sets never come from the capped detail.
    - rb_judge_standalone_same_warns: status `Same`, for differs and for missing. Warn, with the P4 standalone lines.
    - rb_judge_standalone_changed_or_unknown_fails: statuses `Changed` and `Added`, and a key absent from the map, stay red.
    - rb_judge_mixed_check_status: one warn and one red finding. The check is `Fail`, `ok` is false, and the warn finding keeps status Warn.
    - rb_judge_finding_status_serialization: a red finding's JSON has no `status` key, and a warn finding's has `"status":"warn"`. `evaluate` (Strict) output of an existing CV2 case is unchanged.
    - rb_apply_preexisting_unwritten_difference_warns (scenario 1): an overlay on `ALARM_UNIT` (unwritten in the one-change rig) before the run. The outcome is `applied`, the outcome warnings name `ignition/general-alarm-settings` and the overlay member, and the event order equals a plain applied run.
    - rb_apply_unwritten_moved_rolls_back (scenario 2): the overlay on `ALARM_UNIT` is armed in a `host.after("commit", 1, …)` hook. The run ends `rolled_back` with cause `verify_failed`.
    - rb_apply_baseline_read_failure_writes_nothing: a `resource_list` or `resource_singleton` fault is armed by an observer on `GwbkDownloaded`. The run ends with the gateway error and no `staged` event, `lock_acquires() == 0`, the mode collection is unchanged and the last event is `failed`.
  </behavior>
  <action>
    This task implements D-01 and R4 (the whole pure judge, including the Standalone variant used by Task 3) and the master tracer path. Follow superpowers test-driven-development.

    1. RED.
       - Add `FakeGateway::rest_overlay(&self, key: &str, member: &str, value: serde_json::Value)` to `tests/common/fake_deploy.rs`. It stores `(key, member) -> value` in `FakeState` and applies it to the matching record's `config` object, at the top-level member, every time `reload_from_disk` runs and immediately when set. Match `key` as `module/type` or `module/type/name` against the record's `resource_type` and `name`. Document it as "the gateway filling a default into its REST view".
       - Add a `one_change_rig()` helper in `deploy_apply.rs`. It is `changing_rig` without `fx.remove(MODE, ALARM_UNIT)`: POSTGRES gets +1 byte and the ledger is kept. Then `ignition/general-alarm-settings` is plan-`same` and unwritten, and POSTGRES is `changed`.
       - Write every `rb_judge_*` test in `tests/deploy_verify.rs` and the three `rb_apply_*` tests above. For pure tests, build `Expected` from `ExpectedResource::exact` (or `ExpectedResource::new` with `Compare`), `BTreeMap<String, Remote>` maps for r0 and r1, and a green `ObservedState` otherwise.
       - Add the new public items with real signatures first, so the tests compile. While RED, `evaluate_with` judges every variant strictly and drive takes no baseline. The tests then fail on assertions.
       - Run them, record the failing names for SUMMARY, and commit `test(261031-bbs): read_back judge and baseline scenarios (RED)`.
    2. GREEN, `verify.rs`.
       - Add `Finding.status` (P2) and update every constructor the compiler flags, including `pair.rs` `fn finding`.
       - Split the per-resource comparison out of `read_back_check` into a pure `fn read_back_paths(compare: &Compare, rest: &Value) -> BTreeSet<String>`. It returns uncapped, deduplicated paths with the existing strip-nulls, diff and printable rules unchanged.
       - Add `ReadBackBaseline::build(node: &str, expected: &Expected, before: &BTreeMap<String, Remote>, written: &BTreeMap<String, Written>) -> ReadBackBaseline`. Per key it keeps a `BaselineEntry { basis, before: Remote, differs_at: BTreeSet<String> }` per the table: `DeclaredFile` for keys not in `written`; `LiveFile` for `Changed(Some(compare))` with a Config r0 (B against that compare); `Strict` otherwise. Its `Debug` must never print a value. `Remote`'s Debug already withholds values; do not derive `Serialize` on it.
       - Add the covered-path rule and `pub(crate) fn read_back_check_with(expected, read_back, judge: &ReadBackJudge, warnings) -> (Check, usize)` implementing the table, P3 and P4. Keep `read_back_check` as the `Strict` wrapper; pair and backup callers and `pair/tests.rs` depend on it.
       - Add `pub fn evaluate_with(expected, observed, judge) -> VerifyReport`, and make `evaluate` call it with `ReadBackJudge::Strict`.
       - Doc-comment the residual risks from RESEARCH: masking of a written path the gateway also normalises, the window false-red, and the strict judging of `added` units.
    3. GREEN, wiring.
       - In `actions/deploy.rs`, add `VerifyContext` and `deploy_verify_judged(declared, project, deploy, gateway, reader, opts, ctx: VerifyContext<'_>)`. It is today's `deploy_verify` body, with `evaluate_with`. `PostApply(b)` maps to `ReadBackJudge::Baseline(b)`. `Standalone` maps to `ReadBackJudge::Strict` in this task; Task 3 switches it to the plan statuses. `deploy_verify` keeps its signature and calls it with `Standalone`.
       - In `apply/mod.rs` `drive`, after `spec.problem()` and before `prepare` (P6):
         - Build `Expected::from_render` for the target mode.
         - Build `written` from the plan entries with status `Added` (`Written::Added`) or `Changed` (`Written::Changed(None)`; Task 2 adds the live compare) whose key is in `expected.resources`.
         - Read r0 with `gather_read_back(ctx.deploy, &expected).await?`, then `ReadBackBaseline::build("master", …)`.
       - Pass the baseline to `confirm`, which calls `deploy_verify_judged(…, VerifyContext::PostApply(&baseline))`. No REST or tree read may sit between `LockAcquired` and the commit.
       - Update the module doc's step list in `apply/mod.rs` with the baseline step.
    4. Run the verify command. Fix any regression at its cause; never weaken an assertion. Commit `feat(261031-bbs): read_back judged against a pre-apply REST baseline (tracer)`. Stage by explicit path (H1).
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --test deploy_verify rb_judge 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=16)}' && cargo test -p ignition-core --test deploy_apply rb_apply 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=3)}' && cargo test -p ignition-core --lib --test deploy_verify --test deploy_apply 2>&1 | grep -E '^test result' && cargo test -p ignition-cli --test contract_deploy 2>&1 | grep -E '^test result' && cargo clippy -p ignition-core --all-targets -- -D warnings && grep -q 'VerifyContext::PostApply' crates/ignition-core/src/deploy/apply/mod.rs && echo T1-OK</automated>
  </verify>
  <done>`T1-OK` is printed. There are at least 16 `rb_judge_*` and 3 `rb_apply_*` tests, all passing. Every existing test in the ignition-core lib, `deploy_verify`, `deploy_apply` and the CLI `contract_deploy` passes unchanged: the event orders, `auto_rollback_verify_red_restores` and the CV2 `evaluate` cases. Two commits (RED, then GREEN) are on `fix/deploy-read-back-scope`, staged by explicit path, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: written units on the live-file basis (scenario 3), the backup baseline and pair variant (R1), `read_back_baseline.json` and `verify_checks` findings (R5)</name>
  <files>crates/ignition-core/src/deploy/verify.rs, crates/ignition-core/src/deploy/apply/mod.rs, crates/ignition-core/src/deploy/apply/converge.rs, crates/ignition-core/src/deploy/pair.rs, crates/ignition-core/tests/common/fake_deploy.rs, crates/ignition-core/tests/deploy_verify.rs, crates/ignition-core/tests/deploy_apply.rs</files>
  <precondition>Task 1's GREEN commit is HEAD, and `cargo test -p ignition-core --test deploy_apply rb_apply` passes.</precondition>
  <read_first>
    Grep each anchor first, then read around it (Task 1 shifted lines).
    - `verify.rs`: grep `pub fn from_render` (the per-resource closure that becomes `compare_for`) and `pub struct ReadBackBaseline`.
    - `apply/mod.rs`: grep `ReadBackBaseline::build`, `VerifyChecks {` (the variant and its `json!` arm), `fn confirm` and `converge::Baseline {`.
    - `apply/preflight.rs` 392-404 (a targeted `reader.read(&Scope { mode, types, tag_providers })`).
    - `pair.rs` 125-140 (`BackupNode::read_back`), 300-360 (`BackupCheck`, `fn finding`), 445-480 (`backup_check_from`, `read_back_backup_check`), 510-525 (`converged`) and 670-682 (`effective_files`).
    - `apply/converge.rs` 60-135 (`Phase`, `converge_backup`, `Baseline`) and 250-280 (`content`).
    - `tests/common/fake_deploy.rs`: grep `pub fn with_pair`, `fn sync` or `copy_collection`, and `pub fn diverge` (how a sync reloads the backup).
    - `tests/deploy_apply.rs`: grep `async fn with_pair`, `async fn backup_unsynced_content_differs`, `fn assert_unsynced`, `fn backup_options`, `fn run_dir_canary_leaks` and `async fn canary_rig`, and the run-dir `names(&run_dir)` listing in `apply_happy_path_local` (around 685-695).
  </read_first>
  <behavior>
    - rb_judge_record_value_free: a baseline whose r0 carries a string canary value `CANARY-rb-0f3a` under member `secretish`. `record()` serializes without the canary and lists `secretish` in `differs_at`. `rest_sha256` is 64 lowercase hex characters and is identical for two r0 values that differ only in key order and null members.
    - rb_apply_written_preexisting_difference_passes (scenario 3): an overlay on `DB_UNIT` (written, `changed`) before the run, kept through the scan. The outcome is `applied`, with a warning naming POSTGRES and the overlay member.
    - rb_apply_written_new_difference_rolls_back: an overlay on `DB_UNIT` armed in `host.after("commit", 1, …)`. The run ends `rolled_back` / `verify_failed`, and the `verify_checks` read_back entry has a finding with subject `ignition/database-translator/POSTGRES` whose detail names the member.
    - rb_apply_unwritten_moved_rolls_back (extended from Task 1): `verify_checks` carries a finding with subject `ignition/general-alarm-settings`, and its detail names the overlay member.
    - rb_apply_pair_backup_preexisting_difference_converges: a pair rig (one-change), with an overlay on `ALARM_UNIT` on the backup gateway only, set before the run. The outcome is `applied`, not `applied_backup_unsynced`. The backup report is `converged`, its read_back check is `warn`, and one finding with status Warn names the key.
    - rb_apply_baseline_file_value_free: an overlay on `ALARM_UNIT` with value `CANARY-rb-0f3a`. `read_back_baseline.json` exists with mode 0600, `schema_version` 1 and a `master` node listing the key with `basis` `declared_file` and the member in `differs_at`. `run_dir_canary_leaks` finds nothing.
    - backup_unsynced_content_differs (existing, unchanged) still ends `applied_backup_unsynced` / `content_differs` with a red read_back naming POSTGRES. apply_happy_path_local's run-dir listing gains `read_back_baseline.json`.
  </behavior>
  <action>
    This task implements D-01(a), R1, R4 (written units) and R5. TDD as in Task 1.

    1. RED. Write the tests above, and add `read_back_baseline.json` to the `apply_happy_path_local` listing and to any other full run-dir listing that grep finds. If the pair harness keeps the backup `FakeGateway` private, expose it on `Rig` or return it from `with_pair`. Run, record the failures, and commit `test(261031-bbs): written-unit basis, backup baseline, baseline file (RED)`.
    2. `verify.rs`.
       - Move `from_render`'s per-resource closure into `pub fn compare_for(module: &str, ty: &str, file: impl Fn(&str) -> Option<&[u8]>) -> Result<Compare, &'static str>`. The `Err` value is the existing not-checked reason. Keep `from_render`'s output identical.
       - Add `ReadBackBaseline::record(&self) -> BaselineRecord` (P5, `Serialize`, value-free) and the canonical hash helper.
    3. `apply/mod.rs`.
       - **Written basis (R4).** Before the master r0 read, when any `Changed` key is in `written`, make one targeted `ctx.reader.read(&Scope { mode, types: <the distinct (module, ty) of those keys>, tag_providers: Vec::new() }).await?` and apply `pair::effective_files`. Build `Written::Changed(compare_for(…).ok())` from the live bytes at `<key>/<file>`. Skip the read when no such key exists, as in the canary case.
       - **Backup (R1, P8).** When `ctx.backup` is set, after the master baseline, call `backup.read_back(&expected).await?` and `ReadBackBaseline::build("backup", &expected, &r0_backup, &written)` with the same `written`. Hand it to `converge::Baseline` through a new field `read_back: ReadBackBaseline`. Rename nothing.
       - **Persist (R5).** Write `{"schema_version":1,"nodes":[master record, backup record when present]}` to `read_back_baseline.json` with `run_dir.write_json` before `prepare`.
       - **Event (R5, P7).** Change `ApplyEvent::VerifyChecks` to carry per-check findings, and extend its `json!` arm. Keep `id` and `status` as they are.
    4. `converge.rs` and `pair.rs`. Add `pair::read_back_backup_check_with(expected, read_back, judge, warnings)` beside the strict `read_back_backup_check`, which stays for the pair tests. In `Phase::content`, judge with `ReadBackJudge::Baseline(&self.read_back_baseline)`. `converged` and the V2 reason logic already treat `Warn` as not failing; do not change them.
    5. Run the verify command. Commit `feat(261031-bbs): written-unit live basis, backup baseline, read_back_baseline.json, verify_checks findings`, staged by explicit path (H1).
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && cargo test -p ignition-core --test deploy_verify rb_judge 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=17)}' && cargo test -p ignition-core --test deploy_apply rb_apply 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=7)}' && cargo test -p ignition-core --test deploy_apply backup_unsynced_content_differs 2>&1 | grep -E '^test result: ok\. 1 passed' && cargo test -p ignition-core --lib --test deploy_verify --test deploy_apply 2>&1 | grep -E '^test result' && cargo test -p ignition-cli --test contract_deploy 2>&1 | grep -E '^test result' && cargo clippy -p ignition-core --all-targets -- -D warnings && grep -q 'read_back_baseline.json' crates/ignition-core/src/deploy/apply/mod.rs && grep -q 'read_back_backup_check_with' crates/ignition-core/src/deploy/apply/converge.rs && echo T2-OK</automated>
  </verify>
  <done>`T2-OK` is printed. Scenario 3, the written-new-path rollback, the pair variant and the value-free baseline file all pass. `backup_unsynced_content_differs` stays red. The only change to pinned run-dir listings is `read_back_baseline.json`, and the pinned event orders are unchanged. The `verify_checks` event names finding subjects with paths and no values. Two commits (RED, then GREEN), staged by explicit path, with no co-author line.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: standalone verify warns only on plan-`same` (R2, master and backup), the human renderer, README parity, the Docker rig gate and the full gate</name>
  <files>crates/ignition-core/src/actions/deploy.rs, crates/ignition-cli/src/render.rs, crates/ignition-core/tests/deploy_verify.rs, crates/ignition-cli/tests/contract_deploy.rs, crates/ignition-core/tests/live_deploy_apply.rs, README.md, .planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-SUMMARY.md</files>
  <precondition>Task 2's GREEN commit is HEAD. Docker can run: `docker info` succeeds, or `open -a Docker` brings it up within 180 s.</precondition>
  <read_first>
    Grep each anchor first, then read around it.
    - `actions/deploy.rs`: grep `pub(crate) async fn deploy_verify_judged`, `pub async fn deploy_verify_node`, `async fn verify_backup_tick` and `async fn verify_backup_files`.
    - `crates/ignition-cli/src/render.rs` 1520-1545 and 1860-1880 (the two finding-line loops).
    - `tests/deploy_verify.rs`: grep `async fn verify_green_and_red_through_action`, `fn node_pair`, `async fn run_node` and `async fn verify_node_both_carries_the_backup_read_back_warnings`.
    - `crates/ignition-cli/tests/contract_deploy.rs` 1705-1760 (`mount_verify_gateway`, `omit_translator`, `local_tracer`), 1810-1860 (`verify_cli_red_exits_6`) and 1990-2060 (`verify_cli_human_lines`).
    - README: grep the `read_back` row of the verify checks table (around 3349), `### The \`deploy verify\` verdict exception` (around 102), `### \`deploy apply\`` and its numbered steps (around 3514), the run-directory contents and the "Where secrets can be, and where they cannot" paragraph (around 3594-3610), the verify command row (around 581) and the `--node` backup check text (around 3575).
    - `crates/ignition-core/tests/live_deploy_apply.rs` 1-60 (env table, guards), 221-260 (`rig`), then grep `async fn live_apply_then_rollback` and read its setup, apply and cleanup (the model).
    - `crates/ignition-core/tests/fixtures/live-rig-deploy/capture22.sh` usage (`up22`, `setup22`).
  </read_first>
  <behavior>
    - rb_verify_standalone_same_warns_through_action: through `deploy_verify` on the existing action harness (the `verify_green_and_red_through_action` model), a resource omitted from the fake REST, or carrying an extra member, while the live files are plan-`same`. `ok` is true, read_back is `warn`, the finding has status Warn, and `data.warnings` has the P4 standalone line.
    - rb_verify_standalone_changed_stays_red_through_action: the same REST difference with the live file edited, so the plan says `changed`. Red, with `failure_exit_code() == Some(6)`.
    - rb_verify_backup_same_warns: `deploy_verify_node(VerifyNode::Both)` with the backup missing a resource that is plan-`same`. The backup read_back is `warn`, the backup is `converged` and `ok` is true.
    - verify_green_and_red_through_action: its red half now also drifts the live file, so the plan is `changed` and it stays red. Add a warn twin.
    - verify_cli_red_exits_6 and verify_cli_human_lines: also edit the live translator config in the tracer's data dir, so the plan says `changed`. Every existing assertion is unchanged.
    - verify_cli_preexisting_warns_exit_0 (new): `omit_translator` with the live files `same`. Exit 0, `data.ok` true, read_back `warn`, finding `status` `warn`, and the human line ends with `(warn)`.
  </behavior>
  <action>
    This task implements D-02, R2, D-03 (contract and README) and R6 (live).

    1. RED. Write the tests above and adjust the three existing ones as stated. Run and record the failures, then commit `test(261031-bbs): standalone read_back warns on plan-same (RED)`.
    2. GREEN, `actions/deploy.rs`.
       - **Master.** In `deploy_verify_judged`, collect `file_status: BTreeMap<String, PlanStatus>` for the non-tag keys in `expected.resources`, from the `planned` it already builds. `Standalone` then maps to `ReadBackJudge::Standalone { file_status: &file_status }`, and `PostApply` stays as it is. Return the statuses to `deploy_verify_node`; an inner function returning `(DeployVerifyResult, BTreeMap<String, PlanStatus>)` is fine.
       - **Backup.** In `verify_backup_tick`, judge the backup's read_back with `pair::read_back_backup_check_with` and `Standalone`. Take the statuses from the master result (`--node both`), or from the master read that `verify_backup_files` already makes, run through `plan(…)` there (`--node backup`). Compute the files check before judging the read_back if needed, and keep the report order `[mode, read_back, files]`. If no statuses are available, stay strict (P10). Add no extra tree read.
       - Update the doc comments of `deploy_verify` and `deploy_verify_node` with the rule.
    3. `render.rs` (P9): a finding whose `status == Some(CheckStatus::Warn)` ends with ` (warn)` in both loops.
    4. README (D-03).
       - **read_back row.** Passes when every declared resource is present over REST with an equal `config`. Standalone: a difference, or `resource_missing`, on a resource the plan reports `same` is a `warn` finding (`"status": "warn"`) plus a `data.warnings` line; `changed`/`added` stays red. After an apply, read_back is judged against the pre-apply baseline (link to the apply section).
       - **Checks intro.** Add one sentence on the per-finding `status`.
       - **`--node` backup text.** The backup gets the same standalone rule.
       - **Apply steps.** The baseline step, before staging and outside the lock: the R4 rules in prose; `added` units are strict (the hand-written minimal config risk, mitigated by `deploy pull`); the accepted residual risks (a written path the gateway also normalises is masked; an operator change between the baseline and the scan reads as this run's disturbance).
       - **Backup in a pair.** It is judged on its own baseline (R1).
       - **Run directory.** `read_back_baseline.json` (P5) is in the contents list and in the "cannot hold secrets" list.
       - **Verdict exception.** One sentence: a green verdict whose `read_back` is `warn` exits 0.
       - **Unchanged.** Leave every exit-code row as it is.
    5. Live (R6, Docker rig only).
       - Run `cargo install --path crates/ignition-cli --force`.
       - Bring the rig up: `cd crates/ignition-core/tests/fixtures/live-rig-deploy && bash capture22.sh up22 && bash capture22.sh setup22`, then source `~/.cache/ign-deploy-rig/tokens.env`.
       - **Discovery, time-boxed to about 30 minutes.** Pull the rig's live tree into a temp repo, the way `live_deploy_plan.rs` does, and run a standalone verify of it. Any read_back `warn` finding is a stock resource the gateway normalises. If none appears, seed one: in the rig's mode collection, write a minimal `config.json` for a non-tag resource that omits a member the gateway fills, scan, and confirm over REST that the member comes back.
       - **Test.** Add `live_preexisting_rest_difference_warns` (`#[ignore]`, quiet green skip without `IGNITION_LIVE_DEPLOY=1`), modelled on `live_apply_then_rollback` under its process-wide lock and name guard. Declare that resource byte-equal to live (plan `same`) plus one changed unit. Apply: the outcome is `applied` and its warnings contain `read_back: <key> differs from its file at:`. Then a standalone `deploy_verify` of the same tree: `ok` is true, read_back is `warn`, and the finding status is `warn`. Cleanup restores every file it wrote, including any seed, even on failure.
       - Document the resource, the member, and found-versus-seeded in the test's doc comment.
       - **Regression.** With `IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 IGNITION_LIVE_TOKEN=$IGN_TOKEN_CAP_A IGN_DEPLOY_RIG_DATA=$HOME/.cache/ign-deploy-rig/data IGN_DEPLOY_RIG_MODE=production`:
         - Run `bash capture-cd5.sh seed`, then `cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1`. This runs the new test, the five Phase 23 gates including `live_queued_config_save_during_apply`, and the cd5 test.
         - Run `cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture --test-threads=1`. In `pull_then_verify_is_green_live`, step (d) must stay red: the plan says `changed`.
         - Then run `bash capture-cd5.sh clean` and `docker compose -p ign-deploy-rig down -v`.
       - If a live test fails, debug it with superpowers systematic-debugging. Never ignore a test or loosen an assertion to make it pass.
    6. Write `261031-bbs-SUMMARY.md` in the task directory. It records:
       - R1 to R7, P1 to P10, and the R3 CONTEXT correction.
       - The RED evidence per task, read from `git log`.
       - A `## Live` section with each command (env values named, never printed), its UTC time and every `test result:` line, then exactly one line `Live verdict: pass`.
       - The residual risks (masking, window, strict `added`, assumption A1).
       - The hand-off, not done here: after merge, the orchestrator tags v1.6.3 on `origin/main` per RELEASING.md; R7's production stability comparison precedes the separate canary re-run on WIGNVM01/WIGNVM04 with the released binary.
    7. Run the full gate (the verify command). Commit `feat(261031-bbs): standalone read_back warns on plan-same; README; live rig gate`, staging every changed file of this task by explicit path, the SUMMARY included (H1).
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && set -o pipefail && S=.planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-SUMMARY.md && test "$(grep -cE '^Live verdict: pass$' "$S")" = 1 && test "$(grep -c 'test result: ok' "$S")" -ge 2 && grep -q 'live_preexisting_rest_difference_warns' crates/ignition-core/tests/live_deploy_apply.rs && grep -q 'read_back_baseline.json' README.md && grep -q 'ReadBackJudge::Standalone' crates/ignition-core/src/actions/deploy.rs && grep -q 'a89de5043275c1b057289e89cdd78e05bc3d32680ce169e1d885e1a22708c473' crates/ignition-core/tests/deploy_plan.rs && grep -q '5084afccdf21436e192a5e71b8da3b4ba5cafa49fa822552ab393ffdc963f87a' crates/ignition-core/tests/deploy_plan.rs && grep -q 'ef3832a4703a23afa5b7da611ab61662f665d8bd0513355bc1b4bee0bbbc36ae' crates/ignition-core/tests/deploy_plan.rs && grep -q '7c8b41b16aa7680fac4e3cc1b4ac0ba3031973a720f33a625079e408f5b376a1' crates/ignition-cli/tests/contract_deploy.rs && cargo test -p ignition-core --test deploy_verify rb_verify 2>&1 | grep -E '^test result: ok\. ' | awk '{exit !($4>=3)}' && cargo test -p ignition-cli --test contract_deploy verify_cli 2>&1 | grep -E '^test result: ok' && cargo test -p ignition-core --lib error:: 2>&1 | grep -E '^test result: ok' && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep -E '^test result' | tail -3 && PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings && git diff --quiet "$(git merge-base HEAD main)" -- Cargo.toml Cargo.lock crates/ignition-cli/Cargo.toml crates/ignition-core/Cargo.toml crates/ignition-tui/Cargo.toml && echo FULL-GATE-OK</automated>
  </verify>
  <done>`FULL-GATE-OK` is printed: fmt, workspace clippy, every `cargo test --workspace` result at 0 failed, and the windows-msvc `cargo xwin clippy` are green. All four goldens are byte-identical, and there is no Cargo.toml or Cargo.lock change since the merge base. Standalone verify warns only on plan-`same` for both nodes, and the CLI exits 0 on a warn-only verdict and 6 on a `changed` one. README documents the baseline, the rules, the file and the warn status. SUMMARY carries `Live verdict: pass` with the Docker `test result: ok` lines and the v1.6.3 hand-off. Six commits are on `fix/deploy-read-back-scope` (RED and GREEN per task), with no co-author line and no H1 file.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| gateway REST -> verify judge | gateway-supplied configs and keys decide pass, warn or fail and roll back |
| engine -> run directory | the baseline and the events persist what REST returned |
| apply -> gateway (write window) | the time between the baseline read and the post-scan read |
| live tests -> rig | tokens and the bind-mounted data dir during the Docker gate |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-bbs-01 | Spoofing (false green, fail-open verify) | `read_back_check_with` Baseline and Standalone | high | mitigate | Written `changed` units are judged against the LIVE pre-apply file (B), not the desired file. `added` units stay strict. Path sets are uncapped and never come from the capped detail. Tests cover the prefix rule, a new path and a write that did not take effect (`rb_judge_write_not_taken_effect_fails`). Standalone stays red unless the plan says `same`, with no status meaning strict (P10). Rollback on a real red is unchanged, and no flag skips verify |
| T-bbs-02 | Information disclosure | `read_back_baseline.json`, the `verify_checks` event, warnings | high | mitigate | Only paths, the state, the basis and a whole-config SHA-256 are kept, never values or per-path hashes (R5). r0 stays in memory with withheld `Debug`. The file is written 0600 via `RunDir::write_json`. `rb_judge_record_value_free`, `rb_apply_baseline_file_value_free` and the existing `run_dir_canary_leaks` walker check this. Gateway keys pass through `printable()` |
| T-bbs-03 | Denial of service (every apply rolls back) | rule (b) over a volatile REST field or a foreign change in the window | medium | mitigate | Red only when REST moved AND still differs from the file, so a move toward the file passes. The window is kept short (baseline after the gwbk, just before staging). Assumption A1 is checked by the orchestrator's R7 production stability comparison before the canary re-run, and documented in README and SUMMARY |
| T-bbs-04 | Repudiation (undiagnosable rollback) | the run record | low | mitigate | `verify_checks` carries the finding subjects and paths, and `read_back_baseline.json` records what differed before the write (R5) |
| T-bbs-05 | Denial of service (lock hold exhausted) | baseline timing | medium | mitigate | The REST and live-file reads happen before `prepare` and `HeldLock::acquire`, with no read between `LockAcquired` and the commit (P6). The pinned event orders prove the order is unchanged |
| T-bbs-06 | Tampering (false unsynced or false converged backup) | `converge.rs` content | medium | mitigate | The backup is judged on its own pre-write r0 (R1, P8). `backup_unsynced_content_differs` stays red, and `rb_apply_pair_backup_preexisting_difference_converges` goes green |
| T-bbs-07 | Information disclosure (hash brute force) | `rest_sha256` | low | accept | One whole-config hash only, as R5 allows. The 0700 run dir already holds the `.gwbk` with the real values |
| T-bbs-08 | Elevation / Tampering | an accidental production call | critical | mitigate | H1: no production profile, target or host is named. The live tests keep their loopback URL guard and `ign-deploy-rig` name guard |
| T-bbs-SC | Tampering | package installs | low | accept | No new crate. Task 3's verify asserts no Cargo.toml or Cargo.lock change since the merge base |
</threat_model>

<verification>
- **Task 1:** at least 16 `rb_judge_*` and 3 `rb_apply_*` tests pass (scenarios 1 and 2 and the baseline-failure run). The CV2 `evaluate` cases, the pinned event orders and the CLI `contract_deploy` pass unchanged.
- **Task 2:** scenario 3, the written-new-path rollback, the pair variant and the value-free baseline file pass. `backup_unsynced_content_differs` stays red. `verify_checks` carries the finding subjects.
- **Task 3:** the standalone warn and red cases pass for the master and the backup, plus the CLI warn exit 0 and the human `(warn)` line. README is in parity. The Docker live gate is `pass`. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` are green. The goldens are byte-identical, and there is no Cargo.toml or Cargo.lock change.
- **Scope:** no worktree, no version bump, no tag, no release, no production call. The H1 files are untouched and unstaged.
</verification>

<success_criteria>
- A production-shaped apply, where unwritten resources already differ from their files over REST, reaches `applied` with one warning per resource. A run that disturbs an unwritten resource, or whose written unit gains a new difference, still rolls back.
- A redundant pair no longer ends `applied_backup_unsynced` on pre-existing backup differences.
- Standalone verify is green with warnings when the files are `same`, and red when repo and live disagree.
- The run directory alone explains a rollback: the baseline file plus the finding subjects in `verify_checks`.
- The full gate, the Windows cross-check and the Docker live gate are green, with six commits on `fix/deploy-read-back-scope` and the v1.6.3 hand-off in SUMMARY.
</success_criteria>

<output>
Create `.planning/quick/261031-bbs-deploy-read-back-judges-only-what-the-ap/261031-bbs-SUMMARY.md` when done (Task 3, step 6). It records:
- R1 to R7, P1 to P10, and the R3 CONTEXT correction.
- The RED evidence per task.
- The `## Live` section with `Live verdict: pass`, and how the live resource was found or seeded.
- The residual risks.
- The hand-off: the v1.6.3 tag per RELEASING.md after merge, then the R7 production stability comparison, then the separate production canary re-run.
</output>

## Checker amendments (binding, orchestrator 2026-10-31)

- **A1 (checker W1, judge_safety):** in the `LiveFile` basis, a written `changed` unit is also red when its desired file differs from its pre-apply live file but its REST view did not move between the baseline and the post-apply read (`diff_exact(r0, r1)` is empty). That is, the write did not take effect. This closes the ancestor/`(root)` masking case, which production has (`namespace-server-set/1,2,7,8` differ at the root). Add `rb_judge_write_not_taken_effect_root_covered_fails` (baseline set = {`(root)`}) to Task 1, and the equivalent apply-level case to Task 2. The masking note in the plan's risk section becomes: covered by A1.
- **A2 (checker W2):** retitle RESEARCH.md's open questions section to `## Open Questions (RESOLVED)` with inline resolutions (deletions: R3; wording: P4).
