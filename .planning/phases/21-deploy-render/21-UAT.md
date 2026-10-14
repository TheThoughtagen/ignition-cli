---
status: complete
phase: 21-deploy-render
source: [21-VERIFICATION.md]
started: 2026-10-13T16:08:40Z
updated: 2026-10-14T00:51:56Z
---

## Current Test

[testing complete]

## Tests

### 1. DRND-05 cross-OS byte fidelity (SC3)
expected: Push or open a PR; ci.yml is green on all three OS legs for a commit carrying deploy_corpus.rs. Record the run id.
result: pass
evidence: CI run 38483193605 on ff07c85 (ubuntu, macos, windows green; corpus_render_is_byte_identical ok on all three)

### 2. WR-02 Referenced-wrapper fail-open decision
expected: Decide whether a malformed Referenced wrapper whose data carries a plaintext password (renders with exit 0 today) must be refused under DRND-07 before ship. Fix if in scope: is_referenced requires data to be exactly the string members providerName and secretName.
result: pass
evidence: fixed in quick task 261013-neg (e095293, 865fee4, 80a7f6c); CI run 38483193605 green

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
