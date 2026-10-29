---
status: complete
phase: 22-transport-plan-pull-verify
source: [22-VERIFICATION.md, 22-15-SUMMARY.md, 22-16-SUMMARY.md]
started: 2026-10-15T18:43:48Z
updated: 2026-10-15T21:30:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Windows CI leg builds and tests the cfg(windows) code (re-test after G-22-1)
expected: CI on origin/main head ab8608a is green on all four jobs, including check (windows-latest). Run 38838954722 shows success for keyring-smoke, check (ubuntu-latest), check (windows-latest), check (macos-latest).
result: pass
previous: issue (G-22-1, resolved by 22-16)

### 2. Pulled tree verifies green, probes still gate (re-test after G-22-2)
expected: With the ign-cli WebDev bundle installed and one Good verify.tag_probes[] entry, run `ign deploy pull` into a fresh dir, then `ign deploy verify` on the UNMODIFIED pulled tree. Result: exit 0, data.ok true, read_back passes with no finding on ignition/cobranding, probes pass. Then add a Bad probe (e.g. [default]UAT22/Missing): finding tag_probe_not_good, exit 6. Run `cargo install --path crates/ignition-cli --force` first so `ign` is the fixed build.
result: pass
source: agent-run on user request ("do it for me")
evidence: "fresh ign-deploy-rig (8.3.6, production mode), ign 1.5.0 @ HEAD, ign-cli WebDev bundle deployed. deploy pull: exit 0, 91 files, cobranding config.json holds 9 null members. verify on unmodified tree: exit 0, data.ok true, 28 resources, read_back pass, probes warn (none declared). + probe [System]Gateway/CurrentDateTime: exit 0, all six checks pass. + probe [default]UAT22/Missing: exit 6, data.ok false, deploy_verify_failed, tag_probe_not_good quality Bad, read_back still pass."
previous: issue (G-22-2, resolved by 22-15)

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

```yaml
- gap_id: G-22-1
  resolved_by: 22-16-PLAN.md
  resolved_at: 2026-10-15
  truth: "CI (ubuntu, windows, macos) is green on the Phase 22 head, so the cfg(windows) client code compiles and its tests pass on windows-latest"
  status: resolved
  reason: "User reported: CI run 38796885290 on 395f8ee: check (ubuntu-latest) failed at cargo clippy -D warnings with 2 needless_borrows_for_generic_args errors (crates/ignition-core/src/deploy/ledger.rs:185 `.map_err(&bad)`, crates/ignition-core/src/deploy/transport/remote.rs:994 `.map_err(&failure)`); fail-fast cancelled windows-latest and macos-latest, so the cfg(windows) code was never compiled. Local rustc 1.94.1 clippy does not flag them; CI uses stable."
  severity: blocker
  test: 1
  root_cause: "CI stable clippy (newer than local rustc 1.94.1) denies clippy::needless_borrows_for_generic_args on `.map_err(&bad)` (ledger.rs:185) and `.map_err(&failure)` (remote.rs:994): the closures are Copy, so the borrow is needless. Under -D warnings the ubuntu check fails and fail-fast cancels windows-latest."
  artifacts: [crates/ignition-core/src/deploy/ledger.rs, crates/ignition-core/src/deploy/transport/remote.rs, .github/workflows/ci.yml]
  missing: ["drop the borrow at both call sites; re-run CI so windows-latest actually compiles and tests the cfg(windows) code"]
- gap_id: G-22-2
  resolved_by: 22-15-PLAN.md
  resolved_at: 2026-10-15
  truth: "A tree produced by `ign deploy pull` from a live gateway verifies green with `ign deploy verify` against that same gateway (read_back compares equal), so a green verdict with Good tag probes is reachable without hand-editing the tree"
  status: resolved
  reason: "User reported: Run on the local ign-deploy-rig (Ignition 8.3.6, ign 1.5.0 @ 395f8ee, ign-cli WebDev bundle 1.3.0 installed). Probes behave as specified: one Good probe -> probes check pass; adding [default]UAT22/Missing -> probes fail, finding tag_probe_not_good 'quality Bad', exit 6, envelope data.ok=false slug deploy_verify_failed. But an unmodified tree from `ign deploy pull` does NOT verify green: read_back fails resource_differs on ignition/cobranding (differs at appIcon, appIconName, appIconSize, favicon, faviconName, faviconSize, logo, logoName, logoType), exit 6. Green (exit 0, 27 resources) only after removing ignition/cobranding from the tree. Contradicts README claim that a pulled tree compares equal."
  severity: major
  test: 2
  root_cause: "verify's read_back applies the N1 null rule one-sided: check_read_back (verify.rs ~610/612) and diff_attributes (~776) strip null members from the file config only, then diff_exact against the raw REST config. Ignition 8.3.6 serves ignition/cobranding's 9 unset image fields as explicit null in REST (same nulls as the pulled file), so after the file-side strip those keys exist only on the REST side and are reported as differing. Pull is byte-exact and correct. Tests missed it because the mock rest_config helpers strip nulls; CV2 (22-09) compared raw values; no live pull->verify ran before UAT."
  debug_session: .planning/debug/resolved/pulled-tree-cobranding-readback.md
  artifacts: [crates/ignition-core/src/deploy/verify.rs, crates/ignition-core/tests/deploy_verify.rs, crates/ignition-cli/tests/contract_deploy.rs, README.md, .planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md]
  missing: ["symmetric N1: strip null members at every depth from the REST config and REST attribute values before comparing (null == absent on both sides; file null vs non-null REST and REST null vs file value still fail)", "regression tests: cobranding-shape REST explicit nulls matching file -> pass; REST null vs file value -> fail; file null vs REST value -> fail", "update verify.rs N1 doc, README 2935-2946 'compares equal' text, CV2 table cobranding row", "live gate: pull then verify on the pulled tree, not only the tracer tree"]
```
