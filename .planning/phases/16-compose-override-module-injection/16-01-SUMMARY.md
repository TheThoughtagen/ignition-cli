---
phase: 16-compose-override-module-injection
plan: 01
subsystem: rig-modules
tags: [compose, override, yaml, module, provisioning, tdd]
status: complete
dependency-graph:
  requires: [15-module-artifact-fetch-verify]
  provides: [compose-override-injection, module-provisioning-seam]
  affects: [ign-rig-up, ign-rig-down, ign-rig-status, ign-rig-logs, ign-rig-reset]
tech-stack:
  added: []
  patterns: [hand-rolled-yaml-writer, shared-arg-builder-helper, tri-state-override-lifecycle]
key-files:
  created:
    - crates/ignition-core/src/rig/modules.rs
    - crates/ignition-core/tests/rig_module_override.rs
  modified:
    - crates/ignition-core/src/rig/compose.rs
    - crates/ignition-core/src/rig/mod.rs
    - crates/ignition-core/src/actions/rig.rs
    - crates/ignition-core/src/config/profile.rs
    - crates/ignition-core/src/module/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-tui/src/workers/rig_stream.rs
    - README.md
decisions:
  - "D-07: paths single-quoted with apostrophe doubling (a backslash in a double-quoted YAML scalar is an escape and mangles C:\\Users\\...); env VALUES double-quoted (bare Y is a YAML boolean). Two distinct Windows traps, two different quoting rules."
  - "D-09: config_args NEVER receives the override — otherwise ign's own injected mount appears in rig status, the port pre-flight and the gateway-URL heuristic. Order is resolve (no override) -> provision -> act."
  - "D-11: ONE shared project_and_file_args helper for all four builders, so the override-after-base ordering cannot drift per builder."
  - "D-13: one new slug, ModuleNotRegistered (exit 3, module_not_registered). Every other refusal reuses rig_error (exit 7)."
metrics:
  completed: 2026-09-24
actuals:
  tasks: 3
  commits: 3
---

# Phase 16 Plan 01: Compose override module injection Summary

A rig declares a module; Phase 15 fetches and verifies it; `ign` writes
`compose.ign-modules.yml` into the rig's project directory and passes it as a
SECOND `-f` on every project-scoped compose invocation. The user's own compose
file is never touched.

## What Changed

**Task 1 (tracer, `5d56fd5`)** — `rig/modules.rs`: `generate_override` (pure,
deterministic, sorted by `ModuleSpec::id` so regeneration is byte-stable),
`write_override`, `provision_modules`, `ModuleProvisioning`. `ModuleSpec` gained
`gateway_module_id`; `RigPlan` gained `gateway_service` and `modules`;
`[rigs.NAME]` gained a `modules` table and `module_service`.

**Task 2 (refusals, `3eb5a8c`)** — `preflight_mount_source` (absolute → metadata
→ `is_file`), `write_override`'s D-08 tri-state (write / delete a stale override
/ `Ok(None)`), and `CoreError::ModuleNotRegistered`.

**Task 3 (`-f` helper + durability, `5831bd2`)** — `existing_override` wired into
`rig_down`/`rig_status`/`rig_logs` (verbs that never provision, so the file on
disk is the whole truth), all four builders routed through one shared helper, and
the `up`/`down`/`up` sequence.

## Why `gateway_module_id` Is Its Own Field

Verified from each artifact's `module.xml`, not inherited from documentation:

| Module | Registry slug | Gateway module id |
|---|---|---|
| Git | `git` | `com.axone_io.ignition.git` (reverse-DNS) |
| Project Scan Endpoint | `project-scan-endpoint` | `project-scan-endpoint` (bare slug) |

No derivation from the slug produces both. Getting this wrong is silent: the
acceptance variable simply names a module that does not exist, and the real
module stays quarantined with nothing in the logs pointing at the cause.

## Tests Are Falsifiable, Not Merely Green

Every guard was verified by perturbing what it protects, confirming the right
failure, then restoring. Selected results:

- Flipping `"Y"` to `'Y'` fails the golden test; stripping apostrophe doubling
  surfaces the un-escaped injection shape threat row T-16-01 exists to prevent.
- A tamper-write inside `write_override` fails `user_compose_file_is_never_modified`,
  which compares the base file's BYTES — a stronger gate than grepping for write
  calls.
- Disabling the `is_file` pre-flight lets a DIRECTORY provision successfully and
  an override be written: the live Pitfall-1 defect shape, and the exact reason
  `ignition-git-module`'s repo-root rig cannot boot.
- Inverting the `-f` order broke FIVE tests including two written in Task 1 —
  evidence the shared helper propagates the invariant to code that predates it.

**One test passed while its guard was removed.** The override-path-occupant test
survived deletion of the guard because `std::fs::write` happens to error on a
directory with a superficially similar message. It was strengthened to assert the
guard's own wording, re-run with the guard still removed to confirm it then
failed, and only then restored. A green test nobody has seen fail is not evidence.

## Deviations

- **Task 1 put its tests inline in `modules.rs`** rather than creating
  `tests/rig_module_override.rs` as the plan specified, so Task 1's own `<verify>`
  line would have errored "no test target". Task 2 created the file; the gap is
  closed going forward but Task 1 shipped without it.
- **Task 1's first executor stalled mid-task**, leaving ~400 lines of
  implementation, zero tests, a non-compiling tree and nothing committed. Recovery
  took four fixes (a premature `existing_override` import belonging to Task 3, a
  `ModuleSpec` literal missing the new field, a `manual_option_as_slice` lint, and
  an SC-1 round-trip test that compared a full `Config` against an input fragment
  and so could never have passed). A scoped follow-up executor then wrote the three
  missing Task 1 tests.
- **`rig_reset`'s teardown half was not passing the override**, so the two halves
  of one reset cycle disagreed about the file set. Task 1's doc comment already
  claimed they matched; Task 3 made it true.

## Verification

All five CI gates, run by the orchestrator independently of the executors'
reports: `fmt` 0 · `clippy --workspace --all-targets -D warnings` 0 ·
`build --workspace` 0 · `build -p ignition-cli --no-default-features` 0 ·
`test --workspace` 0 (68 suites). `rig_module_override` 12/12, `rig::compose`
24/24, `rig::modules` 2/2. `Cargo.lock` unchanged; `crates/ignition-cli`
untouched (plan 16-02 owns the CLI surface).

## What This Plan Does NOT Prove

Scoped Docker-free deliberately, so its failure modes are visible without a
gateway. Consequently:

- **No module has loaded into a real gateway.** SC-2 is untouched; every test here
  uses `FakeRunner` or wiremock.
- **The comma-joined acceptance variables are an assumption.** `ACCEPT_MODULE_CERTS: "a,b"`
  is pinned by a golden test, which makes it stable, not correct. If the gateway
  wants another separator, the test still passes and both modules stay quarantined.
- **The `gateway_service` heuristic** (first service publishing a port targeting
  8088, then 443) has never met a real rig's compose file.
- **SC-3 is half-proven**: the override file survives `down`/`up` on disk; whether
  the MOUNT survives a container recreate is the live half.

Plan 16-03's live gate is where these are confirmed or falsified.
