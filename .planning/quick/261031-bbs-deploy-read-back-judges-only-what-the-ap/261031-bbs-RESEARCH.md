# Quick 261031-bbs: deploy read_back judges only what the apply changed - Research

**Researched:** 2026-10-31
**Domain:** `ign deploy` verify/apply engine (Rust, codebase-only research)
**Confidence:** HIGH for code facts (every file:line below was read this session). MEDIUM for timing attribution. LOW for the rig reproduction member.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
**Post-apply read_back ("Baseline diff")**
- Before the write (inside apply, before the lock or at least before commit), apply records a **REST baseline**: the REST view of every declared resource that `read_back` would compare.
- After the scan, read_back **fails** only on:
  - (a) a unit this run wrote, whose post-apply REST view does not match what was declared. "Match" must not false-fail on the gateway's own normalisation of a written resource. Recommended: compare the set of differing JSON paths. A path that already differed between REST and file in the baseline, for the same resource, is not new. A brand-new unit with no baseline is judged as today, and the planner documents the risk.
  - (b) a resource this run did NOT write, whose REST view changed from the baseline.
- A difference that already existed in the baseline (REST vs file, unwritten resource, unchanged) is a **warning**, never red. Each warning names the resource and the differing paths.
- A deleted unit: the REST view must show it absent (as today). *(See Open Question 1: read_back does not check this today.)*
- Rollback stays automatic on a real read_back failure. No flag skips verify. "no flag skips the plan hash, the verify or the rollback" stays true.

**Standalone `deploy verify` ("Warn, not red")**
- Without a baseline, read_back differences become **warnings** (`data.warnings`, and the check's findings with `status: warn`). Verify is green when scan, mode, files and loaded pass.
- `files` still catches real on-disk drift. *(True only for tag units; see Finding F2.)*
- The `read_back` check status becomes `warn` when there are only differences, and `fail` only for something real. The planner decides whether `resource_missing` stays red. Recommended: warn when the plan says the files are `same`.

**Contract**
- Additive only. No rename or removal. The four-place rule applies if a slug is added; prefer none.
- README: the verify checks table (read_back row), the apply section (baseline), the verdict-exception text if it changes.
- `PLAN_GOLDEN*` and `RENDER_GOLDEN` untouched. Any verify/apply envelope golden that changes is updated deliberately and explained.

**Out of scope:** modelling each module's REST normalisation; changing what `files`, `loaded`, `scan`, `mode`, `probes` judge; a Cargo.toml bump; any production write.

### Claude's Discretion
- Where the baseline lives in the run dir (for example `run_dir/read_back_baseline.json`), with no secrets: JSON paths and hashes only, never values.
- New warning code names (for example `read_back_preexisting`).
- Whether the baseline read happens before `lock_acquired`. Recommended: before.

### Deferred Ideas (OUT OF SCOPE)
None listed in CONTEXT.md.
</user_constraints>

## Project Constraints (from CLAUDE.md)
- No git worktrees. The shared `CARGO_TARGET_DIR` corrupts `ignition_core` artifacts. Run on the main checkout (`workflow.use_worktrees=false`).
- `ign` on PATH is the installed binary. Run `cargo install --path crates/ignition-cli --force` before any manual smoke check.
- `thiserror` plus stable slugs. The exit-code source of truth is the README table and `crates/ignition-core/src/error.rs`, pinned by `readme_exit_table_agreement` and `exit_code_mapping_enumerated`. No new slug is planned here.
- Keep the dependency tree lean: no new crate. `sha2` is already used (`tree::sha256_hex`, `deploy/tree.rs:36`).
- Work through GSD (`/gsd-quick`). Rust edition 2024, MSRV 1.85.

## Summary

`read_back` is a strict, stateless equality check of every declared **non-tag** resource's `config.json` against the REST effective view (`verify.rs:610-668`). Tag units are never read over REST: `Expected::from_render` skips them (`verify.rs:321-323`), and `files` judges them. In the production canary, the written unit was a `tag-definition`. It was judged by `files` (pass), and the 23 reds were all unwritten resources. Rule (b) alone would have kept that run green. The post-apply verify is the full `deploy_verify` action, called from `confirm()` (`apply/mod.rs:1486-1495`). It runs **after** the scan, which releases the lock (`hold.rs:138`, D12), so neither the post-apply read_back nor a pre-commit baseline uses any of the 176 s hold.

The REST sweep is cheap. `gather_read_back` (`actions/deploy.rs:1428-1471`) makes one list call per named type and one call per singleton. On production that is about 40 calls for about 146 non-tag resources. The 14 minutes of the 2026-10-30 verify (scan_done 21:54:38 to verify_done 22:08:47) are dominated by the full ssh tree read inside `deploy_verify` (`reader.read(&scope)`, `actions/deploy.rs:1105`), not by REST. The preflights took 15 minutes too (run_started 21:37:54 to backup_sampled 21:53:04), and they also read whole trees.

Two findings change the plan beyond CONTEXT. **(F1)** The backup's convergence uses the same strict read_back (`converge.rs:260-265`, `pair.rs:468-474`, `converged` at `pair.rs:514-522`). Once the master is fixed, every production pair apply would end `applied_backup_unsynced` (exit 6) on the same 23 resources. The backup needs the same baseline treatment. **(F2)** `files` checks tag units only (`verify.rs:837-888`). If standalone read_back turns to warn unconditionally, non-tag on-disk drift becomes invisible. Gate the warn on the plan status that `deploy_verify` already computes (`actions/deploy.rs:1106`).

**Primary recommendation:** Keep `read_back_check` as the strict comparison primitive. Expose its **uncapped** per-resource path sets. Add a pure `ReadBackJudge` layer with three variants: `Strict` (today's behaviour, the default of `evaluate`), `Standalone { file_status }`, and `Baseline { … }`. Take the master and backup REST baselines right after the batch spec is validated and before `prepare` (`apply/mod.rs:1229-1245`). Persist the baseline as `read_back_baseline.json` holding paths and hashes only.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary | Rationale |
|---|---|---|---|
| Path-set diff and judging (pure) | `deploy/verify.rs` | `deploy/pair.rs` (backup wrapper) | `evaluate` does no I/O. The judge stays pure and testable. |
| REST sweep (baseline and post) | `actions/deploy.rs::gather_read_back` | `pair.rs::BackupNode::read_back` | Already shared by master and backup. Reused unchanged. |
| Baseline timing and persistence | `deploy/apply/mod.rs::drive` | `apply/run_store.rs` (`RunDir::write_json`) | The engine owns the run order and the run dir. |
| Backup judging | `deploy/apply/converge.rs::content` | `pair.rs` | Convergence runs after finalize. It never rolls back the master. |

## Findings

### 1. How read_back works today
- **Expected set:** `Expected::from_render` (`verify.rs:303-383`). It skips tag types (`layout.rs:80-85`: `"tag-provider", "tag-group", "tag-type-definition", "tag-definition"`), skips `NO_READ_ROUTE` (`verify.rs:107-110`), and builds a `Compare` per resource: `Exact(config.json)`, `NameKeyedSubset` for `ignition/security-levels` (N2), or `Attributes` from `resource.json` (N3). The key is `module/type/name`, or `module/type` for a singleton (`resource_id`, `verify.rs:275-280`). This key equals the plan's `PlanEntry.key` for non-tag units.
- **REST route mapping:** `gather_read_back` (`actions/deploy.rs:1428-1471`). A singleton uses `resource_singleton(module, ty, None)`, and `NotFound` maps to `Missing`. A named type uses one `resource_list(module, ty, default)` per type and finds the item by name. Collection `system` maps to `Remote::System`. Any other error is `Err`, which ends the verify with no verdict.
- **Comparison:** `read_back_check` (`verify.rs:610-668`). `Missing` gives finding `resource_missing`. `System` gives a warning. `Config(rest)` runs `strip_nulls` on both sides (`verify.rs:673-684`, N1), then `diff_exact` (`722-749`), `diff_subset` (`753-796`) or `diff_attributes` (`803-817`). Paths are dotted. Containers or arrays of different length give the container path, and a type mismatch at the top gives `(root)`. N2 uses `[name]` segments. Keys pass through `printable()`. The detail is `"differs at: " + join_paths(paths)`, which sorts, dedups and **caps at 20** (`MAX_PATHS`, `verify.rs:116`; `join_paths` at `819-829`). The judge must use the raw `Vec<String>` from before `join_paths`, not the detail string.
- **Status:** `finished()` (`verify.rs:533-544`) returns `Fail` when there is any finding. `evaluate` (`477-501`) sets `ok = no Fail` and copies each `Warn` check's detail into `warnings`.
- **Post-apply call:** `confirm()` (`apply/mod.rs:1449-1518`) runs the lock check, `scan_and_wait`, then `deploy_verify(...)` (`1486-1495`). A red result emits `VerifyDone{ok:false}` and then `VerifyChecks{checks: (id,status)}` (`1497-1509`; fields at `610-615`, statuses only). It returns `Confirmed::Abort{VerifyFailed}`, and `drive` calls `roll_back` (`1317-1321`). A green result passes `verify.warnings` into `ApplyOutcome.warnings`.

### 2. Where to take the baseline; cost
`drive()` order (`apply/mod.rs:1056-1415`): `RunStarted` (1072), then preflights and `run.json`/`plan.json` (1092-1165), then the backup sample (1172-1179, fail-fast `?`), then `take_gwbk` (1187), then `restart_pending` (1192), then the ledger and `batch_spec` (1195-1233), then **[baseline here]**, then `prepare` and `Staged` (1245-1252), then `HeldLock::acquire` and `LockAcquired` (1253-1266), then commit (1287), then `confirm`.

**Recommendation:** take the baseline after `spec.problem()` passes and before `prepare`. A failed read then leaves nothing staged, so there is no `discard_interrupted` path, and none of the hold is spent. Use `?` fail-fast, like the backup sample. Do not take it under the lock: `LOCKED_SAMPLE_TIMEOUT` exists precisely because the hold has no allowance for reads.

**Cost:** about 40 REST GETs per node on production (35 non-tag types, many of them singletons; from the plan entries in the run dir). That is seconds. In the 14-minute verify, the expensive step is `reader.read(&scope)` over ssh across about 719 units, 573 of them tag units (`actions/deploy.rs:1105`). REST is minor. This attribution is MEDIUM confidence: the events carry no sub-step times. It is inferred from the equally slow tree-reading preflight window.

### 3. The rule: "new difference"
Inputs: the written key set W, made of plan entries with status `Added`/`Changed` whose key is a non-tag resource in `Expected.resources`. The tracer write set covers whole units (`write_set`, `apply/mod.rs:237-260`).

| Case | Baseline | Red when | Otherwise |
|---|---|---|---|
| Unwritten, baseline `Config(r0)` | B = diff(r0, declared file) | REST moved: `diff(strip(r0), strip(r1)) ≠ ∅` **and** P = diff(r1, file) ≠ ∅. Detail: `changed since the pre-apply read at: …` | P ≠ ∅: warning naming the paths. P = ∅: pass. |
| Unwritten, `Config` then `Missing` | | red `resource_missing` | |
| Unwritten, `Missing` then `Missing` (e.g. `edge-sync-settings`) | | | warning |
| Unwritten, `Missing` then `Config` | | red unless P = ∅ | |
| Written `Changed`, baseline `Config(r0)` | **B = diff(r0, live file before)**, the pre-apply effective live copy, NOT the desired file | `P − B ≠ ∅`, prefix-aware: p is old if B holds p or an ancestor of p (the next char after the ancestor is `.` or `[`; `(root)` covers everything) | warning listing the old paths |
| Written, post `Missing` | | red `resource_missing` | |
| Written `Added` (no baseline) | none | P ≠ ∅, as today (risk: a hand-written minimal config that the gateway fills with defaults rolls back; mitigation `deploy pull`, already in the README) | |

Why B is measured against the **live** file for written units: measured against the desired file, B would contain every path the apply changes. A write that never took effect would then look "pre-existing", and this run would go green. read_back is the only check that proves a written **non-tag** unit loaded: `files` covers tag units only, and `loaded` is best effort. Get the live bytes with one targeted read: `ctx.reader.read(&Scope{mode, types: <written non-tag (module,ty)>, tag_providers: vec![]})` (pattern at `preflight.rs:398-403`). Take `pair::effective_files` (`pair.rs:676-680`, mode over core), then build the `Compare` with the same code as `from_render`. Refactor `from_render`'s per-resource closure (`verify.rs:333-357`) into a shared `compare_for(key, module, ty, files)`. For the canary, W ∩ read_back is empty, so this read is skipped.

Residual masking risk: a written path the gateway *also* normalises (it is in B) and that did not take effect stays green. Document it; it is accepted.

**Backup (F1):** at the same point, call `backup.read_back(&expected)` (`pair.rs:132-136`, read-only), fail-fast like `backup.status()`. In `converge.rs::content` (`254-277`), judge with the same `Baseline` variant over the backup's own r0. For written units, use the master's B (the backup receives the master's files). The backup check stays `Warn` for pre-existing differences, so `converged` (`pair.rs:522`, `!= Fail`) holds. Standalone `verify --node backup|both` (`actions/deploy.rs:1347-1354`): pre-existing differences warn, because the backup's `files` check already compares every rendered path with the master's live copy byte for byte.

### 4. Standalone verify becomes warn (F2)
`deploy_verify` (`actions/deploy.rs:1085-1156`) already builds `planned` over the full manifest scope (1104-1106). Today it passes only the tag-unit statuses (1107-1112). Pass the non-tag statuses too, as `file_status: BTreeMap<key, PlanStatus>`. **Standalone rule:** a `resource_differs`/`resource_missing` for a key whose plan status is `Same` becomes a `Warn` finding (same code; no new code needed) plus one `data.warnings` line naming the key and paths. Any other status (`Changed`, `Added` = repo and live disagree) stays `Fail`. The check status is `Fail` if any finding is red, otherwise `Warn` if any finding is left, otherwise `Pass`.

Keep `verify::evaluate(expected, observed)` = `Strict`, which leaves the about 15 pure CV2 tests in `tests/deploy_verify.rs` unchanged. Add `evaluate_with(expected, observed, &ReadBackJudge)`.

Tests and docs that pin red behaviour, and what happens to each:

| Test / doc | Where | Effect |
|---|---|---|
| `verify_green_and_red_through_action` | `ignition-core/tests/deploy_verify.rs` (around 1259) | resource omitted from the fake and live files `same`: **flips to warn**. Make red by also drifting the live file (plan `changed`), and add a warn twin. |
| `verify_cli_red_exits_6` | `ignition-cli/tests/contract_deploy.rs:1813` | `omit_translator` with live `same` **flips to exit 0**. Fix: also edit the live translator config so it stays `resource_missing` red (asserts unchanged), plus a new `…_preexisting_warns_exit_0`. |
| `verify_cli_human_lines` | `contract_deploy.rs:1994` (asserts `read_back: fail`, `resource_missing … POSTGRES` at 2050-2056) | same fix |
| `pull_then_verify_is_green_live` step (d) | `live_deploy_plan.rs:615-651` | repo-side edit, so the plan says `changed` and it **stays red**. No change. |
| `each_check_fails_alone`, `read_back_*` (CV2) | `deploy_verify.rs:134-600` | use `evaluate` = Strict. No change. |
| `the_mode_and_read_back_checks_reuse_the_verify_evaluation` | `pair/tests.rs:395` | strict wrapper kept. No change. |
| `backup_unsynced_content_differs` | `deploy_apply.rs:5915-5941` | written unit diverged on the backup. Must **stay red** under rule (a); run it to confirm. |
| `apply_happy_path_local` | `deploy_apply.rs:685-695` (run-dir `names` list) | add `read_back_baseline.json` |
| `auto_rollback_verify_red_restores` | `deploy_apply.rs:1493-1556` | event order unchanged. `verify_checks` gains findings, and the existing assertions still hold. |
| README | `:3349` read_back row; `:3514` apply step 9; `:3594-3606` run-dir contents and the secrets list; `:102-134` verdict exception (green with read_back `warn` is still exit 0; the text needs only a sentence) | |

Four-place rule: **no new slug and no new finding code**. The finding codes stay `resource_missing`/`resource_differs`, and the distinction lives in the detail and the status. Warnings are plain strings.

### 5. Test strategy (offline)
The harness is the `Rig` in `tests/deploy_apply.rs:205-245`: a `PlanFixture` over a local data dir, `LocalFsTransport` behind `FaultyHost`, `FakeGateway::with_data_dir` (REST is reloaded from the mode collection's `config.json` files at every config scan: `common/fake_deploy.rs:207-242`), and wiremock for logs and preflights. `changing_rig()` (432-456) writes `DB_UNIT` (`ignition/database-translator/POSTGRES`, a non-tag unit, so rule (a) is exercised) and leaves the other declared resources `same`.

Add one knob to `FakeGateway`: `rest_overlay(key, member, Value)`, merged into `config` inside `reload_from_disk`. That is the gateway "filling a default". Use it to build the CONTEXT scenarios:
1. Overlay an unwritten singleton/resource before `prepare`. Expect `applied`, plus a warning naming the key and member.
2. Arm a second overlay on an unwritten resource in a `host.after("commit", 1, …)` hook (the pattern at `deploy_apply.rs:1349-1356`). The scan reloads, REST moves, and the run must end `rolled_back` / `verify_failed`. `verify_checks` carries the finding subject.
3. Overlay `DB_UNIT` before the run (it is in B against the live file), and keep it after the scan. Expect `applied`.
Also add a pair variant (`with_pair`, around 5915) where the backup carries an overlay: expect `applied`, not `applied_backup_unsynced`. And pure tests for the judge, including the prefix rule and a write that did not take effect (stays red).

**Live:** `tests/live_deploy_apply.rs` is gated by `IGNITION_LIVE_DEPLOY=1` with fixture `tests/fixtures/live-rig-deploy/compose.yml` (8.3.6). Add an `#[ignore]` test that seeds a *minimal* `config.json` for a non-tag tracer resource in the mode collection, scans, declares the same minimal file, and applies an unrelated change. On a stock rig, which member the gateway re-fills is **unknown**. The executor finds it by dropping one member and reading REST back (README: "a hand-written, minimal config.json can differ from REST in members the gateway fills with defaults"). Run the existing five gates as a regression, `live_queued_config_save_during_apply` included (`:1394`; a foreign save on a written unit must still roll back or be refused).

### 6. Pitfalls
- **Secrets:** REST `config` carries `{"type":"Embedded","data":{ciphertext…}}` (22-LIVE-CAPTURES.md:1859). Keep r0 in memory only. `read_back_baseline.json` holds `{schema_version, node, resources:{key:{state:"config"|"missing"|"system", rest_sha256, differs_at:[paths]}}}`, where `rest_sha256` is SHA-256 over canonical (sorted-key, null-stripped) JSON. Never store per-path value hashes: a short secret is brute-forceable. The existing canary walker `run_dir_canary_leaks` (`deploy_apply.rs:5369-5395`) scans every non-exempt run file, so it covers the new file automatically.
- **Lock hold:** the baseline is taken outside the lock (Finding 2). Do not put REST or tree reads between `LockAcquired` and the commit.
- **Window false-red:** an operator change or a pending, unscanned on-disk edit between the baseline and the post-scan counts as rule (b). The "red only if P ≠ ∅" clause lets a change *toward* the declared file pass. A REST field that changes on every read would false-red. None was seen (all 22 production differences were stable defaults or normalisation), but this is ASSUMED.
- **Path cap:** never derive sets from `Finding.detail` (capped at 20, joined into a string).
- **`plan_sha256`:** hashes plan entries and file hashes only (`plan.rs:112-137`). The baseline must not go into `plan.json` or the plan. Goldens are untouched.
- **Run-dir listing and events:** a new *file* changes one pinned listing. A new *event* would change about six pinned event arrays (`LAPSED_EVENTS` `:1383`, `:738-750`, `:1512-1528`, …). Prefer the file, and enrich the existing `verify_checks` event with `findings:[{code,subject,detail}]` (value-free by construction, `verify.rs:17-23`).
- **`edge-sync-settings`-style `Missing` on both sides:** a warning, not red, in both modes when the plan says `same`.
- **Deleted units (Open Question 1).**

## Package Legitimacy Audit
No external packages are installed in this phase. **Removed [SLOP]:** none. **Flagged [SUS]:** none.

## Environment Availability
| Dependency | Required by | Available | Fallback |
|---|---|---|---|
| cargo | build and tests | yes (`/opt/homebrew/bin/cargo`) | none |
| docker | optional live rig check | yes | skip the live test; offline scenarios are the gate |
| live rig env (`IGNITION_LIVE_URL`, `IGNITION_LIVE_TOKEN`, `IGN_DEPLOY_RIG_DATA`, `IGN_DEPLOY_RIG_MODE`) | live test | not checked | the test skips quietly when `IGNITION_LIVE_DEPLOY` is not `1` |

## Validation Architecture
| Property | Value |
|---|---|
| Framework | cargo test (tokio, wiremock, assert_cmd), existing |
| Quick run | `cargo test -p ignition-core --test deploy_verify --test deploy_apply` |
| Full suite | `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (plus the `cargo xwin clippy` windows-msvc gate used by the prior quick tasks) |
| Live (optional) | `IGNITION_LIVE_DEPLOY=1 cargo test -p ignition-core --test live_deploy_apply -- --ignored --test-threads=1` |

| Behaviour | Test type | Where |
|---|---|---|
| Judge rules (unwritten moved/unmoved, written prefix rule, Added strict, Missing transitions) | unit | new `tests/deploy_verify.rs` cases |
| Standalone warn when the plan says `same`, red when `changed` | action plus CLI contract | `deploy_verify.rs`, `contract_deploy.rs` |
| CONTEXT scenarios 1-3 plus the pair variant | engine (fake gateway) | `deploy_apply.rs` (Wave 0: `FakeGateway::rest_overlay`) |
| Baseline file is value-free | canary | existing `canary_*` tests (automatic) |

## Security Domain
| ASVS | Applies | Control |
|---|---|---|
| V5 Input validation | yes | gateway keys go through `printable()` (`verify.rs:1036-1046`) in every path |
| V6 Crypto | yes (hash only) | `sha2` via `tree::sha256_hex`; no hand-rolled hashing |
| V8 Data protection | yes | no values in the baseline, events or envelope; run dir 0700/0600 (README `:3594-3606`) |
| V2/V3/V4 | no | unchanged auth path |

Threat: a fail-open verify, where a real failure becomes a warning. Mitigated by the written-unit B taken against the **live** file, by keeping `Strict` for `Added` units, and by keeping standalone red when the plan is not `same`.

## Assumptions Log
| # | Claim | Risk if wrong |
|---|---|---|
| A1 | The production REST `config` of an unwritten resource is stable across a config scan (no volatile fields) | rule (b) false-reds and every production apply rolls back again. Mitigation: before the canary re-run, run read-only `deploy verify` twice on production and compare the finding sets. |
| A2 | The 14-minute verify is dominated by the ssh tree read | low. It only matters for the timing claims. |
| A3 | The backup's REST view has the same 23 differences as the master | if not, F1 is a smaller problem. The fix is the same. |
| A4 | A stock 8.3.6 rig has a resource whose REST fills a default the file omits | the live test needs a different seed. Offline tests remain the gate. |

## Open Questions (RESOLVED)

_Resolved by the orchestrator: deletions per R3 (read_back never checked deletions; no check is added); wording per plan P4._
1. **Deleted units.** CONTEXT says "REST must show it absent (as today)". read_back does **not** check deletions today: `Expected` holds rendered resources only (`verify.rs:320-368`), and `files` covers tag units only. Recommendation: keep today's behaviour (no new check) and correct the CONTEXT wording. Optionally, an additive check for a `Removed` non-tag entry whose `live_in == ["mode"]` (a core copy keeps it live legitimately). Confirm with the user.
2. **Warning wording.** Proposed: `read_back: <key> differs from its file at: <paths> (pre-existing; not changed by this run)`, and on the standalone path `read_back: <key> differs from its file at: <paths> (files are the same; REST normalisation)`. Planner's discretion; no slug.

## Sources
- Read this session: `crates/ignition-core/src/deploy/verify.rs` (full), `actions/deploy.rs:995-1556`, `deploy/apply/mod.rs:1-90, 400-1560`, `apply/converge.rs:1-80, 180-351`, `apply/hold.rs:100-170`, `apply/preflight.rs:60-120, 380-420`, `deploy/pair.rs:400-500, 514-523`, `deploy/plan.rs:108-165, 170-265`, `layout.rs:80-85`, `actions/deploy_apply.rs:150-360`, `tests/common/fake_deploy.rs:1-60, 200-640`, `tests/deploy_apply.rs` (cited ranges), `tests/deploy_verify.rs:40-205, 1255-1330, 2030-2075`, `tests/live_deploy_plan.rs:560-651`, `tests/live_deploy_apply.rs:220-260, 1250-1400, 1690-1735`, `ignition-cli/tests/contract_deploy.rs:1700-1730`, README `:100-136, 3335-3460, 3465-3612`.
- Production run dir `…/wignvm01/20261030T213754Z-28f01c/` (`events.ndjson`, `result.json`, `run.json`, and `plan.json` summary only). The `.gwbk` and the snapshot were not opened.

## Metadata
Valid until the next change to `verify.rs` or `apply/mod.rs`. No external research was needed: the work is internal to the codebase, and no packages are added.
