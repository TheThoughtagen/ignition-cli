---
status: testing
phase: 23-guarded-apply-rollback
source: [23-VERIFICATION.md]
started: 2026-10-18T19:30:00Z
updated: 2026-10-18T19:30:00Z
---

## Current Test

number: 1
name: C2b — Designer or gateway-UI save while the scan-lock is held
expected: |
  With an `ign deploy apply` holding the scan-lock on the rig or the Azure host, a Designer
  (or gateway-UI) save of a resource the plan touches does not land silently over the deploy:
  either the save is blocked or the run detects it and restores. Record the observed behavior
  in 23-LIVE-CAPTURES.md. Until recorded, the Designer preflight is the only assumed guard.
awaiting: user response

## Tests

### 1. C2b — Designer or gateway-UI save while the scan-lock is held
expected: The save is blocked, or the run detects it and restores. The outcome is recorded in 23-LIVE-CAPTURES.md.
result: [pending]

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
passed: 1
issues: 0
pending: 5
skipped: 0
blocked: 0

## Gaps
