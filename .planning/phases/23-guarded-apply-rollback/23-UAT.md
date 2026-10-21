---
status: complete
phase: 23-guarded-apply-rollback
source: [23-VERIFICATION.md]
started: 2026-10-18T19:30:00Z
updated: 2026-10-19T17:10:00Z
---

## Current Test

[testing complete]

## Tests

### 1. C2b — Designer or gateway-UI save while the scan-lock is held
expected: The save is blocked, or the run detects it and restores. The outcome is recorded in 23-LIVE-CAPTURES.md.
result: pass (recorded, 2026-10-18T21:17Z to 21:21Z, gateway web UI only). The lock queues the UI save: the UI shows 503 and the save is applied 0.3 s after release. It is not blocked. Recorded under 23-LIVE-CAPTURES.md `## C2b (manual)`. The Designer run (21:34Z to 21:38Z) found that project saves are not under the config lock and land immediately. They are outside the apply's write set (mode collection config only), so there is no conflict. Rig test `live_queued_config_save_during_apply` (2026-10-18T22:09Z): a config save queued during the apply's hold is refused as stale (`resource_signature_rejected`) because the engine's release scan reloaded the deployed copy first; the apply reported applied with the declared config on disk and in REST and a green fresh verify. Recorded under 23-LIVE-CAPTURES.md `## C2b (manual)`.

### 2. FA-23-05 — Linux ssh host (`commit.sh`) against a live Linux gateway
expected: `ign deploy apply` and `ign deploy rollback --run` over ssh to a live Linux Ignition host commit, verify and restore the same way the Azure Windows host does, including the WR-04 `deadline` stop.
result: pass (2026-10-19, ign-lnx-836, after 23-22). Five-gate run #2 at 02:22:49Z, one sequential invocation: 5 passed, 0 failed (`lnxvm_apply_script_run`, `lnxvm_cli_apply_rollback` with plan exit 0, apply `applied` with `.gwbk`, modes 644/775, verify ok, rollback `restored`, plan after equal to the first; `lnxvm_conformance_table_on_host` 48 of 48; `lnxvm_listing_capture`; `lnxvm_sh_commit_time_budget`). Needs read and execute (list) on `data` and `data/config` for the deploy account (user grant, non-recursive). Later on the same host: `lnxvm_apply_script_run` on the survey_root tree (23-24, 06:54Z, PASS) and `lnxvm_listing_capture` with `ls -lan` and the real parser (debug sftp-listing-parser-diff, D3, PASS). Not captured: a move across file systems (home and production share one file system there; README says so). Recorded under 23-LIVE-CAPTURES.md `### Grant of list rights on data and data/config, and the rerun of all five gates (2026-10-19)` and `### FA-23-05 status`. The first run (2026-10-18T23:36Z to 23:45Z) was an issue: every sftp listing failed on the bare-name `ls -la` shape (gap G1 below).

### 3. FA-23-06 — gateway backup type
expected: The user confirms the pre-apply `.gwbk` should use `BackupType::All`.
result: pass (user decision 2026-10-18: keep `BackupType::All`).

### 4. FA-23-08 — scan-lock hold size
expected: The user accepts `HOLD_S = 176` s (sized from the replace commit, 85.496 s at 1,520 files) instead of the plan's 129 s, or asks to switch back (two literals in `hold.rs`).
result: pass (user decision 2026-10-18: keep `HOLD_S = 176` s, 1,527-file ceiling).

### 5. Windows CI on windows-latest
expected: The Windows conformance job runs green on GitHub's windows-latest runner (including `sftp-server.exe` availability), not only on the Azure host.
result: pass (draft PR #21, 2026-10-19). G2 fixed by 23-23 (staging surveyed from the account home, honest mkdir text) after 23-22's marker-led listings. CI run 39219142462 on head 9d068e5: check (ubuntu-latest), check (macos-latest), check (windows-latest) and keyring-smoke all success (ubuntu failed once on the load-sensitive `contract_mcp::mcp_tools_call_write_verbs_unknown`, one `gh run rerun --failed`, then green). windows-latest `deploy_host_conformance`: 47 passed, 0 failed, every conformance_ps_* row and ps_* test ok. The docs-only head ce4aec0 (run 39220703311) is green on all four jobs too, and so is head 6fa9d11 after the listing hardening and the WR-01/WR-02 fixes (run 39289206545, windows-latest `deploy_host_conformance` 47 passed, 0 failed). Recorded in 23-LIVE-CAPTURES.md `## Windows CI (windows-latest)`.

### 6. Rig live gates after the review fixes
expected: `live_deploy_apply -- --ignored` passes on the local 8.3.6 rig with CR-01, WR-01 and WR-04 in place.
result: pass — 4 passed, 0 failed, 2026-10-18T19:24:05Z to 19:25:20Z (live_apply_then_rollback, live_kill_after_green_verify_then_recover, live_kill_mid_write_then_recover, live_lock_lapse_restores)

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

- truth: "`ign deploy plan`, `apply` and `rollback --run` over ssh to a live Linux Ignition host commit, verify and restore like the Windows host."
  status: resolved (23-22 fix and five-gate run #2, 5 of 5 on ign-lnx-836, 2026-10-19T02:22:49Z; listing hardened to `ls -lan` with a required prefix by debug sftp-listing-parser-diff, 206998e, re-captured live with the real parser; re-verified 2026-10-19)
  reason: "FA-23-05, gate `lnxvm_apply_script_run` (also `lnxvm_sh_commit_time_budget`, `lnxvm_conformance_table_on_host`, `lnxvm_cli_apply_rollback`). The POSIX sftp listing parser (`ListFormat::Prefixed`, `crates/ignition-core/src/deploy/transport/sftp.rs`) requires the server to print `<dir>/<name>` in an `ls -la` line; the OpenSSH 8.9p1 `sftp-server` of Ubuntu 22.04 prints bare names (as Win32-OpenSSH does), so the first line fails. Exact text: `transport failed (read) on nodes.master: cannot use the sftp listing: a listing line does not carry a listed directory prefix: "drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:43 ."` (read, from `ign deploy plan`, exit 4) and `prepare ... drwxr-xr-x    4 root     root         4096 Oct 18 22:52 .` (apply half)."
  severity: blocker
  test: 2
  artifacts: [".planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md ## Live gate: Linux ssh host (FA-23-05)"]
  missing: ["A listing format that does not need the server to prefix names (the Marked pwd-marker form, or detection) for POSIX hosts; then a rerun of the four gates"]

- truth: "The deploy_host_conformance PowerShell rows pass on GitHub's windows-latest runner, not only on the Azure host."
  status: resolved (23-23 fix, 23-24 CI proof: run 39219142462 head 9d068e5, windows-latest 47 of 47; docs head ce4aec0 run 39220703311 all green)
  reason: "CI run 39175065312, job check (windows-latest): 18 of 47 tests in deploy_host_conformance fail (all conformance_ps_* rows and the ps_* tests), every one with the same prepare error: `TransportFailed { operation: \"prepare\", node: \"nodes.test\", detail: \"exit code 1 (refused with no reason: a held or non-empty target): remote mkdir \\\"/C:/Users/RUNNER~1\\\": Failure\" }`. The runner's temp dir is under the 8.3 short name C:\\Users\\RUNNER~1; the sftp mkdir-all walk tries to create that existing component and the failure is misclassified as a held or non-empty target. Root cause unconfirmed: test-setup only, or an engine gap if an account home can appear in 8.3 form. First time these tests ran on windows-latest (the file is new in phase 23)."
  severity: blocker
  test: 5
  artifacts: ["https://github.com/TheThoughtagen/ignition-cli/actions/runs/39175065312"]
  missing: ["Root cause on a Windows host (the Azure VM ign-win-836 can reproduce with an 8.3 path)", "A fix (mkdir-all tolerant of an existing component, or long-path normalization) plus a correct classification of the error", "A green windows-latest run"]
