---
phase: 23-guarded-apply-rollback
verified: 2026-10-19T17:15:00Z
status: passed
score: 5/5 roadmap success criteria verified (12/12 requirements satisfied in code); UAT gap truths G1 and G2 verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: human_needed
  previous_score: 5/5 roadmap success criteria (12/12 requirements), 6 human items open
  previous_verified: 2026-10-18T20:00:00Z
  gaps_closed:
    - "UAT G1 (FA-23-05): plan, apply and rollback --run over ssh to a live Linux Ignition host commit, verify and restore like the Windows host (23-22, five-gate run #2 5 of 5 on ign-lnx-836; listing hardened to ls -lan by debug sftp-listing-parser-diff and re-captured live with the real parser)"
    - "UAT G2: deploy_host_conformance PowerShell rows pass on windows-latest (23-23 survey_root fix, 23-24 CI proof; re-confirmed on HEAD 6fa9d11, run 39289206545, 47 of 47)"
    - "Human item C2b: recorded (UI save queues behind the lock; Designer project saves are outside the write set; queued config save refused as stale)"
    - "Human item FA-23-06: user decision, keep BackupType::All"
    - "Human item FA-23-08: user decision, keep HOLD_S = 176 s"
    - "Human item Windows CI: green on windows-latest"
    - "Human item rig live gates after review fixes: 4 of 4 passed 2026-10-18T19:24Z"
  gaps_remaining: []
  regressions: []
gaps: []
---

# Phase 23: Guarded Apply & Rollback Verification Report

**Phase Goal:** A user can apply a reviewed plan to a single gateway and end with either a verified deploy or the snapshot restored, never a partially loaded tree, and can roll a past run back from a fresh process.
**Verified:** 2026-10-19T17:15Z
**Status:** passed
**Re-verification:** Yes. This follows UAT gap closure (plans 23-22, 23-23, 23-24), debug sessions sftp-listing-parser-diff and mcp-write-verbs-timeout, and gap-review fixes WR-01 and WR-02. HEAD is 6fa9d11.

## Re-verification (2026-10-19)

### What changed since the 2026-10-18 report

Every code commit since then touches the sftp transport or test files:

- c67e8bc, f0dced9, 3a47279, 206998e, 866a16b, 465113d: `transport/sftp.rs`, `sftp/apply_ops.rs`, `remote.rs`
- 8c424f1: the `contract_mcp.rs` harness
- live gate files, plus one headless `IsTerminal` import in `main.rs`

The apply engine (`deploy/apply/*`), gates, run store, MCP catalog and GUARDED_OPS are unchanged. So I re-checked success criteria 1 to 5 for regressions only, and fully verified the two UAT gap truths.

### UAT gap truths

| Gap | Truth | Status | Evidence (code read and test run by me, not taken from SUMMARY) |
|-----|-------|--------|------------------------------------------------------------------|
| G1 | `ign deploy plan`, `apply` and `rollback --run` over ssh to a live Linux Ignition host commit, verify and restore like the Windows host | VERIFIED | **Code:** `list_level_as` (sftp.rs:745-753) puts an `@pwd` marker before every listing. POSIX hosts get `SftpOp::LsLongAllNumeric` (`@ls -lan`, remote.rs:537) and Windows hosts get `@ls -la`. `line_columns_and_name` (sftp.rs:523) refuses a POSIX line unless it carries the exact `<dir>/` prefix after a space with one component after it. `output_lines` (sftp.rs:505) splits on `\n` only and strips `\r` only when every line is CRLF. `account_home` (sftp.rs:849) requires exactly one marker line. The parser checks the marker count, the date column and `safe_remote_path`.<br>**Tests, local run:** `posix_listing_parses_captured_ls_lan_output_from_openssh_8_9` (a verbatim capture from the real 8.9 host), `posix_listing_refuses_captured_bare_openssh_8_9_output`, `posix_listing_refuses_a_name_split_by_a_raw_newline`, `posix_listing_refuses_a_name_ending_in_a_carriage_return`. Over the real protocol (local `sftp -D`, `ls -lan`): `deploy_host_conformance` 48/0 and `deploy_transport` 63/0. On ubuntu-latest CI at 6fa9d11, `deploy_host_conformance` was 48/0.<br>**Live (recorded, not re-run per instruction), ign-lnx-836:**<br>- 23-22 five-gate run #2 at 2026-10-19T02:22:49Z: 5 passed, 0 failed. `lnxvm_cli_apply_rollback` covered plan exit 0, apply `applied` with `.gwbk`, modes 644/775, verify ok, rollback `restored`, and the plan after the rollback equal to the first.<br>- 23-24 `lnxvm_apply_script_run` on the survey_root tree: PASS.<br>- Debug D3 `lnxvm_listing_capture` with `-lan` and the real parser: PASS. It listed the same home and production directory, with the same entries. |
| G2 | The `deploy_host_conformance` PowerShell rows pass on GitHub's windows-latest runner | VERIFIED | **Code:** `survey_root` (sftp.rs:1977) awaits `account_home()` (WR-01) and compares the home's components case-insensitively on Windows (WR-02, sftp.rs:1985). A root below the home is surveyed from the home's own spelling, so nothing at or above the home is listed or created. Its callers are `put_tree` (apply_ops.rs:236) and `stage_inner` (sftp.rs:2171). `failing_command_is_mkdir` and the "may already exist" class text are at remote.rs:634 and remote.rs:1009.<br>**Tests, local run:** `prepare_below_a_short_name_home_makes_nothing_at_or_above_it`, `mirror_stage_without_a_cached_home_surveys_from_the_short_name_home`, `put_tree_without_a_cached_home_surveys_from_the_short_name_home`, `mirror_stage_below_a_windows_home_spelled_in_another_case`, `a_mkdir_failure_is_not_called_a_held_target`: all ok.<br>**CI, checked with `gh` by me:** run 39289206545, pull_request on phase-23-ci, headSha 6fa9d11c (equal to local HEAD and to `origin/phase-23-ci`). check ubuntu-latest, macos-latest, windows-latest and keyring-smoke all `success`. In the windows-latest log, `deploy_host_conformance` ran 47 tests, 47 passed, 0 failed: every `conformance_ps_*` row and `ps_*` test reports ok. No `test result:` line in the run reports a failure. |

### Previous human-verification items

| Item | Resolution |
|------|-----------|
| C2b: Designer or UI save under the scan-lock | UAT test 1, pass. The UI save queues behind the lock. Designer project saves are outside the write set. Rig test `live_queued_config_save_during_apply` shows a queued config save is refused as stale (23-LIVE-CAPTURES.md `## C2b (manual)`). |
| FA-23-05: Linux ssh host | UAT test 2, pass (G1 above) |
| FA-23-06: `BackupType::All` | UAT test 3, user decision, keep |
| FA-23-08: `HOLD_S = 176` | UAT test 4, user decision, keep |
| Windows CI | UAT test 5, pass (G2 above; re-confirmed on 6fa9d11) |
| Rig live gates after CR-01/WR-01/WR-04 | UAT test 6, pass, 4 of 4. Later commits do not touch the rig path: they change the sftp transport only, and the rig does not use sftp. |

### Regression check of the success criteria

All five truths below still hold. The suites that back them pass on HEAD 6fa9d11 in my run:

- `deploy_apply` 91/0
- `contract_deploy` 55/0
- `contract_mcp` 15/0, 1 ignored
- `deploy_family_coverage` 7/0
- `contract_stdout_purity` 13/0
- `ignition-core` lib 977/0

I ran `cargo test --workspace --no-fail-fast` once, on the main checkout with no worktree: **2524 passed, 0 failed, 69 ignored** (exit 0). `mcp_tools_call_write_verbs_unknown` passed. Its harness fix (8c424f1) keeps an out-of-order reply for its own wait instead of dropping it. No timeout or assertion was relaxed, and `expect_silence` now also fails on an unclaimed reply.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | apply without `--yes` refuses with the plan. A `--plan-sha256` mismatch is `plan_stale`. Destructive, drift and `shadows_removal` gates apply. An empty plan exits 0 with no lock, snapshot or scan. | VERIFIED | `apply/gates.rs::evaluate_gates` checks in this order: flag validation, plan_stale, noop, deploy_blocked, destructive, drift. `run_apply` returns `Noop` before `RunDir::create_fresh`, before any host call, lock or scan (`empty_plan_is_a_noop`). The drift reason is recorded in `run.json`. CLI matrix in `contract_deploy` (55 passed). |
| 2 | All preflights are reported in one refusal. A snapshot is taken on the host and in the run dir, with hashes. `.gwbk` is on by default and `--skip-gwbk` is recorded. Nothing is ever uploaded. | VERIFIED | `run_preflights` runs before any stage or lock. Tests: `preflight_all_fail_in_fixed_order`, `preflight_designers_*`, `preflight_wrong_target*`, `snapshot_gwbk_*`, `snapshot_skip_gwbk_is_recorded`, `snapshot_run_dir_hashes_equal_record`. The gwbk is download-only. On Linux ssh, live `lnxvm_cli_apply_rollback` recorded the `.gwbk`. |
| 3 | Writes happen inside the scan-lock within the measured hold. Fault injection ends with the snapshot restored and scanned. A red verify rolls back as `deploy_rolled_back` exit 6. A failed rollback is `rollback_failed` with guidance. | VERIFIED | `HOLD_S = 176` (user-accepted). Every abort goes through one `restore_snapshot`. CR-01 post-commit handling. Tests: `fault_write_failed_mid_commit_restores`, `a_commit_that_outruns_the_hold_stops_and_restores_as_lock_expired`, `auto_rollback_*`, `post_commit_*` (`deploy_apply` 91/0). Live: rig `live_lock_lapse_restores`, the Windows gates, and on Linux `lnxvm_sh_commit_time_budget` (the WR-04 deadline over ssh). |
| 4 | `rollback --run` from a fresh process restores, and refuses on a fingerprint mismatch unless `--discard-live-changes`. The journal blocks plan, apply and rollback. Every run writes a run dir. `restart_pending` is reported and the gateway is never restarted. | VERIFIED | `rollback_fresh_process_from_record_only`, `rollback_live_changed_refuses`, `rollback_live_edit_after_the_check_is_refused_under_the_lock`, `journal_blocks_cli_then_recovers`, `run_dir_matrix`, `run_dir_never_restarts`. Live: rollback `restored` on Linux and Windows, and `live_kill_*` on the rig. |
| 5 | `apply` and `rollback` are in GUARDED_OPS and not in MCP. render, plan, pull and verify remain in MCP. The secret canary covers every surface. | VERIFIED | `mcp.rs` `EXCLUDED_PATHS` is unchanged. `contract_mcp` 15/0 (1 ignored). Canary suites and `deploy_family_coverage` 7/0. `contract_stdout_purity` 13/0. |

**Score:** 5/5 truths verified, 0 behavior-unverified.

### Requirements Coverage

| Requirement | Verdict | Evidence |
|-------------|---------|----------|
| DAPL-01 | SATISFIED | `evaluate_gates` plan_stale and refusal prose; CLI matrix tests |
| DAPL-02 | SATISFIED | destructive, drift (reason recorded) and `shadows_removal` -> `deploy_blocked` gates; `apply_canary_drift_reason` |
| DAPL-03 | SATISFIED | `empty_plan_is_a_noop` |
| DAPL-04 | SATISFIED | `preflight.rs`, all-at-once refusal, Designer modes |
| DAPL-05 | SATISFIED | host and run-dir snapshot with hashes; `.gwbk` default (BackupType::All, user-confirmed), `--skip-gwbk` recorded |
| DAPL-06 | SATISFIED | `HeldLock`, measured `HOLD_S`, commit deadline, restore on expiry or write failure |
| DAPL-07 | SATISFIED | auto-rollback `deploy_rolled_back` exit 6; `rollback_failed` with guidance |
| DAPL-08 | SATISFIED | fresh-process rollback; live-change refusal re-checked under the lock |
| DAPL-09 | SATISFIED | journal check in plan, apply and rollback; kill tests and live kill gates; the journal refusal over Linux ssh (`lnxvm_apply_script_run`) |
| DAPL-10 | SATISFIED | `result.json` written on every path; `restart_pending` is read-only |
| DSURF-02 | SATISFIED | MCP exclusion by full path plus GUARDED_OPS |
| DSURF-03 | SATISFIED | canary suites over plan, apply, rollback and verify output, `--json`, the run dir, events and on-box files |

No orphaned requirements: plans claim all 12 Phase 23 IDs.

### Behavioral Spot-Checks (run by me)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Whole workspace (once) | `cargo test --workspace --no-fail-fast` | 2524 passed, 0 failed, 69 ignored, exit 0 | PASS |
| G1 parser hardening | named tests in the `ignition-core` lib run (above) | ok | PASS |
| G2 survey and mkdir text | named tests in the `ignition-core` lib run (above) | ok | PASS |
| CI on HEAD | `gh run view 39289206545` | headSha 6fa9d11, 4 of 4 jobs success; windows-latest conformance 47/0 | PASS |

I did not re-run the live gates, per instruction. The live evidence is the recorded runs cited above.

### Anti-Patterns

The added lines of the gap-closure diff (`5fa3897..HEAD -- crates/`) contain no TBD/FIXME/XXX/TODO/HACK, `todo!()`, `unimplemented!()` or `.only`. Live gates stay `#[ignore = "live: ..."]` by design.

### Open, non-blocking

- **Gap review IN-01..IN-08** (23-REVIEW-GAPS.md) are left open by request. They are all Info: defence in depth, harness hygiene, and diagnostic text. None makes a must-have false.
- **Linux CLI gate on the `ls -lan` tree.** `lnxvm_cli_apply_rollback` last ran on the 23-22 tree, before 206998e. The changed surface is only the POSIX listing command and its parse. On the same host, that surface was re-captured live with the real parser, over the same home and production directory, with identical entries. It also passes over the real protocol on macOS and ubuntu-latest. WR-01 does not change the `prepare` path, because `staging_abs` already asked the home first. A re-run before a first production Linux apply is advisable but not required.
- **Move across file systems.** Not captured: on ign-lnx-836 the account home and the data directory share one file system. The README states this and advises a disposable gateway and keeping the `.gwbk`. No Phase 23 requirement covers it.
- **Linux account rights.** The deploy account needs read and execute (list) on `data` and `data/config`. The README documents this, and a follow-up note for spec section 9 (PR #63) is in 23-LIVE-CAPTURES.md.

### Documentation state (for the orchestrator, not a code gap)

- REQUIREMENTS.md still shows DAPL-01..07, DAPL-10, DSURF-02 and DSURF-03 unchecked and "Pending". The ROADMAP Phase 23 line is unchecked. Update both on phase completion.
- 23-LIVE-CAPTURES.md `## G1: POSIX sftp listing shapes` still describes the 23-22 bare-name reading. It names `posix_listing_parses_captured_openssh_8_9_output`, which is now `posix_listing_refuses_captured_bare_openssh_8_9_output`, and it has no record of the D3 `-lan` capture. That record exists only in `.planning/debug/resolved/sftp-listing-parser-diff.md`. A one-paragraph pointer there would keep the capture file accurate.
- 6fa9d11 (local main) is pushed to `phase-23-ci`; PR #21 is still a draft.

### Human Verification Required

None. All six items from the 2026-10-18 report are resolved in 23-UAT.md (6 of 6 pass).

### Gaps Summary

None. G1 and G2 hold in code, in local tests, in CI on the current head and in recorded live gates. All five success criteria and all 12 requirements still hold, with no regressions.

---

_Verified: 2026-10-19T17:15Z_
_Verifier: Claude (gsd-verifier)_
