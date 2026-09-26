---
phase: 16-compose-override-module-injection
plan: 03
subsystem: rig-modules
tags: [live-gate, docker, adopt, evidence]
status: complete-with-finding
dependency-graph:
  requires: [16-01, 16-02]
  provides: [live-proof-module-loads, sc4-falsification]
  affects: [16-04]
tech-stack:
  added: []
  patterns: [opt-in-live-gate, adopt-bootstrapped-test-auth, unconditional-teardown]
key-files:
  created:
    - crates/ignition-core/tests/live_rig_module_injection.rs
    - crates/ignition-core/tests/fixtures/live-rig/compose.yml
  modified:
    - README.md
decisions:
  - "D-23 applied: SC-4's falsification is RECORDED, not quietly downgraded. The test asserts what is true (the override is gone and not recreated) and prints an explicit known-limitation note naming the modules that remain installed."
  - "Test auth bootstraps through the real adopt action rather than basic auth — a commissioned 8.3 gateway answers /data/api/v1 with 401 for Basic (observed live)."
  - "The fixture sets NO acceptance variable (D-21). That absence is the control: a healthy module proves the values came from ign's override."
metrics:
  completed: 2026-09-25
actuals:
  tasks: 1
  commits: 2
---

# Phase 16 Plan 03: Live gate Summary

The first proof in Phase 16 that is not a fake. `LIVE EXIT: 0`, 124 seconds,
against a real `inductiveautomation/ignition:8.3.3` container.

## Proven Live

| Criterion | Result |
|---|---|
| SC-2 — module loads on a stock image | **proven** — both modules HEALTHY, neither quarantined |
| SC-3 — survives a container recreate | **proven** — both healthy again after `down`/`up` |
| SC-5 — one generic seam, two modules | **proven** — different orgs, different id shapes |
| D-24 — comma-separated accept vars | **CONFIRMED CORRECT** |
| `gateway_service` heuristic | works against a real compose file |
| SC-4 — deleting the override reverts | **FALSIFIED** (below) |

The fixture sets **no** `ACCEPT_MODULE_CERTS`/`ACCEPT_MODULE_LICENSES`. That
absence is the experiment's control: a signed module a gateway has not been
told to trust is quarantined, so a module reaching the HEALTHY list proves the
acceptance values came from the override `ign` generated. Quarantine is treated
as a FAILURE, not a pass.

D-24 mattered more than it looked. Plan 16-02 pinned `ACCEPT_MODULE_CERTS: "a,b"`
in a golden test and flagged it as stable-not-correct. A wrong separator would
have left one module quarantined while all 68 suites stayed green — the exact
silent failure the live gate exists to catch. Two modules were declared
precisely so the format could be falsified rather than assumed.

## SC-4 Is Falsified

**The claim:** deleting `compose.ign-modules.yml` "fully reverts module
provisioning" (RMOD-06, SC-4, and the README section written in 16-02).

**Observed:** after deleting the override and bringing the rig up with nothing
declared, the gateway still reports BOTH modules healthy.

**Cause:** the mount at `user-lib/modules` is genuinely gone — that path is not
in the data volume. But Ignition *installs* an accepted module into its data
directory, and that directory IS the persistent volume. Removing the source
file does not uninstall the module.

**Why no test caught it:** no unit or contract test can observe a gateway's
installed-module state. Every one of them agreed with the requirement's wording,
and the README repeated it, because the claim was derived from the requirement
rather than from behavior.

**Handling:** D-23 applied. The test asserts what is true (the override is gone
and is not recreated) and prints an explicit note naming the modules that remain
installed, rather than passing silently or being weakened into an
artifact-presence check that still claimed SC-4. README corrected (`120e92c`).
The uninstall path — `DELETE /data/api/v1/modules/uninstall`, confirmed present
in the 83-api collection — is plan 16-04.

## Four Diagnoses the Gate Earned

Each is now a comment where the next reader would hit it:

1. **`healthy: []`, `quarantined: []`.** The helper swallowed the client error
   with `if let Ok(...)`, making an empty list and a failed call identical —
   which would send someone debugging module loading when the problem was auth.
   Carrying the error turned this into the next finding.
2. **HTTP 401.** Basic auth is rejected by a commissioned 8.3 gateway's
   `/data/api/v1`. The test now bootstraps through the real `adopt` action
   (OIDC login → mint API token), exercising `ign`'s own supported path.
3. **"profile vanished mid-adopt".** `adopt` REWRITES an existing profile's
   auth; it does not create one. The test seeds the profile first.
4. **The fixture had no named volume.** The gateway's data died with the
   container, so §B was testing a brand-new gateway rather than a recreate — it
   was not testing what it claimed. Fixed, with the reasoning written into the
   fixture so it is not "cleaned up" later.

## Cleanup Is Verified, Not Assumed

The test mints a real API token into the operator's macOS keychain via `adopt`.
Teardown deletes it unconditionally and runs `down -v`. Both confirmed after the
run: zero containers leaked (`docker ps -a`), keychain entry absent
(`security find-generic-password`).

## Verification

`IGNITION_LIVE_RIG_MODULES=1` → exit 0, 124.47s. Unset → green no-op (D-20), so
CI and any developer without Docker are unaffected. All five CI gates green:
`fmt` 0 · `clippy` 0 · `build` 0 · `no-default-features` 0 · `test` 0
(69 suites).

## Carried Into 16-04

The uninstall path is not just an API call. `rig up` is not in `GUARDED_OPS`
today, and uninstalling a module from a gateway would be the first destructive
gateway write this feature performs. Whether that may happen without a
confirmation gate is a guard decision, not a detail.
