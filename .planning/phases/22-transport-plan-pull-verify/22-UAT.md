---
status: complete
phase: 22-transport-plan-pull-verify
source: [22-VERIFICATION.md]
started: 2026-10-15T18:43:48Z
updated: 2026-10-15T20:16:20Z
---

## Current Test

[testing complete]

## Tests

### 1. Windows CI leg builds and tests the cfg(windows) code
expected: Push main; the windows-latest CI job is green. origin/main is 87 commits behind, so no Phase 22 code has run on Windows CI.
result: issue
reported: "CI run 38796885290 on 395f8ee: check (ubuntu-latest) failed at cargo clippy -D warnings with 2 needless_borrows_for_generic_args errors (crates/ignition-core/src/deploy/ledger.rs:185 `.map_err(&bad)`, crates/ignition-core/src/deploy/transport/remote.rs:994 `.map_err(&failure)`); fail-fast cancelled windows-latest and macos-latest, so the cfg(windows) code was never compiled. Local rustc 1.94.1 clippy does not flag them; CI uses stable."
severity: blocker

### 2. Live tag-probe verify (DPLN-08)
expected: Against a gateway with the ign-cli WebDev bundle installed and verify.tag_probes[] declared, `ign deploy verify` reports every probe Good (green) and a bad-quality probe turns the verdict red with exit 6.
result: issue
reported: "Run on the local ign-deploy-rig (Ignition 8.3.6, ign 1.5.0 @ 395f8ee, ign-cli WebDev bundle 1.3.0 installed). Probes behave as specified: one Good probe -> probes check pass; adding [default]UAT22/Missing -> probes fail, finding tag_probe_not_good 'quality Bad', exit 6, envelope data.ok=false slug deploy_verify_failed. But an unmodified tree from `ign deploy pull` does NOT verify green: read_back fails resource_differs on ignition/cobranding (differs at appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType), exit 6. Green (exit 0, 27 resources) only after removing ignition/cobranding from the tree. Contradicts README claim that a pulled tree compares equal."
severity: major

## Summary

total: 2
passed: 0
issues: 2
pending: 0
skipped: 0
blocked: 0

## Gaps

```yaml
- gap_id: G-22-1
  truth: "CI (ubuntu, windows, macos) is green on the Phase 22 head, so the cfg(windows) client code compiles and its tests pass on windows-latest"
  status: failed
  reason: "User reported: CI run 38796885290 on 395f8ee: check (ubuntu-latest) failed at cargo clippy -D warnings with 2 needless_borrows_for_generic_args errors (crates/ignition-core/src/deploy/ledger.rs:185 `.map_err(&bad)`, crates/ignition-core/src/deploy/transport/remote.rs:994 `.map_err(&failure)`); fail-fast cancelled windows-latest and macos-latest, so the cfg(windows) code was never compiled. Local rustc 1.94.1 clippy does not flag them; CI uses stable."
  severity: blocker
  test: 1
  root_cause: "CI stable clippy (newer than local rustc 1.94.1) denies clippy::needless_borrows_for_generic_args on `.map_err(&bad)` (ledger.rs:185) and `.map_err(&failure)` (remote.rs:994): the closures are Copy, so the borrow is needless. Under -D warnings the ubuntu check fails and fail-fast cancels windows-latest."
  artifacts: [crates/ignition-core/src/deploy/ledger.rs, crates/ignition-core/src/deploy/transport/remote.rs, .github/workflows/ci.yml]
  missing: ["drop the borrow at both call sites; re-run CI so windows-latest actually compiles and tests the cfg(windows) code"]
- gap_id: G-22-2
  truth: "A tree produced by `ign deploy pull` from a live gateway verifies green with `ign deploy verify` against that same gateway (read_back compares equal), so a green verdict with Good tag probes is reachable without hand-editing the tree"
  status: failed
  reason: "User reported: Run on the local ign-deploy-rig (Ignition 8.3.6, ign 1.5.0 @ 395f8ee, ign-cli WebDev bundle 1.3.0 installed). Probes behave as specified: one Good probe -> probes check pass; adding [default]UAT22/Missing -> probes fail, finding tag_probe_not_good 'quality Bad', exit 6, envelope data.ok=false slug deploy_verify_failed. But an unmodified tree from `ign deploy pull` does NOT verify green: read_back fails resource_differs on ignition/cobranding (differs at appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType), exit 6. Green (exit 0, 27 resources) only after removing ignition/cobranding from the tree. Contradicts README claim that a pulled tree compares equal."
  severity: major
  test: 2
  artifacts: []
  missing: []
```
