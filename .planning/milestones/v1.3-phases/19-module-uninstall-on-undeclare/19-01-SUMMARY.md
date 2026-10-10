---
phase: 19-module-uninstall-on-undeclare
plan: 01
subsystem: rig-modules
tags: [uninstall, guard, gateway-write, confirmation]
status: complete
dependency-graph:
  requires: [16-01, 16-02]
  provides: [rig-module-uninstall-verb, uninstall-refusals]
  affects: [19-02, 19-03]
tech-stack:
  added: []
  patterns: [guarded-op, denial-rides-200, typed-error-slug]
key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/rig.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/tests/module_uninstall_contract.rs
    - README.md
decisions:
  - "The uninstall joins GUARDED_OPS (user lock, binding): it is an irreversible gateway write, so it requires --yes or IGNITION_YES=1."
  - "D-19-04: ONE call per ModuleSpec, never a batch. Batch atomicity was never probed, so a partial failure would be indistinguishable from a total one."
  - "Denial rides 200: the gateway answers success:false with HTTP 200. uninstall_module returns Ok(()) only when parsed.success is EXPLICITLY true; anything else is module_uninstall_denied."
metrics:
  completed: 2026-09-27
actuals:
  tasks: 3
  commits: 3
---

# Phase 19 Plan 01: The uninstall verb

`ign rig module uninstall <id> --rig <name> --yes` — the only way a module
`ign` provisioned leaves the gateway.

## What shipped

| Piece | Where |
|---|---|
| `GatewayApi::uninstall_module` | `client/mod.rs` |
| `actions::rig::rig_module_uninstall` | `actions/rig.rs` |
| Three refusals | `module_uninstall_denied`, `module_not_in_registry`, `module_still_declared` |
| The guard | `GUARDED_OPS` entry keyed on the clap leaf path |

## The three refusals

1. **No `--yes`** — the guard refuses before any request leaves the process.
2. **Unregistered id** — `ign` only knows the two registry modules; anything
   else is refused rather than passed through to the gateway.
3. **Still declared** — uninstalling a module the rig still declares would be
   undone by the next `rig up`, so it is refused with the fix in the message.

## Falsifiability

The guard was perturbed (removed from `GUARDED_OPS`), the contract test was
confirmed to fail for the right reason, and the guard was restored.

## What this plan could NOT prove

That the gateway actually removes the module. No unit or contract test can
observe a gateway's installed-module state — that is 19-03's job, and it is
the same gap that let Phase 16's SC-4 claim ship false.
