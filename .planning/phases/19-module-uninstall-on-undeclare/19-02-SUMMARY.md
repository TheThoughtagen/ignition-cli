---
phase: 19-module-uninstall-on-undeclare
plan: 02
subsystem: rig-modules
tags: [orphan-report, undeclare, override-lifecycle, tui-parity]
status: complete
dependency-graph:
  requires: [19-01]
  provides: [orphaned-modules, undeclared-provisioning, clear-override]
  affects: [19-03]
tech-stack:
  added: []
  patterns: [report-never-remove, feed-free-cleanup, config-name-on-plan]
key-files:
  created: []
  modified:
    - crates/ignition-core/src/rig/modules.rs
    - crates/ignition-core/src/rig/mod.rs
    - crates/ignition-core/src/actions/rig.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-tui/src/workers/rig_stream.rs
    - crates/ignition-core/tests/rig_module_override.rs
    - README.md
decisions:
  - "rig up REPORTS orphans, it never removes them. Removal is the separate guarded verb from 19-01 (user-confirmed reading)."
  - "clear_override is feed-free so the skip paths can delete the override without constructing a ModuleFeed — the dead write_override delete branch is gone."
  - "RigPlan gained config_name because RigPlan.name is the COMPOSE PROJECT name, not the [rigs.NAME] config key; remove_with must print the key the user would type."
  - "candidate_id_from_target_line requires the full CONTAINER_MODULE_DIR prefix plus a .modl suffix, so an unrelated volume line is never read as a module id."
metrics:
  completed: 2026-09-28
actuals:
  tasks: 4
  commits: 4
---

# Phase 19 Plan 02: The orphan report

Undeclaring a module deletes its mount and tells you what is now orphaned.
It does not touch the gateway.

## The diff

`orphaned_modules(previous, declared, config_name)` reads the PRE-OVERWRITE
override, extracts the module ids `ign` itself wrote, intersects them with the
registry, and subtracts what is still declared. Each orphan carries a
`remove_with` string naming the exact command — including `--rig <name>`.

## Two wiring defects, same shape

Both were found before the live gate ran, and both are instances of this
milestone's recurring failure: the unit was correct and the caller was not.

1. **The dead delete branch.** `write_override` had a delete path that nothing
   called. Found independently by phase research and by review. Fixed with
   `clear_override` invoked from both skip sites.
2. **A second TUI divergence.** The TUI carried its own copy of the
   empty-modules branch and would have dropped the orphan list. The 16-02
   lesson repeated in the same phase.

## What this plan could NOT prove

That a reported orphan can actually be removed from the gateway. 19-03 asks a
real container.
