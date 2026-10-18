---
status: testing
phase: 23-guarded-apply-rollback
source: [23-VERIFICATION.md]
started: 2026-10-18T19:30:00Z
updated: 2026-10-18T23:50:00Z
---

## Current Test

number: 5
name: Windows CI on windows-latest
expected: |
  The Windows conformance job runs green on GitHub's windows-latest runner (including `sftp-server.exe` availability), not only on the Azure host.
awaiting: user response

## Tests

### 1. C2b — Designer or gateway-UI save while the scan-lock is held
expected: The save is blocked, or the run detects it and restores. The outcome is recorded in 23-LIVE-CAPTURES.md.
result: pass (recorded, 2026-10-18T21:17Z to 21:21Z, gateway web UI only). The lock queues the UI save: the UI shows 503 and the save is applied 0.3 s after release. It is not blocked. Recorded under 23-LIVE-CAPTURES.md `## C2b (manual)`. The Designer run (21:34Z to 21:38Z) found that project saves are not under the config lock and land immediately. They are outside the apply's write set (mode collection config only), so there is no conflict. Rig test `live_queued_config_save_during_apply` (2026-10-18T22:09Z): a config save queued during the apply's hold is refused as stale (`resource_signature_rejected`) because the engine's release scan reloaded the deployed copy first; the apply reported applied with the declared config on disk and in REST and a green fresh verify. Recorded under 23-LIVE-CAPTURES.md `## C2b (manual)`.

### 2. FA-23-05 — Linux ssh host (`commit.sh`) against a live Linux gateway
expected: `ign deploy apply` and `ign deploy rollback --run` over ssh to a live Linux Ignition host commit, verify and restore the same way the Azure Windows host does, including the WR-04 `deadline` stop.
result: issue (2026-10-18T23:36Z to 23:45Z, ign-lnx-836, Ubuntu 22.04, OpenSSH 8.9p1 sftp-server). The ssh path works (pinned key and host key, stub, home by sftp pwd; session umask 0002, uid 1001), but every sftp directory listing fails: `transport_failed ... cannot use the sftp listing: a listing line does not carry a listed directory prefix: "drwxr-xr-x    4 root     root         4096 Oct 18 22:52 ."`. Gates: `lnxvm_apply_script_run` FAIL at prepare, `lnxvm_sh_commit_time_budget` FAIL at prepare, `lnxvm_conformance_table_on_host` 28 passed / 20 failed of 48 (same error), `lnxvm_cli_apply_rollback` FAIL at `ign deploy plan` (exit 4) before any write; production and the home were left as found. Recorded under 23-LIVE-CAPTURES.md `## Live gate: Linux ssh host (FA-23-05)`.

### 3. FA-23-06 — gateway backup type
expected: The user confirms the pre-apply `.gwbk` should use `BackupType::All`.
result: pass (user decision 2026-10-18: keep `BackupType::All`).

### 4. FA-23-08 — scan-lock hold size
expected: The user accepts `HOLD_S = 176` s (sized from the replace commit, 85.496 s at 1,520 files) instead of the plan's 129 s, or asks to switch back (two literals in `hold.rs`).
result: pass (user decision 2026-10-18: keep `HOLD_S = 176` s, 1,527-file ceiling).

### 5. Windows CI on windows-latest
expected: The Windows conformance job runs green on GitHub's windows-latest runner (including `sftp-server.exe` availability), not only on the Azure host.
result: [pending]

### 6. Rig live gates after the review fixes
expected: `live_deploy_apply -- --ignored` passes on the local 8.3.6 rig with CR-01, WR-01 and WR-04 in place.
result: pass — 4 passed, 0 failed, 2026-10-18T19:24:05Z to 19:25:20Z (live_apply_then_rollback, live_kill_after_green_verify_then_recover, live_kill_mid_write_then_recover, live_lock_lapse_restores)

## Summary

total: 6
passed: 4
issues: 1
pending: 1
skipped: 0
blocked: 0

## Gaps

- truth: "`ign deploy plan`, `apply` and `rollback --run` over ssh to a live Linux Ignition host commit, verify and restore like the Windows host."
  status: failed
  reason: "FA-23-05, gate `lnxvm_apply_script_run` (also `lnxvm_sh_commit_time_budget`, `lnxvm_conformance_table_on_host`, `lnxvm_cli_apply_rollback`). The POSIX sftp listing parser (`ListFormat::Prefixed`, `crates/ignition-core/src/deploy/transport/sftp.rs`) requires the server to print `<dir>/<name>` in an `ls -la` line; the OpenSSH 8.9p1 `sftp-server` of Ubuntu 22.04 prints bare names (as Win32-OpenSSH does), so the first line fails. Exact text: `transport failed (read) on nodes.master: cannot use the sftp listing: a listing line does not carry a listed directory prefix: "drwxr-x---    4 svc-ignition-deploy svc-ignition-deploy     4096 Oct 18 23:43 ."` (read, from `ign deploy plan`, exit 4) and `prepare ... drwxr-xr-x    4 root     root         4096 Oct 18 22:52 .` (apply half)."
  severity: blocker
  test: 2
  artifacts: [".planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md ## Live gate: Linux ssh host (FA-23-05)"]
  missing: ["A listing format that does not need the server to prefix names (the Marked pwd-marker form, or detection) for POSIX hosts; then a rerun of the four gates"]
