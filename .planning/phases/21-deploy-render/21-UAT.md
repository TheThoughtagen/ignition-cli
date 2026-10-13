---
status: testing
phase: 21-deploy-render
source: [21-VERIFICATION.md]
started: 2026-10-13T16:08:40Z
updated: 2026-10-13T16:08:40Z
---

## Current Test

number: 1
name: DRND-05 cross-OS byte fidelity (SC3)
expected: |
  A green ci.yml run on ubuntu, macos and windows for a commit carrying crates/ignition-core/tests/deploy_corpus.rs. Record the run id, then flip DRND-05 to Complete.
awaiting: user response

## Tests

### 1. DRND-05 cross-OS byte fidelity (SC3)
expected: Push or open a PR; ci.yml is green on all three OS legs for a commit carrying deploy_corpus.rs. Record the run id.
result: [pending]

### 2. WR-02 Referenced-wrapper fail-open decision
expected: Decide whether a malformed Referenced wrapper whose data carries a plaintext password (renders with exit 0 today) must be refused under DRND-07 before ship. Fix if in scope: is_referenced requires data to be exactly the string members providerName and secretName.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
