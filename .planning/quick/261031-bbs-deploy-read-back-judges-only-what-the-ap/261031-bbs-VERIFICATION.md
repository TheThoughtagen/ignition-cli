---
phase: quick-261031-bbs
verified: 2026-10-31T17:30:00Z
status: human_needed
score: 7/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Docker 8.3.6 rig gate: run `cargo test -p ignition-core --test live_deploy_apply live_preexisting -- --ignored --nocapture --test-threads=1` against ign-deploy-rig (IGNITION_LIVE_DEPLOY=1), then the full live_deploy_apply ignored set"
    expected: "Apply ends Applied with a warning `read_back: ignition/general-alarm-settings differs from its file at: liveEventLimit (already different before this apply)`; standalone verify gives read_back warn; the 7 existing live apply gates still pass"
    why_human: "Needs a live gateway. This verifier was told to run no live or ignored tests, so the only evidence is the executor's SUMMARY log (15:50Z and 15:51Z runs)"
  - test: "Windows cross-clippy: `PATH=\"$HOME/.cargo/bin:$PATH\" cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings` (or the CI Windows job)"
    expected: "exit 0, no warnings"
    why_human: "Part of the must-have 7 gate. The verifier did not re-run it; the evidence is the SUMMARY claim. Nothing in the diff is cfg(windows)-specific, so the risk is low"
---

# Quick Task 261031-bbs: deploy read_back judges only what the apply changed. Verification Report

**Goal:** the post-apply `read_back` in `ign deploy apply` (master and backup convergence) judges against a pre-apply REST baseline. Differences that were already there are warnings. Only changes the apply introduced, or writes that did not take effect, are red. Standalone `deploy verify` warns on read_back differences for plan-`same` resources only. The run directory holds a value-free `read_back_baseline.json`. Rollback stays automatic and no flag skips verify. Goldens are unchanged and there is no Cargo bump. v1.6.3.
**Verified:** 2026-10-31
**Status:** human_needed (all offline must-haves verified; the live rig gate and the Windows cross-clippy were not re-run here)
**Re-verification:** No, this is the initial verification.
**Range:** 208c6bf..41301d8 (6 commits) on `fix/deploy-read-back-scope`. The verifier changed no code.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Unwritten resource with a pre-existing REST difference: apply ends `applied` (exit 0), one warning line names the key and paths, the read_back check is `warn`, and the finding has `status: "warn"` | VERIFIED | `rb_apply_preexisting_unwritten_difference_warns` passes: Applied, `failure_exit_code()==None`, the warning names `ALARM_UNIT` and the overlay member, and the event order equals a plain run. The check status and the finding status are pinned by `rb_judge_unwritten_preexisting_warns` and `rb_judge_finding_status_serialization`. Code: `verify.rs` `judge_resource`, Baseline/DeclaredFile arm (L1082-1097) and `read_back_check_with` status rule (L978-985) |
| 2 | Unwritten resource moved between baseline and post-scan and still differs: `rolled_back` / `verify_failed`; `verify_checks` names the key and paths, with no values | VERIFIED | `rb_apply_unwritten_moved_rolls_back` passes. The overlay is armed after commit, the run rolls back with `verify_failed`, and the `verify_checks` event finding has code `resource_differs`, subject = key, and a detail naming the member and "changed during this apply". Event serialization: `apply/mod.rs` L628-635. Finding details are value-free by construction (paths, codes, timestamps, slugs) |
| 3 | Written `changed`: a difference within the live-file baseline set (an ancestor counts) warns; a new path or a write that did not take effect is red and rolls back; `added` is strict | VERIFIED | Passing tests: `rb_apply_written_preexisting_difference_passes`, `rb_apply_written_new_difference_rolls_back`, `rb_apply_written_not_taken_effect_rolls_back` (A1), `rb_judge_prefix_rule`, `rb_judge_written_*`, `rb_judge_write_not_taken_effect_root_covered_fails`, `rb_judge_added_is_strict`, `rb_judge_uncapped_paths`. Code: LiveFile arm L1106-1133 and `path_covered` L751-760. Live compare built in `read_back_baselines` (`apply/mod.rs` L1496-1535) |
| 4 | Pair: a backup with a pre-existing difference converges (`applied`); the backup is judged on its own baseline; a diverged written unit still ends `applied_backup_unsynced` / `content_differs` | VERIFIED | `rb_apply_pair_backup_preexisting_difference_converges` passes: Applied, converged, backup read_back `warn` with one warn finding, and the baseline file lists `["master","backup"]` with the backup-only `differs_at`. `backup_unsynced_content_differs` still passes. Wiring: `converge::Baseline.read_back` is passed from `drive` (L1404-1408), and `Phase::content` calls `read_back_backup_check_with(ReadBackJudge::Baseline)` (converge.rs). `converged()` only blocks on `Fail` (pair.rs L538) |
| 5 | Standalone verify (master, `--node backup\|both`) warns only on plan `same`; `changed`, `added` or unknown stay red (exit 6); warn-only is green with exit 0 | VERIFIED | Passing: `rb_verify_standalone_same_warns_through_action`, `rb_verify_standalone_changed_stays_red_through_action`, `rb_verify_backup_same_warns`, `rb_verify_backup_changed_stays_red`, `rb_judge_standalone_*`, `verify_green_and_red_through_action`, `verify_cli_preexisting_warns_exit_0` (exit 0, `data.ok` true, ` (warn)` human suffix), `verify_cli_red_exits_6`. Wiring: `deploy_verify_planned` builds `ReadBackJudge::Standalone{file_status}` (actions/deploy.rs L1200-1205); `verify_backup_tick` uses master or files statuses, else Strict (L1463-1467) |
| 6 | `read_back_baseline.json` is written before `staged`, value-free (state, sha256, basis, uncapped paths); read before `prepare`, outside the lock; a failed baseline read stages nothing | VERIFIED | `read_back_baselines(...)` is called after `spec.problem()` and before `ctx.host.prepare` and `HeldLock::acquire` (apply/mod.rs L1249-1284), and `write_json` (0600) runs inside it. Passing: `rb_apply_baseline_file_value_free` (mode 0600, schema 1, 64-hex hash, bases, canary absent, `run_dir_canary_leaks` empty); `rb_apply_baseline_read_failure_writes_nothing` (network_error, 0 lock acquires, collection unchanged, no `staged`, no `prepare`); `rb_judge_record_value_free`. The event order of a plain run is unchanged (`RB_APPLIED_EVENTS`) |
| 7 | No slug or finding code added; optional `status` serialized only as warn; goldens byte-identical; README documents the rules; gate green; no Cargo change; no production call | VERIFIED | `Finding.status` has `skip_serializing_if = "Option::is_none"` (verify.rs L205-206). No new finding code strings appear in the diff. `deploy_plan.rs` is untouched. The `contract_deploy.rs` diff has no GOLDEN or `7c8b41b1` line. `plan_sha256_golden*` (3) and `render_digest_golden` pass. `git diff 208c6bf..HEAD -- Cargo.toml Cargo.lock crates/*/Cargo.toml` is empty. fmt clean, workspace clippy `-D warnings` clean, `cargo test --workspace` gives 96 result lines, 3061 passed, 0 failed, 92 ignored, exit 0 (matches SUMMARY). README: read_back row, standalone warning rule, baseline section, run-dir contents, secrets list, and the verdict sentence ("no flag skips the verify or the rollback" is kept at L585 and L3613). Windows xwin clippy was not re-run (see human item 2) |
| 8 | Docker 8.3.6 rig: a pre-existing REST difference applies with a warning; the standalone verify is `read_back: warn`; the existing live gates pass | ? UNCERTAIN (human) | `live_preexisting_rest_difference_warns` exists and is substantive (`live_deploy_apply.rs` L2123-…, `#[ignore]` live gate, seeds `general-alarm-settings` without `liveEventLimit`). The pass evidence is only the executor's SUMMARY log. Live tests were out of scope for this verifier |

**Score:** 7/8 truths verified (0 present-but-behavior-unverified, 1 needing human or live confirmation)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/ignition-core/src/deploy/verify.rs` | ReadBackJudge, ReadBackBaseline, BaselineBasis, Written, evaluate_with, read_back_check_with, compare_for, record, Finding.status | VERIFIED | All present and substantive; `evaluate` delegates to `evaluate_with(Strict)` |
| `crates/ignition-core/src/actions/deploy.rs` | VerifyContext, deploy_verify_judged, Standalone judge for master and backup | VERIFIED | Wired at L1108, L1200-1205, L1309, L1463-1467 |
| `crates/ignition-core/src/deploy/apply/mod.rs` | pre-prepare baselines, live-file basis, read_back_baseline.json, verify_checks findings | VERIFIED | `read_back_baselines` L1483-1556; `confirm` uses `VerifyContext::PostApply` L1609 |
| `crates/ignition-core/src/deploy/apply/converge.rs` | backup judged on its own baseline | VERIFIED | `read_back_backup_check_with` with `ReadBackJudge::Baseline` |
| `crates/ignition-core/src/deploy/pair.rs` | `read_back_backup_check_with` | VERIFIED | Present; delegates to `verify::read_back_check_with` |
| `crates/ignition-cli/src/render.rs` | ` (warn)` suffix on master and backup findings | VERIFIED | `warn_suffix` used in both renderers; `describe_apply_event` compiles with the 3-tuple |
| `crates/ignition-core/tests/common/fake_deploy.rs` | `rest_overlay` | VERIFIED | Persists across `reload_from_disk` (models a filled default) |
| `crates/ignition-core/tests/deploy_apply.rs` | rb_apply_* | VERIFIED | 8 tests, all pass |
| `crates/ignition-core/tests/deploy_verify.rs` | rb_judge_*, rb_verify_* | VERIFIED | 18 + 4 tests, all pass |
| `crates/ignition-core/tests/live_deploy_apply.rs` | live_preexisting_rest_difference_warns | VERIFIED (exists) | Not run (live) |
| `README.md` | read_back row, baseline, run-dir, secrets, verdict | VERIFIED | See truth 7 |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `apply::drive` (after `spec.problem()`, before `prepare`) | `gather_read_back` → `ReadBackBaseline::build` | `read_back_baselines(...).await?`, outside the lock | WIRED |
| `apply::confirm` | `deploy_verify_judged(VerifyContext::PostApply)` → `evaluate_with(Baseline)` | red verdict → `Confirmed::Abort(VerifyFailed)` → rollback | WIRED (exercised by the rb_apply rollback tests) |
| `converge::Phase::content` | `pair::read_back_backup_check_with(Baseline(backup))` | `converge::Baseline.read_back` from `drive` | WIRED (exercised by the pair test) |
| `deploy_verify` / `verify_backup_tick` | `ReadBackJudge::Standalone { file_status }` | plan statuses from the master live read | WIRED (exercised by rb_verify_* and the CLI test) |

### Behavioral Spot-Checks (run by the verifier, offline only)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Judge and standalone action | `cargo test -p ignition-core --test deploy_verify rb_` | 22 passed, 0 failed | PASS |
| Apply scenarios 1-3, A1, pair, baseline file, baseline failure | `cargo test -p ignition-core --test deploy_apply rb_apply` | 8 passed, 0 failed | PASS |
| CLI verify contract | `cargo test -p ignition-cli --test contract_deploy verify_cli` | 17 passed, 0 failed | PASS |
| Goldens | `deploy_plan golden` (3) and `contract_deploy render_digest_golden` (1) | 4 passed | PASS |
| Regression guards | `deploy_apply backup_unsynced_content_differs auto_rollback_verify_red_restores` | 2 passed | PASS |
| Formatting | `cargo fmt --all --check` | clean | PASS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | no warnings | PASS |
| Full workspace (run once) | `cargo test --workspace` | 96 result lines; 3061 passed, 0 failed, 92 ignored; exit 0 | PASS |

Build hygiene: no git worktree exists (`git worktree list` shows the main checkout only), so the shared global `CARGO_TARGET_DIR` (`~/Library/Caches/cargo-target`) has a single checkout building into it, per `.claude/CLAUDE.md`.

### Probe Execution

Not applicable. The task declares no `probe-*.sh`; its live gate is the ignored cargo test listed under human verification.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|-------------|-------------|--------|----------|
| QUICK-bbs | 261031-bbs-PLAN.md | SATISFIED (offline) | Truths 1-7 verified; truth 8 is live-only |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (diff 208c6bf..HEAD) | - | No TODO, FIXME, XXX, TBD, `todo!`, `unimplemented!`, `.skip` or `.only` in added lines | - | none |

Notes (informational, not gaps):
- Two existing tests (`verify_cli_red_exits_6`, `verify_green_and_red_through_action`) had a fixture change: they now drift the live translator so the plan says `changed`. Their red assertions are unchanged, and the old scenario became the new warn assertion. This is a legitimate consequence of R2, not a weakened test.
- An apply's `data.warnings` holds the per-resource line plus the check-level summary line `read_back: <n> declared resource(s) compared ...; <w> difference(s) reported as warnings`. Only the per-resource line names the key, which matches truth 1 and the README.
- The residual risks (masking a normalised written path, the baseline-to-scan window false red, and strict `added`) are documented in the `read_back_check_with` doc comment and in README, as the plan required.

### Human Verification Required

#### 1. Docker 8.3.6 rig gate (truth 8)

**Test:** With ign-deploy-rig up (`capture22.sh up22 && setup22`, env per SUMMARY), run `cargo test -p ignition-core --test live_deploy_apply -- --ignored --nocapture --test-threads=1`.
**Expected:** `live_preexisting_rest_difference_warns` passes (Applied, `liveEventLimit` warning, standalone `read_back: warn`), and the other live apply gates pass.
**Why human:** Needs a live gateway, and this verification was scoped to offline tests. The SUMMARY records a pass at 15:50Z and 15:51Z on 2026-10-31.

#### 2. Windows cross-clippy (part of truth 7's gate)

**Test:** `cargo xwin clippy -p ignition-core --target x86_64-pc-windows-msvc --all-targets -- -D warnings`, or confirm the CI Windows job is green on the PR.
**Expected:** exit 0.
**Why human:** Not re-run by the verifier. Low risk: no `cfg(windows)` code changed.

### Gaps Summary

No gaps. Every offline must-have is backed by code that is present, wired and exercised by passing tests that the verifier ran itself: the baseline judge, master and backup wiring, rollback on real disturbance, standalone plan-`same` warnings, the value-free baseline file and event findings, the additive contract, unchanged goldens and no Cargo change. The only open items are the live Docker rig gate and the Windows cross-clippy, which this verifier was not allowed to run or did not re-run. The production canary re-run is out of scope (post-release hand-off).

---

_Verified: 2026-10-31_
_Verifier: Claude (gsd-verifier)_
