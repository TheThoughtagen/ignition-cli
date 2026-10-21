---
phase: 24-redundant-pair
plan: 04
subsystem: cli
tags: [ignition, redundancy, active-node, clap, wiremock]
status: complete

requires:
  - phase: 24-redundant-pair
    provides: V8 = a (`--peer <PROFILE>`) from the 24-02 contract decisions; the Phase 20 `active_node` library
provides:
  - "ActiveNodeReport {active, this_role, peer_role, peer_profile, peer}"
  - "RedundancyStatusResult.active_node (omitted, not null, without --peer)"
  - "redundancy_status_pair(this, peer, peer_profile)"
  - "ign redundancy status --peer PROFILE and the `active node:` human line"
affects: [24-07, 24-10]

actuals:
  tokens: 3900
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "second node resolved through Session::resolve_side (its own secret chain); active node through Session::resolve_loaded on the same loaded config"

key-files:
  created:
    - .planning/phases/24-redundant-pair/deferred-items.md
  modified:
    - crates/ignition-core/src/actions/redundancy.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_diagnostics.rs
    - README.md

key-decisions:
  - "The active profile is resolved with Session::resolve_loaded on a config the dispatch arm loads itself, so the same config feeds named_profile_client for the peer; the no-peer arm keeps today's Session::resolve path untouched"
  - "The same-profile refusal compares --peer to the resolved active profile name after resolving the active session but before the peer session or any request"

requirements-completed: []

duration: not timed
completed: 2026-10-19
---

# Phase 24 Plan 04: `ign redundancy status --peer` Summary

**`ign redundancy status --peer <PROFILE>` reads a second node through its own profile and adds `data.active_node` (`this`, `peer`, `both` or `neither`), decided by `activityLevel` through the Phase 20 `active_node` library and never by `role`; without `--peer` the output is unchanged.**

## What was built

- `actions/redundancy.rs`: `ActiveNodeReport`, `RedundancyStatusResult.active_node` (`skip_serializing_if = "Option::is_none"`), and `redundancy_status_pair`, which maps `ActiveNode::{OnlyA, OnlyB, Both, Neither}` to `this`, `peer`, `both`, `neither`. No comparison on `role` exists in the file (acceptance grep prints nothing); `role` is copied through for display only.
- `cli.rs`: `RedundancyCommand::Status { peer: Option<String> }`, `--peer <PROFILE>`.
- `main.rs`: two arms. `peer: None` is the old path verbatim. `peer: Some(p)` loads the config, resolves the active session, refuses `p == active profile` as `InvalidInput` (exit 2) before any request, resolves the peer with `named_profile_client`, and calls `redundancy_status_pair`. The envelope `profile` stays the active profile.
- `render.rs`: after today's lines, `peer (<profile>): role <r>, activity <a>` and one of `active node: this (Master)`, `active node: peer (Backup)`, `active node: both report Active (split-brain)`, `active node: neither reports Active`.
- README: the `ign redundancy status [--peer PROFILE]` row now documents the flag, `active_node`, the split-brain meaning of `both`, the healthy Cold standby and the same-profile refusal.
- Tests (`contract_diagnostics.rs`, verbatim C-S3 bodies for the pair): `redundancy_status_peer_names_the_active_node`, `_after_failover`, `_split_brain`, `_neither`, `_same_profile_refused` (zero requests recorded by wiremock), `_human`, and `redundancy_status_without_peer_is_unchanged` (data key set is exactly `["status"]`).

## Task commits

1. Task 1 (tracer): `521ccfd` feat(24-04): ign redundancy status --peer names the active node (tracer)
2. Task 2: `933ed18` test(24-04): split-brain, neither, same-profile refusal, human line, README row

The tracer's `<verify>` (`cargo test -p ignition-cli --test contract_diagnostics redundancy_status`) is fully automated and passed (4 tests), so execution continued into Task 2.

## Verification

- `cargo test -p ignition-cli --test contract_diagnostics redundancy_status`: 8 pass (`redundancy_status_envelope`, `redundancy_status_without_peer_is_unchanged` and the six `_peer_` tests).
- `cargo test -p ignition-cli --test contract_mcp` (15 pass, 1 ignored) and `--test tui_coverage` (5 pass): no pin on the redundancy tool's parameters or flags needed updating.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --no-fail-fast`: 2547 passed, 1 failed, 69 ignored. The one failure is `ignition-core/tests/live_gateway.rs::guard_drop_during_unwind_inside_runtime_still_cleans_up`, which failed in both full-workspace runs but passes when run alone. It is a timing-sensitive Drop-cleanup test unrelated to redundancy; logged in `deferred-items.md`, not fixed (scope boundary).

## Deviations from Plan

### Auto-fixed Issues

None needing a rule: the plan was executed as written.

### Process note

- Implementation for Task 1 was written before its tests, so no failing-test (RED) run or commit exists for either task; the tests were added afterwards and pass. Every behaviour in the plan is asserted by a passing test.

## Known Stubs

None.

## Threat Flags

None. The peer's credentials resolve through `Session::resolve_side` (T-24-13) and are never printed; the envelope names profiles only. T-24-12 and T-24-14 are covered by the four-outcome tests and the same-profile refusal test.

## Self-Check: PASSED

- `crates/ignition-core/src/actions/redundancy.rs`, `crates/ignition-cli/tests/contract_diagnostics.rs`, README row (`grep -c` prints 1): found.
- Commits `521ccfd` and `933ed18`: present in `git log`.
