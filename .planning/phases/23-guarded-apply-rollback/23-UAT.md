---
status: testing
phase: 23-guarded-apply-rollback
source: [23-VERIFICATION.md]
started: 2026-10-18T19:30:00Z
updated: 2026-10-18T21:30:00Z
---

## Current Test

number: 2
name: FA-23-05 — Linux ssh host (`commit.sh`) against a live Linux gateway
expected: |
  apply and rollback over ssh to a live Linux Ignition host commit, verify and restore like the Azure Windows host, including the WR-04 deadline stop.
awaiting: user response

## Tests

### 1. C2b — Designer or gateway-UI save while the scan-lock is held
expected: The save is blocked, or the run detects it and restores. The outcome is recorded in 23-LIVE-CAPTURES.md.
result: pass (recorded, 2026-10-18T21:17Z to 21:21Z, gateway web UI only). The lock queues the UI save: the UI shows 503 and the save is applied 0.3 s after release. It is not blocked. Recorded under 23-LIVE-CAPTURES.md `## C2b (manual)`. The Designer run (21:34Z to 21:38Z) found that project saves are not under the config lock and land immediately. They are outside the apply's write set (mode collection config only), so there is no conflict. Still owed: whether an apply detects a config save queued during its hold, covered by a live rig test in a quick task.

### 2. FA-23-05 — Linux ssh host (`commit.sh`) against a live Linux gateway
expected: `ign deploy apply` and `ign deploy rollback --run` over ssh to a live Linux Ignition host commit, verify and restore the same way the Azure Windows host does, including the WR-04 `deadline` stop.
result: [pending]

### 3. FA-23-06 — gateway backup type
expected: The user confirms the pre-apply `.gwbk` should use `BackupType::All`.
result: [pending]

### 4. FA-23-08 — scan-lock hold size
expected: The user accepts `HOLD_S = 176` s (sized from the replace commit, 85.496 s at 1,520 files) instead of the plan's 129 s, or asks to switch back (two literals in `hold.rs`).
result: [pending]

### 5. Windows CI on windows-latest
expected: The Windows conformance job runs green on GitHub's windows-latest runner (including `sftp-server.exe` availability), not only on the Azure host.
result: [pending]

### 6. Rig live gates after the review fixes
expected: `live_deploy_apply -- --ignored` passes on the local 8.3.6 rig with CR-01, WR-01 and WR-04 in place.
result: pass — 4 passed, 0 failed, 2026-10-18T19:24:05Z to 19:25:20Z (live_apply_then_rollback, live_kill_after_green_verify_then_recover, live_kill_mid_write_then_recover, live_lock_lapse_restores)

## Summary

total: 6
passed: 2
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
