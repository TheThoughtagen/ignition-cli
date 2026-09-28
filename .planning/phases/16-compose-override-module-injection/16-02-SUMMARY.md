---
phase: 16-compose-override-module-injection
plan: 02
subsystem: rig-modules
tags: [registry, cli, clap, fetch-policy, json-envelope]
status: complete
dependency-graph:
  requires: [16-01]
  provides: [module-registry-second-entry, with-module-flag, provisioned-modules-envelope]
  affects: [ign-rig-up, ign-rig-reset]
tech-stack:
  added: []
  patterns: [registry-parameterized-tests, single-named-join-helper, empty-map-short-circuit]
key-files:
  created: []
  modified:
    - crates/ignition-core/src/module/mod.rs
    - crates/ignition-core/src/rig/modules.rs
    - crates/ignition-core/tests/rig_module_override.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/tests/contract_rig.rs
    - crates/ignition-core/src/actions/rig.rs
    - README.md
decisions:
  - "D-16: --with-module merges with config, flag winning per id. No subtractive form; stated in the flag's help text so the absence is discoverable."
  - "D-17: --refresh maps to FetchPolicy::Refresh, --accept-upstream-change to AcceptUpstreamChange. Accept is NEVER implied by refresh — collapsing them would silently undo Phase 15's D-02."
  - "D-19: no new exit-code slug. Malformed flag values reuse invalid_input (exit 2); unregistered ids reuse module_not_registered (exit 3) from 16-01."
metrics:
  completed: 2026-09-25
actuals:
  tasks: 3
  commits: 4
---

# Phase 16 Plan 02: Registry second module + CLI surface Summary

`bw-design-group/ignition-project-scan-endpoint` is registered alongside the Git
module, `ign rig up --with-module git@2.3.4` works end to end, and the README
documents the surface.

## What Changed

**Task 1 (`aa99eef`)** — `PROJECT_SCAN_ENDPOINT` registered; registry tests
parameterized over `MODULES` so a third module is covered the day it is added;
`join_gateway_module_ids` extracted as the single named helper for the
comma-join assumption; two-module and Windows-path goldens.

**Task 2 (`6f7051d`)** — `--with-module ID@VERSION` on `rig up`/`reset`, the
dispatch wiring, `provisioned_modules` on both result envelopes, and the two
fetch-policy flags.

**Task 3 (`75805f3`)** — the README section.

## Why the Second Module Proves Something

It differs from the Git module in every column, and each difference is a place
the registry could have hardcoded an assumption:

| | Git | Project Scan Endpoint |
|---|---|---|
| Org | WhiskeyHouse | bw-design-group |
| Asset filename | `Git-{version}-signed.modl` | `Project-Scan-Endpoint.modl` (no version) |
| Gateway module id | `com.axone_io.ignition.git` (reverse-DNS) | `project-scan-endpoint` (bare slug) |
| Size | 7.7 MB | 30 KB |
| Min gateway | 8.3.1 | 8.3.0 |

Its identity was verified by downloading and unzipping the artifact:
`certificates.p7b` present (signed), `module.xml` `<id>` read directly, digest
matching the published `sha256:f0ffb9cf…`.

## The SC-5 Gate Could Not Pass As Written

The plan's `<verify>` grep — "must print 0" for module identifiers under
`crates/ignition-core/src/rig/` — was unsatisfiable before and after this plan.
Two legitimate causes: `rig/mod.rs:111`'s `GIT_MODULE_RELPATH` (the Phase 4
rig-DISCOVERY constant, unrelated to provisioning, and the very constant phase 18
removes), and golden-test fixtures, which by construction name the ids they pin.

Scanning production code only (above each `#[cfg(test)]` boundary), `rig/` has
exactly one match — that unrelated constant — and `rig/modules.rs` and
`actions/rig.rs` have zero. **SC-5 holds; the gate does not.** Recorded in the
plan (`304873d`).

The plan-checker had marked SC-5 PASS by confirming the gate existed without
running it. The real protection is the test: injecting
`if id == "project-scan-endpoint"` into `provision_modules` raised the grep count
13→15 AND failed `provisioning_is_registry_driven_for_both_modules`.

## SC-1 Is Proven at the Dispatch Layer

The empty-merged-map short-circuit is a single visible early branch
(`main.rs:2101`): an undeclared rig builds no `ModuleFeed`, resolves no cache
root, and constructs no network client. Not asserted — the branch is structural,
and the binary-level contract test pins an empty `provisioned_modules` for an
unprovisioned rig.

## Security Review Finding — Real, Already Fixed

An automated review flagged `module_fetch_policy` as conflating the flags
(`if accept_upstream_change || refresh`). That was true of an intermediate state
and had already been corrected before the stall. Verified in the tree: flags map
independently, and `fetch.rs:331` shows `FetchPolicy::Refresh` returning
`ModuleDigestChanged` (exit 6) on a digest change rather than silently accepting.

Had the conflated form survived, `--refresh` would have silently accepted
re-released artifacts — undoing Phase 15's deliberate two-step. The review
independently landed on the risk flagged when the task was dispatched.

## Deviations and Recovery

**The executor stalled mid-falsifiability-testing with nothing committed** — the
second such stall in this phase, both times after the work and before the commit,
despite an explicit instruction to commit first. Recovery was by hand:

- Verified no perturbation survived by inspecting `parse_with_module_flag`
  directly rather than trusting the absence of marker comments.
- `merge_declarations_refuses_malformed_values` grouped `"@2.3.4"`, `"@"` and
  `"git@"` as parser-level refusals asserting the whole raw value appears in the
  message. Those DO split on `@` into an empty half, so a validator refuses them
  and names the offending half instead; only the missing-separator case is caught
  by the parser. Regrouped by which layer refuses, asserting the validator's own
  wording rather than an empty-string fragment that would match anything.
- Collapsing that group to one element left a `clippy::single_element_loop`
  (my own error, caught locally by running CI's full gate set).

## Verification

All five CI gates, run by the orchestrator independently of the executor's
report: `fmt` 0 · `clippy` 0 · `build` 0 · `no-default-features` 0 ·
`test` 0 (68 suites). `rig::modules` 15/15, `contract_rig` 19/19,
`readme_exit_table_agreement` green, exit-code table untouched (D-19).

## What This Plan Does NOT Prove

Still no Docker, so the phase's central claim remains untested:

- **No module has loaded into a real gateway.** SC-2 untouched.
- **The comma-joined accept vars remain an assumption** pinned by a golden test.
  Stable is not correct; if the gateway wants another separator, every test here
  still passes and both modules stay quarantined.
- **The `gateway_service` heuristic** has never met a real compose file.
- **SC-3 is half-proven** — the override file survives `down`/`up` on disk; the
  mount surviving a container recreate is the live half.

Plan 16-03 is where these are confirmed or falsified.
