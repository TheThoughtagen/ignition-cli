---
phase: quick-261031-bbs
plan: 01
subsystem: deploy (verify read_back, apply post-verify, pair convergence)
tags: [deploy, verify, read_back, apply, rollback, redundancy, contract, live-rig]
status: complete
requires:
  - v1.6.2 deploy apply / verify / pair (Phases 22-24, quick 261030-8m8)
provides:
  - read_back judged against a pre-apply REST baseline (master and backup)
  - standalone verify warns on plan-same REST differences (master and backup)
  - read_back_baseline.json and finding subjects in the verify_checks event
  - optional per-finding status ("warn") in the verify envelope
affects:
  - ign deploy apply (post-apply verify, rollback trigger, backup convergence)
  - ign deploy verify (--node master|backup|both)
  - README deploy verify / deploy apply sections and the verify verdict exception
tech-stack:
  added: []
  patterns:
    - pure judge enum (ReadBackJudge) selected by caller context (VerifyContext)
    - value-free run-record evidence (states, bases and paths only; the hash was dropped by IN-01)
key-files:
  created: []
  modified:
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
decisions:
  - "Post-apply read_back is red only for what the run changed or disturbed; a pre-existing REST/file difference is a warn finding (D-01, R4)"
  - "Standalone verify warns on a read_back difference or resource_missing only when the plan status is same; changed, added or no status stays red (R2, P10)"
  - "The backup is judged against its own pre-write REST baseline (R1, P8)"
  - "Finding gains an optional status serialized only as warn; no slug and no finding code added (P2, D-03)"
  - "read_back_baseline.json holds state, basis and uncapped differing paths only, never values (R5, P5; the whole-config SHA-256 was removed by review fix IN-01)"
  - "A written changed unit whose desired file differs from its live file but whose REST view did not move is red (amendment A1)"
  - "A1 is also judged per written path: a path the write changes that still differs, with no REST movement at it, an ancestor or a descendant, is red (review fix CR-01)"
  - "After an apply, a system-collection effective copy of a written unit, or of an unwritten unit that was file-backed before, is red resource_missing (review fix WR-01)"
  - "A written path REST shows neither before nor after (or a null REST config) is left out of A1 and the per-path rule and reported as a warn finding: not visible in the gateway's REST view, so the write could not be verified (review round 2)"
  - "A written path at which the pre-apply REST view already showed the desired value is left out of A1 and the per-path rule (review round 2)"
  - "read_back_baseline.json also records writes_at, paths only (review round 2)"
metrics:
  duration: "about 77 min across the three tasks (first commit 14:43Z, live gate done 15:54Z, gate and SUMMARY 16:05Z)"
  completed: 2026-10-31
estimate:
  tokens: 420000
  tasks: 3
actuals:
  tokens: 33759
  tasks: 3
  commits: 6
---

# Phase quick-261031-bbs Plan 01: deploy read_back judges only what the apply changed Summary

`ign deploy apply` now records a value-free REST baseline of every declared resource before it writes, and judges the post-apply `read_back` against it. A REST view that already differed from its file (filled defaults, module normalisation, the 23 production resources) is a warning, not a rollback. A disturbed unwritten resource, a written unit with a new difference, or a write that did not take effect still rolls back. The backup gets its own baseline. Standalone `ign deploy verify` warns only when the plan says the resource is `same`.

## Commits

| Task | Commit | Message |
|------|--------|---------|
| 1 RED | 6abc846 | test(261031-bbs): read_back judge and baseline scenarios (RED) |
| 1 GREEN (tracer) | b8a4e37 | feat(261031-bbs): read_back judged against a pre-apply REST baseline (tracer) |
| 2 RED | 8553f2f | test(261031-bbs): written-unit basis, backup baseline, baseline file (RED) |
| 2 GREEN | 39ae5f9 | feat(261031-bbs): written-unit live basis, backup baseline, read_back_baseline.json, verify_checks findings |
| 3 RED | 4ebc51f | test(261031-bbs): standalone read_back warns on plan-same (RED) |
| 3 GREEN | 41301d8 | feat(261031-bbs): standalone read_back warns on plan-same; README; live rig gate |

None of the commits has a co-author line. Every file was staged by explicit path. `.planning/config.json`, `.planning/research/.cache/` and `svc-ignition-deploy@52.154.75ace-uat4.ps1` were never staged. This SUMMARY is not committed, per the orchestrator.

## RED evidence (from git log and the Task 3 run)

- **Task 1 (6abc846).** The new public items (`ReadBackJudge`, `ReadBackBaseline`, `Written`, `evaluate_with`, `Finding.status`) landed with strict stub behaviour, so the `rb_judge_*` warn cases and `rb_apply_preexisting_unwritten_difference_warns` failed on assertions. b8a4e37 turned them green.
- **Task 2 (8553f2f).** `BaselineRecord` was a stub that returned no resources. `rb_judge_record_value_free`, `rb_apply_written_preexisting_difference_passes`, `rb_apply_written_new_difference_rolls_back`, `rb_apply_written_not_taken_effect_rolls_back` (A1), `rb_apply_pair_backup_preexisting_difference_converges`, `rb_apply_baseline_file_value_free` and the extended `rb_apply_unwritten_moved_rolls_back` failed. 39ae5f9 turned them green.
- **Task 3 (4ebc51f).** Measured before GREEN:
  - `deploy_verify`: `test result: FAILED. 87 passed; 3 failed`. The failures were `rb_verify_standalone_same_warns_through_action`, `rb_verify_backup_same_warns` and `verify_green_and_red_through_action` (its new warn half).
  - `contract_deploy verify_cli`: `test result: FAILED. 16 passed; 1 failed`. The failure was `verify_cli_preexisting_warns_exit_0`.
  - `rb_verify_standalone_changed_stays_red_through_action` and `rb_verify_backup_changed_stays_red` passed during RED. They are guards on behaviour that must not change, not new behaviour.

## Tests

- `deploy_verify rb_judge`: 18 passed. This includes `rb_judge_prefix_rule`, `rb_judge_write_not_taken_effect_root_covered_fails` (A1), `rb_judge_uncapped_paths`, `rb_judge_record_value_free` and the standalone judge cases.
- `deploy_verify rb_verify`: 4 passed: the standalone same-warns and changed-stays-red cases through the action, and the same pair on `--node both`.
- `deploy_apply rb_apply`: 8 passed: scenarios 1, 2 and 3, the written-new-path rollback, the A1 write-not-taken-effect rollback, the pair variant, the baseline-read failure and the value-free baseline file.
- `backup_unsynced_content_differs`: still `applied_backup_unsynced` / `content_differs` (1 passed).
- `contract_deploy verify_cli`: 17 passed. `verify_cli_red_exits_6` and `verify_cli_human_lines` now drift the live translator so the plan says `changed`; their assertions are unchanged. The new `verify_cli_preexisting_warns_exit_0` checks exit 0, `data.ok` true, read_back `warn` with its detail, the finding `"status": "warn"`, the standalone warning line, and the human line ending ` (warn)`.
- Goldens: `PLAN_GOLDEN`, `PLAN_GOLDEN_LEDGER`, `PLAN_GOLDEN_UNREADABLE` (`deploy_plan.rs`, untouched since the merge base) and `RENDER_GOLDEN` (`7c8b41b1…` in `contract_deploy.rs`) are byte-identical.

## Full gate (after 41301d8)

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0, no warnings |
| `PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | exit 0, no warnings |
| `cargo test --workspace` | 96 result lines: 3061 passed, 0 failed, 92 ignored (the new live test is the extra ignored one) |
| `git diff --quiet $(git merge-base HEAD main) -- Cargo.toml Cargo.lock crates/*/Cargo.toml` | no change |

## Orchestrator resolutions and planner decisions, as implemented

- **R1:** the backup's `read_back` in `converge.rs` is judged by `pair::read_back_backup_check_with(ReadBackJudge::Baseline(backup baseline))`. That baseline is read from the backup before the master write.
- **R2:** in standalone verify, a difference or `resource_missing` warns only on plan `same`. This holds for the master (`deploy_verify_planned` → `ReadBackJudge::Standalone`) and for the backup:
  - `--node both` uses the master verify's statuses.
  - `--node backup` uses statuses from the master read that `verify_backup_files` already makes. There is no extra tree read.
- **R3 (CONTEXT correction):** CONTEXT said "A deleted unit: the REST view must show it absent (as today)". That is not how today works: `read_back` never checked deletions, because `Expected` holds rendered resources only. No deletion check was added. Deletions stay judged by `files` (tag units), the plan and the ledger. README says so in the baseline rules.
- **R4:** the baseline is REST against the LIVE file before the apply.
  - An unwritten resource is red only when it moved and still differs.
  - A written `changed` resource is red on a path that the baseline set does not cover (an ancestor counts).
  - An `added` resource is strict.
  - Path sets are uncapped.
  - Amendment A1 adds one more red case: the desired file differs from the live file but REST did not move.
- **R5:** `read_back_baseline.json` holds the state, a whole-config SHA-256, the basis and the differing paths. It never holds a value. (Review fix IN-01 later removed the SHA-256.) The `verify_checks` event carries `findings: [{code, subject, detail[, status]}]`.
- **R6:** the offline scenarios use `FakeGateway::rest_overlay`. The live gate ran on the Docker rig only (below).
- **R7:** see the production stability evidence below.
- **P1:** the names are as planned: `ReadBackJudge`, `ReadBackBaseline`, `BaselineBasis`, `Written`, `evaluate_with`, `read_back_check_with`, `compare_for`, `read_back_backup_check_with`, `VerifyContext`, `deploy_verify_judged` and `FakeGateway::rest_overlay`. `verify::path_covered` is public too, for the prefix-rule test.
- **P2:** `Finding.status: Option<CheckStatus>`, skipped when `None`.
- **P3:** check status is `Fail` if any finding counts, else `Warn` if any finding is left, else `Pass`. A `Warn` detail ends `; <w> difference(s) reported as warnings`.
- **P4:** the four warning wordings are as specified. They are pinned by `rb_judge_*`, `rb_verify_*` and the CLI test.
- **P5:** the baseline file schema is `schema_version` 1. The hash is computed over an explicit recursive key sort with nulls stripped. The file is written 0600 through `RunDir::write_json`. (Review fix IN-01 later removed the hash.)
- **P6:** the baseline is read after `spec.problem()` and before `prepare`, master first and then the backup. It sits outside the lock, and a read error is fail-fast. The pinned event orders are unchanged.
- **P7:** `verify_checks` has per-check `findings`. It is still emitted only on a red verdict.
- **P8:** the backup's written basis is the backup's own r0 against the master's live-file compare.
- **P9:** ` (warn)` is appended to a warn finding's line in both the master and the backup human renderers.
- **P10:** with no plan status, judging stays strict.

## Live

All live work ran on the local Docker Ignition 8.3.6 rig (`ign-deploy-rig`, OrbStack, `http://localhost:18290`). There were no production calls of any kind. Env values are named here, never printed: `IGNITION_LIVE_TOKEN` = `$IGN_TOKEN_CAP_A` from `~/.cache/ign-deploy-rig/tokens.env`, `IGN_DEPLOY_RIG_DATA` = `$HOME/.cache/ign-deploy-rig/data`, `IGN_DEPLOY_RIG_MODE` = `production`, `IGNITION_LIVE_DEPLOY` = `1`, `IGNITION_LIVE_URL` = `http://localhost:18290`.

1. Before 2026-10-31T15:45:40Z: `cargo install --path crates/ignition-cli --force`. It replaced `~/.cargo/bin/ign`, which reports `ign 1.6.0` because there is no Cargo.toml bump.
2. 2026-10-31T15:45:40Z: `bash capture22.sh up22 && bash capture22.sh setup22` (in `crates/ignition-core/tests/fixtures/live-rig-deploy`). Output: `RUNNING, deploymentMode production` at 15:48:06Z.
3. Discovery. A tree pulled from the stock rig verifies with `read_back` `pass`. `pull_then_verify_is_green_live` pins this, and its 15:53Z run shows `pulled: ok true … resources_checked 28` and `ReadBack Pass []`. The stock rig therefore has no normalised resource, so I seeded one.
   - The probe ran 15:48-15:50Z. I wrote `production/ignition/general-alarm-settings/config.json` without `liveEventLimit`, then ran one scan.
   - REST `/resources/singleton/ignition/general-alarm-settings` answered from collection `production` with `liveEventLimit` filled (5), and the file was left unchanged.
   - The probe seed was then removed and the rig rescanned.
4. 2026-10-31T15:50:40Z: `cargo test -p ignition-core --test live_deploy_apply live_preexisting -- --ignored --nocapture --test-threads=1`.
   - The apply ended `Applied`, cause `None`, with the warning `read_back: ignition/general-alarm-settings differs from its file at: liveEventLimit (already different before this apply)`.
   - The standalone verify gave `read_back: warn` and the finding `resource_differs ignition/general-alarm-settings … Some(Warn)`.
   - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 15.06s`
5. 2026-10-31T15:51:14Z: `bash capture-cd5.sh seed` (exit 0).
6. 2026-10-31T15:51:44Z: `cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1`. It ran these tests, all PASS:
   - `cd5_apply_root_and_nested_live`
   - `live_apply_then_rollback`
   - `live_kill_after_green_verify_then_recover`
   - `live_kill_mid_write_then_recover`
   - `live_lock_lapse_restores`
   - `live_preexisting_rest_difference_warns`
   - `live_queued_config_save_during_apply` (`applied_write_refused`)

   `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 111.36s`
7. 2026-10-31T15:53:43Z: `cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture --test-threads=1`.
   - `pull_then_verify_is_green_live` step (d) stayed red: `edited: ok false … ReadBack Fail [resource_differs:ignition/cobranding (differs at: appIcon)]`, because the plan says `changed`.
   - `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 22.59s`
8. 2026-10-31T15:54:23Z: `bash capture-cd5.sh clean`, then at 15:54:35Z `IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/data/config/resources docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml down -v`. The container, the `ign-deploy-rig_gw-data` volume and the network were removed, and none remain.

The live resource was **seeded, not found**: `ignition/general-alarm-settings`, member `liveEventLimit`. This is recorded in the doc comment of `live_preexisting_rest_difference_warns`.

Live verdict: pass

## R7: production stability evidence (assumption A1)

The orchestrator ran two read-only production `ign deploy verify` calls 17 hours apart. They gave **identical 23-line `read_back` findings**: the same 22 `resource_differs` and 1 `resource_missing` (historian providers, OPC UA, Cirrus Link and core settings). Those REST views did not move between reads, so rule (b), "an unwritten resource that moved and still differs is red", should not false-red on production's volatile-free settings. This evidence came from the orchestrator. I made no production call.

## Residual risks (accepted, documented in README)

- **Masking (narrowed by CR-01).** Before the review fix, a written path covered by a pre-existing difference was masked whenever anything else in the resource moved. A1 is now judged per written path, so only a written path whose REST view did move, to a value that still differs in the pre-existing way, is a warning.
- **Window.** A foreign change to an unwritten resource between the baseline (just before staging) and the post-scan read counts as this run's disturbance and rolls back. The baseline is read late to keep the window short.
- **Strict `added`.** A brand-new unit is judged strictly. A hand-written minimal `config.json` for a new resource can false-red on filled defaults. Mitigation: `deploy pull`, which carries the full config.
- **Assumption A1** (production REST views are stable across a scan) is supported by the R7 evidence. It is re-checked by the canary re-run.

## Deviations from Plan

### Task 1

- **[Amendment A1, binding]** Added `rb_judge_write_not_taken_effect_root_covered_fails` and the A1 rule in the `LiveFile` basis.
- `verify::path_covered` is public so the prefix-rule test can call it directly.

### Task 2

- **[Rule 3 - Blocking]** `crates/ignition-cli/src/render.rs` was outside Task 2's file list. `describe_apply_event` was changed to match `(id, status, _)` after `ApplyEvent::VerifyChecks` gained findings (39ae5f9). It is a compile fix, and the human output is unchanged.
- **[Amendment A1, binding]** Added `rb_apply_written_not_taken_effect_rolls_back`, the apply-level A1 case.
- Pre-existing and out of scope: `cargo doc -D warnings` already fails on this branch because of unrelated intra-doc links. I left it alone.

### Task 3

1. **SUMMARY not committed.** Plan step 7 stages the SUMMARY in the GREEN commit. Per the orchestrator's hard rule it is written but not committed, and STATE.md is not updated.
2. **The read_back summary line.** A prior note said a `warn` read_back adds `read_back: <n> declared resource(s) compared …; <w> difference(s) reported as warnings` to `warnings`. I checked:
   - It is the check's `detail` (P3) in both standalone verify and apply.
   - The apply engine also carries non-pass check details into the apply's `data.warnings` as `read_back: <detail>`. The live run shows this.
   - Standalone verify does not add it to `data.warnings`.
   - README and the CLI test say exactly this. The CLI test asserts the detail.
3. **The backup check order.** `verify_backup_tick` now runs the backup `files` check before `read_back`, so that `--node backup` can take plan statuses from the master read the files check already makes. The report order `[mode, read_back, files]` and the warning order (read-back, then files) are preserved. No extra tree read was added.
4. **Extra guard tests** beyond the listed behaviour: `rb_verify_backup_changed_stays_red`, and a `named` variant inside `rb_verify_standalone_changed_stays_red_through_action` (an overlay difference as well as a missing resource).
5. **Discovery.** There was no separate pull-and-verify discovery step. The pinned `pull_then_verify_is_green_live` already shows a pulled stock tree comparing `pass` on all 28 resources, and its 15:53Z run confirmed it. I went straight to seeding (step 3 of the Live section).
6. **[Rule 1 - Side effect]** `capture-cd5.sh clean` appended 190 capture lines to `.planning/quick/261030-8m8-lift-cd5-root-level-tag-and-nested-udt-f/261030-8m8-LIVE-CAPTURES.md`, which is its default `OUT`. That file belongs to an earlier task, so I restored it with a path-specific `git checkout --`. It is not part of any commit.
7. **The teardown command.** The plan has `docker compose -p ign-deploy-rig down -v`. I also passed `-f compose.yml -f compose.bind.yml`, with `IGN_DEPLOY_RIG_RESOURCES` set, because `compose.bind.yml` requires the variable.

## Known Stubs

None. No placeholder, TODO, `todo!`, `unimplemented!`, `.skip` or `.only` was added. The single `#[ignore]` is the standard live-gate attribute, and the test returns a quiet green skip without `IGNITION_LIVE_DEPLOY=1`.

## Threat Flags

None beyond the plan's threat model. The new run-dir artifact `read_back_baseline.json` and the `verify_checks` findings are value-free and covered by T-bbs-02. `run_dir_canary_leaks` and `rb_apply_baseline_file_value_free` pass.

## Hand-off (not done here)

1. **Tag v1.6.4 after merge (v1.6.3 was mis-tagged on the v1.6.2 commit and yanked).** After the PR from `fix/deploy-read-back-scope` merges, tag v1.6.4 on `origin/main` per RELEASING.md. release.yml stamps the version, so there is no Cargo.toml bump.
2. **Re-run the canary.** Install the released v1.6.4 binary. Then, in `whk-onprem-infra-iac` on the `ignition-deploy-canary` branch, re-plan and re-apply the canary against WIGNVM01/WIGNVM04. Its `.gitignore` already has `**/.ign/`. Before the apply, take fresh backups (a VM snapshot and a `.gwbk` of both prod nodes) and get the user's explicit go-ahead. The expected outcome is `applied`, with one `read_back` warning per pre-existing difference (the 23 resources).
3. **Check WIGNVM04.** Look into WIGNVM04 `projectState: SyncError` before or alongside the canary. It can affect backup convergence (`applied_backup_unsynced`).
4. **Keychain prompt.** Approve the macOS keychain prompt for `profile:prod-backup` when the new binary first reads it.

## Self-Check: PASSED

- Files: every key file listed exists on disk. `crates/ignition-core/tests/live_deploy_apply.rs` contains `live_preexisting_rest_difference_warns`. README contains `read_back_baseline.json`. `actions/deploy.rs` contains `ReadBackJudge::Standalone`.
- Commits: 6abc846, b8a4e37, 8553f2f, 39ae5f9, 4ebc51f and 41301d8 are all on `fix/deploy-read-back-scope`.

## Code review fixes (261031-bbs-REVIEW.md)

Applied on `fix/deploy-read-back-scope`, main checkout, no worktree. Each code fix had its failing test first; REVIEW-FIX.md has the detail.

| Finding | Commit | Message |
|---------|--------|---------|
| CR-01 | 6005f00 | fix(261031-bbs): CR-01 judge A1 per written path |
| WR-01 | 887191e | fix(261031-bbs): WR-01 judge a post-apply system copy against the baseline |
| IN-01 | 76752d7 | fix(261031-bbs): IN-01 drop rest_sha256 from read_back_baseline.json |
| WR-02 | e2c459c | fix(261031-bbs): WR-02 README states read_back does not verify deletions |
| WR-03 | bdc5b71 | fix(261031-bbs): WR-03 README rationale for the standalone read_back warning |
| WR-04 | 648a614 | fix(261031-bbs): WR-04 README documents the strict judge for a changed unit missing before the apply |
| IN-07 | df01c31 | fix(261031-bbs): IN-07 README names both backup strict fallbacks and the baseline-read error |

IN-02 to IN-06 were not fixed, per the orchestrator (reasons in REVIEW-FIX.md). No commit has a co-author line. Every file was staged by explicit path, and `.planning/config.json`, `.planning/research/.cache/` and `svc-ignition-deploy@52.154.75ace-uat4.ps1` were never staged.

RED evidence: `rb_judge_partial_write_covered_path_fails`, `rb_judge_partial_write_root_covered_fails` and `rb_judge_written_now_system_fails` failed on their `!report.ok` assertions before the fixes (`19 passed; 3 failed`). `rb_judge_record_value_free` and `rb_apply_baseline_file_value_free` failed on the absent-hash assertions before IN-01. `rb_judge_written_within_live_baseline_warns` went red under CR-01, because its old scenario was the masked partial write. Its scenario now has the write take effect, and a pre-existing member is left as the warning.

### Full gate (after df01c31)

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0, no warnings |
| `PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | exit 0, no warnings |
| `cargo test --workspace` | 96 result lines: 3065 passed, 0 failed, 92 ignored (the 4 new judge tests account for the difference from 3061) |
| `git diff --quiet $(git merge-base HEAD main) -- Cargo.toml Cargo.lock crates/*/Cargo.toml` | no change |
| Goldens | `deploy_plan.rs` and `contract_deploy.rs` are untouched since 41301d8, so `PLAN_GOLDEN*` and `RENDER_GOLDEN` are byte-identical |

### Live rig re-run (Docker only, no production call)

Env as in the Live section above. `CAPTURE_OUT` pointed at a scratch file, so neither capture script wrote into a tracked LIVE-CAPTURES file, and no restore was needed.

1. 2026-10-31T16:48:30Z: `cargo install --path crates/ignition-cli --force` replaced `~/.cargo/bin/ign` (`ign 1.6.0`).
2. 16:48:36Z: `capture22.sh up22 && capture22.sh setup22` gave `RUNNING, deploymentMode production` (16:51:00Z).
3. 16:51:05Z: `capture-cd5.sh seed` exited 0.
4. 16:51:33Z: `cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1` ran these, all PASS:
   - `cd5_apply_root_and_nested_live`
   - `live_apply_then_rollback`
   - `live_kill_after_green_verify_then_recover`
   - `live_kill_mid_write_then_recover`
   - `live_lock_lapse_restores`
   - `live_preexisting_rest_difference_warns` (`Applied`, with the warning `read_back: ignition/general-alarm-settings differs from its file at: liveEventLimit (already different before this apply)`)
   - `live_queued_config_save_during_apply` (`applied_write_refused`)

   `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 112.43s`
5. 16:53:37Z: `cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture --test-threads=1`. `pull_then_verify_is_green_live` gave `pulled … ReadBack Pass []`, then `edited … ReadBack Fail [resource_differs:ignition/cobranding (differs at: appIcon)]`. `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 22.57s`
6. 16:54:10Z: `capture-cd5.sh clean` refused (`no <CAPTURE_OUT>`, exit 1): it requires the capture file to exist, and the redirected one was never created. Then `IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/data/config/resources docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml down -v` removed the volume `ign-deploy-rig_gw-data` and the network. No container, volume or network named `ign-deploy-rig` remains. The CD5 seed files are left in the host bind directory `~/.cache/ign-deploy-rig/data`, which is outside the repo; the next `up22` deletes that directory first.

Live verdict: pass

## Code review fixes, round 2 (second review of CR-01 and WR-01)

The second review returned REQUEST CHANGES on CR-01; WR-01 was fine. Production shapes (value-free) showed that CR-01 false-redded every legitimate edit to a member REST does not expose:
- OPC UA `Chiller1` `settings.deviceSettings`: a string in the file, absent in REST.
- Historian `SparkDB` `settings.profile`: a string in the file, absent in REST.
- Cirrus `namespace-server-set/1`: REST `config` is null.

The work ran on the same branch, in the main checkout, with no worktree. Each code fix had its failing test first. REVIEW-FIX.md "Round 2" has the detail.

| Item | Commit | Message |
|------|--------|---------|
| UNOBSERVABLE | 58fe621 | fix(261031-bbs): unobservable written paths warn instead of failing read_back |
| ALREADY-SHOWN | 451c163 | fix(261031-bbs): already-shown written paths are not judged as untaken writes |
| Review tests | eeeb610 | fix(261031-bbs): pin the review's read_back cases and the collapse residuals |
| `writes_at` record | 7c04158 | fix(261031-bbs): record writes_at in read_back_baseline.json |
| README | 6bc4ae3 | fix(261031-bbs): README limits read_back's per-path claim and documents what REST cannot show |
| Doc comment | 37280f0 | fix(261031-bbs): read_back doc comment lists the collapse and new-path residuals |

No commit has a co-author line. Files were staged by explicit path, and `.planning/config.json`, `.planning/research/.cache/` and `svc-ignition-deploy@52.154.75ace-uat4.ps1` were never staged. Neither this SUMMARY nor REVIEW-FIX.md is committed.

RED evidence:
- `deploy_verify rb_judge` gave `22 passed; 3 failed` on the three unobservable tests before 58fe621.
- `rb_judge_write_already_shown_root_covered_warns` failed on A1 before 451c163.
- `rb_judge_record_value_free` and `rb_apply_baseline_file_value_free` failed on the `writes_at` key before 7c04158.

The review-case pins passed on arrival. A mutation that disabled the per-path red turned them red, along with the two earlier CR-01 tests (5 in all).

### Full gate (after 37280f0)

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `PATH="$HOME/.cargo/bin:$PATH" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | 96 result lines: 3080 passed, 0 failed, 92 ignored (15 new on top of 3065) |
| Cargo.toml / Cargo.lock / goldens | unchanged since df01c31 (`deploy_plan.rs` and `contract_deploy.rs` untouched) |

### Live rig re-run (Docker only, no production call)

The env was as in the Live section above. `CAPTURE_OUT` pointed at a scratchpad file, which was created first so that `capture-cd5.sh clean` could run. No tracked file was written.

1. 18:17:18Z: `cargo install --path crates/ignition-cli --force` replaced `~/.cargo/bin/ign` (`ign 1.6.0`).
2. 18:18:21Z: `capture22.sh up22 && capture22.sh setup22` gave `RUNNING, deploymentMode production` (18:20:48Z).
3. 18:20:56Z: `capture-cd5.sh seed` exited 0.
4. 18:21:17Z: `cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1` ran these, all PASS:
   - `cd5_apply_root_and_nested_live`
   - `live_apply_then_rollback`
   - `live_kill_after_green_verify_then_recover`
   - `live_kill_mid_write_then_recover`
   - `live_lock_lapse_restores`
   - `live_preexisting_rest_difference_warns` (`Applied`, with the warning `read_back: ignition/general-alarm-settings differs from its file at: liveEventLimit (already different before this apply)`; the new `writes_at` `[]` assertion held)
   - `live_queued_config_save_during_apply` (`applied_write_refused`)

   `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 111.01s`
5. 18:23:18Z: `cargo test -p ignition-core --test live_deploy_plan -- --ignored --nocapture --test-threads=1`. `pull_then_verify_is_green_live` gave `pulled: ok true … resources_checked 28`, `ReadBack Pass []`, then `edited … ReadBack Fail [resource_differs:ignition/cobranding (differs at: appIcon)]`. `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 23.06s`
6. 18:23:59Z: `capture-cd5.sh clean` exited 0. Then `IGN_DEPLOY_RIG_RESOURCES=$HOME/.cache/ign-deploy-rig/data/config/resources docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml down -v` removed the container, the `ign-deploy-rig_gw-data` volume and the network (18:24:12Z). No `ign-deploy-rig` container, volume or network remains.

Live verdict: pass

### Residual risks added by round 2 (documented in README, pinned by tests)

- **Collapse (false green).** A key absent in the live file, an array length change, or a member REST reports as one opaque value collapses the written members into one `writes_at` path, so any REST move under it covers a member that did not land.
- **New-path rule (false red).** An explicit default REST leaves out, or a list REST reports in its own order, rolls back although the gateway took it. Author `changed` units with `deploy pull`.
- **Secret fields.** Unobservable (warn) when REST leaves them out. If a gateway returns a fixed mask instead, a password change would read as not taken. This was not verified on production.

### Follow-up (tracked)

- **Collapse-aware path sets (segment-vector paths, IN-03):** judge written leaves rather than the shallowest differing path, and stop joining path strings. This flips the three residual pins to red and ends the `.`/`[`-in-key collisions.
- Optional: extend the unobservable exclusion to the new-path rule, which would turn the explicit-default false red into a warning. This round left it out, per the round's scope.

