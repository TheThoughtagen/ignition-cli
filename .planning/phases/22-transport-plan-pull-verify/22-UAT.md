---
status: testing
phase: 22-transport-plan-pull-verify
source: [22-VERIFICATION.md]
started: 2026-10-15T18:43:48Z
updated: 2026-10-15T18:43:48Z
---

## Current Test

number: 1
name: Windows CI leg builds and tests the cfg(windows) code
expected: |
  After pushing main, the windows-latest CI job compiles and passes, covering the
  cfg(windows) client branches (icacls key ACL, NUL, process and tree paths).
awaiting: user response

## Tests

### 1. Windows CI leg builds and tests the cfg(windows) code
expected: Push main; the windows-latest CI job is green. origin/main is 87 commits behind, so no Phase 22 code has run on Windows CI.
result: [pending]

### 2. Live tag-probe verify (DPLN-08)
expected: Against a gateway with the ign-cli WebDev bundle installed and verify.tag_probes[] declared, `ign deploy verify` reports every probe Good (green) and a bad-quality probe turns the verdict red with exit 6.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
