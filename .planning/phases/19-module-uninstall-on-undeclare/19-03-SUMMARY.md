---
phase: 19-module-uninstall-on-undeclare
plan: 03
subsystem: rig-modules
tags: [live-gate, docker, evidence, sc1, sc4, cross-version]
status: complete
dependency-graph:
  requires: [19-01, 19-02]
  provides: [live-proof-sc1, live-proof-sc4, selectable-image-tag]
  affects: []
tech-stack:
  added: []
  patterns: [opt-in-live-gate, carry-the-error-retry, non-vacuous-seed, observe-never-set-env]
key-files:
  created: []
  modified:
    - crates/ignition-core/tests/live_rig_module_injection.rs
    - crates/ignition-core/tests/fixtures/live-rig/compose.yml
    - README.md
decisions:
  - "D-19-08 applied: the SC-4 seed is the stock image's OWN modules, read from /modules/healthy before the uninstall. 29 of them on both tags. Non-emptiness is ASSERTED, because a vacuous pass is how the original SC-4 claim shipped false."
  - "D-19-09 applied: §C no longer hand-deletes the override. It drives provision_modules with nothing declared, so rig up performs the deletion the test used to fake."
  - "The test OBSERVES IGNITION_LIVE_RIG_IMAGE_TAG and never sets it — std::env::set_var is unsafe under edition 2024 and the compose child already inherits the operator's environment."
metrics:
  completed: 2026-09-28
actuals:
  tasks: 3
  commits: 1
---

# Phase 19 Plan 03: SC-1 and SC-4, proven live

Two runs, two pinned images, both green. This is the first and only evidence
in the repository that `ign` actually removes a module from a gateway.

## The transcripts

| | 8.3.3 | 8.3.9 |
|---|---|---|
| Runtime | 112.45s | 97.62s |
| Healthy before §D | 31 modules | 31 modules |
| `com.axone_io.ignition.git` after | **absent** | **absent** |
| `project-scan-endpoint` after | **absent** | **absent** |
| §E seed (modules `ign` did not place) | 29 | 29 |
| Seed still healthy after §D | 29 of 29 | 29 of 29 |
| Exit | 0 | 0 |

Leak check after both runs: no fixture container, no fixture volume, and
`security find-generic-password -s ignition-cli -a phase16-live-gate` finds
nothing. With `IGNITION_LIVE_RIG_MODULES` unset the suite is still
`0 passed; 1 ignored`.

## What each section now proves

| Section | Claim |
|---|---|
| §A | a declared module is mounted and reaches HEALTHY, never quarantine (SC-2, SC-5) |
| §B | it survives a container recreate (SC-3) |
| §C | undeclaring makes `rig up` delete the override and report exactly the two orphans (SC-4) |
| §D | `ign`'s uninstall leaves it ABSENT from the gateway's healthy list (**SC-1**) |
| §E | all 29 modules `ign` did not place are still healthy (SC-4) |

## The correction to §C

§C used to call `std::fs::remove_file(&override_path)` before bringing the
rig up. That single line is what masked `write_override`'s dead delete branch
for an entire phase: the test performed the cleanup it was checking. §C now
computes the undeclared provisioning through `provision_modules` and asserts
that `rig up` deleted the file.

This was the sixth defect of that shape in this milestone — the unit correct,
the wiring absent — and the third time a test simulated the path it claimed to
exercise.

## The three LOW-confidence questions the research left open

**Q: did the contract hold on a second version?** Yes, for everything the gate
observes: 8.3.9 matched 8.3.3 on the 31-module healthy list, the ghost-entry
uninstall, the immediacy and the untouched 29-module seed. What the gate does
NOT observe is listed in the next answer.

**Q: did anything contradict the four live facts?** Nothing contradicted them,
but the gate only EXERCISES two of the four, and the original wording of this
answer claimed all four — corrected here rather than left standing.

| Live fact | Exercised by the gate? |
|---|---|
| ghost-entry uninstall succeeds | **yes** — §D, both tags |
| no restart required | **yes** — §D asserts absence immediately, no restart between |
| uninstall REFUSES while the `.modl` is still mounted | **no** — §C unmounts before §D, so this path never runs |
| the denial rides a 200 | **no** — the gate never inspects a raw HTTP status |

The last two rest on the 19-RESEARCH probe against 8.3.3 alone. They are
covered by contract tests at the `GatewayApi` seam (`module_uninstall_contract.rs`),
which pin `ign`'s HANDLING of a `success:false`-on-200 body — not the gateway's
choice to send one. Claiming cross-version confirmation for them would be a
claim derived from a requirement rather than from behavior, which is the exact
failure this phase exists to correct.

**Q: was `data/jar-cache/<gateway-module-id>/` residue still present?**
**Not observed.** Teardown runs `down -v` unconditionally and destroys the
volume before anything can inspect it, and adding a pre-teardown exec was out
of this plan's scope. Recording this as unanswered rather than inferring it:
deriving the answer from the requirement instead of from behavior is exactly
how 16-03's predecessor shipped a falsehood.

## One incidental finding

`com.inductiveautomation.reporting` IS installed on the stock image — it
appears in every healthy list above. The research's probe of that id returned
`success:false`, which it read as ambiguous. It was a refusal, not a missing
module, and the ambiguity is resolved: a `success:false` for an installed
module means the gateway declined, not that the id was unknown.
