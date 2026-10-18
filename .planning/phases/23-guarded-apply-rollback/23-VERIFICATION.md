---
phase: 23-guarded-apply-rollback
verified: 2026-10-18T20:00:00Z
status: human_needed
score: 5/5 roadmap success criteria verified (12/12 requirements satisfied in code)
behavior_unverified: 0
overrides_applied: 0
re_verification: false
gaps: []
human_verification:
  - test: "C2b: Designer or gateway-UI save while the scan-lock is held"
    expected: "Record whether the save succeeds, is refused or queues (steps in 23-LIVE-CAPTURES.md `## C2b (manual)`)"
    why_human: "Interactive session; no driver exists. Until recorded, the Designer preflight is the only assumed guard against a UI save during an apply."
  - test: "FA-23-05: Linux ssh host (`commit.sh`) against a live Linux gateway host"
    expected: "apply and rollback complete; file modes 0644/0755, cross-filesystem mv and the sha tool behave"
    why_human: "No Linux host was captured. commit.sh is proven only under local sh against the shared conformance table. README marks it unproven."
  - test: "FA-23-06: the .gwbk uses BackupType::All"
    expected: "User confirms All (gateway-specific state included) is the intended restore-point type"
    why_human: "Design choice D-13 does not name the type; flagged for user review."
  - test: "FA-23-08: HOLD_S = 176 s, sized from the replace commit (85.496 s at 1,520 files), not the plan's new-file commit (129 s)"
    expected: "User accepts the more conservative hold (the alternative is two literals in hold.rs)"
    why_human: "Judgement on a deliberate deviation from the plan."
  - test: "Windows CI (windows-latest) push and run of deploy_host_conformance and the PowerShell rows"
    expected: "Green on windows-latest"
    why_human: "Not pushed. Conformance was run for real on the Azure Windows host, not on the CI runner."
  - test: "Re-run the rig live gates `live_deploy_apply -- --ignored` (4 tests) on the post-review-fix tree"
    expected: "live_apply_then_rollback, live_kill_*, live_lock_lapse_restores still pass"
    why_human: "Last recorded rig run (2026-10-18T04:19Z) predates review fixes CR-01, WR-01, WR-04 (19:10Z). Windows script gates were re-run after WR-04; the rig gates were not. Offline coverage of the new paths passes."
---

# Phase 23: Guarded Apply & Rollback Verification Report

**Phase Goal:** A user can apply a reviewed plan to a single gateway and end with either a verified deploy or the snapshot restored, never a partially loaded tree, and can roll a past run back from a fresh process.
**Verified:** 2026-10-18
**Status:** human_needed (no code gaps; the open items are verification gaps the phase already declared)
**Re-verification:** No, initial verification (after code-review fixes a35342a..3cec932)

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | apply without `--yes` refuses with the plan; `--plan-sha256` mismatch is `plan_stale`; destructive/drift/`shadows_removal` gates; empty plan exits 0 with no lock, snapshot or scan | VERIFIED | `apply/gates.rs::evaluate_gates` (order: flag validation, plan_stale, noop, deploy_blocked, destructive, drift); `run_apply` returns `Noop` before `RunDir::create_fresh`, any host call, lock or scan; test `empty_plan_is_a_noop`; drift reason recorded in `run.json` (`allow_drift`); CLI matrix in `contract_deploy` (55 passed) |
| 2 | All preflights reported in one refusal (mode on both nodes, wrong target, write probe, no journal, Designers fail/wait/proceed); snapshot on host and in run dir with hashes; `.gwbk` default-on, `--skip-gwbk` recorded; never uploaded | VERIFIED | `run_preflights` before any stage/lock (`drive`); tests `preflight_all_fail_in_fixed_order`, `preflight_designers_*`, `preflight_wrong_target*`, `snapshot_gwbk_*`, `snapshot_skip_gwbk_is_recorded`, `snapshot_run_dir_hashes_equal_record`; gwbk is download-only (`take_gwbk`), skip in `run.json` |
| 3 | Writes inside scan-lock within the measured hold; fault injection (lock expiry, mid-write failure) ends with snapshot restored and scanned; red verify auto-rolls back as `deploy_rolled_back` exit 6; failed rollback is `rollback_failed` with guidance | VERIFIED | `HoldModel::MEASURED`, `HOLD_S = 176` from OQ5b (`hold.rs`); one `restore_snapshot` for every abort; CR-01 gives every post-commit `Err` a restore or typed interrupted-run exit; tests `fault_write_failed_mid_commit_restores`, `a_commit_that_outruns_the_hold_stops_and_restores_as_lock_expired`, `auto_rollback_*`, `post_commit_*` (deploy_apply: 91 passed, run by me); live rig `live_lock_lapse_restores` and Windows gates per 23-LIVE-CAPTURES.md |
| 4 | `rollback --run` from a fresh process restores the snapshot, refuses on post-apply fingerprint mismatch unless `--discard-live-changes`; killed apply leaves on-box journal and plan/apply/rollback refuse; every run writes run.json/result.json/plan.json/events.ndjson; `restart_pending` reported, never restarts | VERIFIED | `rollback_fresh_process_from_record_only`, `rollback_live_changed_refuses`, WR-01 re-check under the lock (`rollback_live_edit_after_the_check_is_refused_under_the_lock`); binary test `journal_blocks_cli_then_recovers` shows plan, apply and rollback-of-another-run refuse with the exact recovery command and write nothing; `run_dir_matrix`, `run_dir_never_restarts`; no restart/failover call anywhere under `deploy/apply` (read-only `restart_pending`) |
| 5 | `apply` and `rollback` in GUARDED_OPS and absent from MCP; render/plan/pull/verify remain; secret canary over plan, apply, rollback, verify, run-dir files, events | VERIFIED | `mcp.rs` `EXCLUDED_PATHS = ["deploy apply","deploy rollback"]`, tests assert exact set `{edit, deploy apply, deploy rollback}` and that the four unguarded tools stay (contract_mcp 12 passed); `GUARDED_OPS` entries in `main.rs`; canary tests `apply_canary`, `apply_canary_all_outcomes`, `rollback_canary`, `canary_repeat_runs_clean`, `apply_canary_drift_reason`, `canary_on_box_files_clean`, `canary_aborted_run_clean`; `deploy_family_coverage` 7 passed; stdout purity 13 passed |

**Score:** 5/5 truths verified, 0 behavior-unverified.

### Requirements Coverage

| Requirement | Verdict | Evidence |
|-------------|---------|----------|
| DAPL-01 | SATISFIED | `evaluate_gates` plan_stale / refusal prose; CLI matrix tests |
| DAPL-02 | SATISFIED | gates for destructive, drift (reason recorded in `run.json`), `shadows_removal` -> `deploy_blocked`; `apply_canary_drift_reason` |
| DAPL-03 | SATISFIED | `empty_plan_is_a_noop`; early return before run dir, lock, scan |
| DAPL-04 | SATISFIED | `preflight.rs`; `preflight_all_fail_in_fixed_order`, Designer modes, both-node mode (fake backup node, per phase scope) |
| DAPL-05 | SATISFIED | host snapshot + run-dir snapshot with hashes; `.gwbk` default-on, validated, `--skip-gwbk` recorded |
| DAPL-06 | SATISFIED | `HeldLock`, measured `HOLD_S`, commit deadline (WR-04), restore on expiry or write failure; fault tests and live rig/Windows captures |
| DAPL-07 | SATISFIED | `roll_back` through `restore_snapshot`; `deploy_rolled_back` in `data`, exit 6; `rollback_failed` with retry-first guidance (WR-02) |
| DAPL-08 | SATISFIED in code (not just the checkbox) | `rollback.rs` reads on-box record only; fresh-process test and binary tests; live-change refusal re-checked under the lock (WR-01). Residual: narrow window between that re-check and the host `restore` move (documented in REVIEW-FIX; not a gap against the requirement text) |
| DAPL-09 | SATISFIED in code (not just the checkbox) | journal check in `actions/deploy.rs` (plan), apply, rollback; `journal_blocks_cli_then_recovers`; kill tests `journal_kill_mid_write_then_recover`, `journal_after_green_verify_restores`; live kill gates |
| DAPL-10 | SATISFIED | `run_apply` single exit writes `result.json` on every path incl. errors; `run_dir_matrix`; `restart_pending` read-only |
| DSURF-02 | SATISFIED | MCP exclusion by full path + GUARDED_OPS; catalog tests |
| DSURF-03 | SATISFIED | canary suites above cover plan/apply/rollback/verify output, `--json`, run-dir files, events and on-box files |

No orphaned requirements: all 12 IDs in REQUIREMENTS.md for Phase 23 are claimed by plans.

### Behavioral Spot-Checks (run by me, single suites)

| Suite | Result |
|-------|--------|
| `cargo test -p ignition-core --test deploy_apply --test deploy_host_conformance` | 91 + 48 passed, 0 failed |
| `cargo test -p ignition-cli --test contract_deploy --test deploy_family_coverage --test contract_mcp --test contract_stdout_purity` | 55 + 7 + 12 (1 ignored) + 13 passed, 0 failed |

Full workspace (2487 passed / 0 failed / 62 ignored, clippy and fmt clean) is the orchestrator's regression gate and was not re-run.

### Anti-Patterns

No TBD/FIXME/XXX/TODO/HACK, `todo!()` or `unimplemented!()` in `deploy/`, `actions/deploy_apply.rs`, `main.rs` or `tests/deploy_apply.rs`. No `--force`, `--skip-verify` or `--no-rollback` flag in `cli.rs`. Info findings IN-01..IN-05 from the review remain unfixed by scope (not blockers).

### Documentation State (for the orchestrator, not a code gap)

REQUIREMENTS.md still shows DAPL-01..07, DAPL-10, DSURF-02, DSURF-03 unchecked and "Pending" in the traceability table (DAPL-08/09 are ticked). ROADMAP.md Phase 23 line is unchecked although 21/21 plans are executed. Update both on phase completion.

### Human Verification Required

See frontmatter `human_verification`: C2b manual Designer save under lock; FA-23-05 Linux ssh host; FA-23-06 `BackupType::All`; FA-23-08 hold basis; Windows CI push; re-run of the rig live gates after the review fixes. CR-01, WR-01, WR-04 and WR-05 were flagged "requires human verification (control flow)" by the fixer; each has failing-before/passing-after tests, so this is a reviewer sign-off rather than an open defect.

### Gaps Summary

None. Every success criterion and requirement is backed by code and passing tests, with live captures for the rig and the Windows host. Remaining items are the declared manual or unproven-environment checks.

---

_Verified: 2026-10-18_
_Verifier: Claude (gsd-verifier)_
