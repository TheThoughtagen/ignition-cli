---
phase: 06-tui-cockpit
plan: 08
subsystem: api
tags: [ignition, resource-surgery, zip, project-import, live-rig]

# Dependency graph
requires:
  - phase: 05-02
    provides: export-zip member surgery transport (replace_member/remove_member + descriptor synthesis)
  - phase: 05-07
    provides: live-proven landing rule (parent-folder descriptor names the basename)
provides:
  - Root-level file puts land on real 8.3 gateways (perspective-properties.json et al) and round-trip through resource get/list/delete
  - Structure-pinned surgery output for project-root members (CI-green without a rig)
affects: [06-tui-cockpit verification, phase 7 interop, e2e_projects live witnesses]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Root-level member mapping: no-slash user path <X> ↔ member <X>/resources/<X> (module named after the file) — the only gateway-adoptable shape for project-root files"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/resources.rs
    - crates/ignition-core/tests/resources_contract.rs

key-decisions:
  - "Root-level surgery LOCKED live: no-slash user path <X> maps to member <X>/resources/<X> — a module named after the file; container descriptor (<X>/resources/resource.json, files:[<X>]) rides via the existing synthesis. Virgin-rig T2: import exit-0, gateway re-exports at the exact member path, content verbatim."
  - "Dead-wire matrix (virgin-project, controlled): file member named <X>/resources → 500 'module folder must have folder flag set' (reserved container path); descriptor at module root → 500 (module node claim); zip-root files + root descriptor → success but SILENT no-op. UAT's layout matrix was contaminated by sequenced manual experiments in one project — superseded."
  - "Failed imports are ATOMIC (T1: virgin project, module-direct-descriptor 500, store stays clean) — no partial application to fear on retries."
  - "Container-level descriptors (<module>/resources/resource.json) are LEGAL and adopt module-root files on a clean store — UAT experiment A's 'resource already exists' was state contamination, not a structural rule."
  - "Deliberate alias documented in user_path: explicit user path <X>/<X> forwards to the same member as <X> and reads back as <X> — one member, the no-slash spelling wins."

patterns-established:
  - "Adoption oracle for import experiments: fresh re-export after overwrite-import (the gateway re-serializes; a member present in re-export = truly adopted) — stronger than reading back the upload"
  - "Virgin-project experiment harness: seed a throwaway project per matrix cell so prior cells' store state can never contaminate the result"

# Metrics
duration: 52min
completed: 2026-08-28
---

# Phase 6 Plan 8: Root-Level Resource Put Gap Closure Summary

**Root-level file puts (`ign resource put <proj> perspective-properties.json`) now land on real 8.3 gateways via a live-proven module-folder member shape (`<X>/resources/<X>` + container descriptor), structure-pinned by three gateway-free tests**

## Performance

- **Duration:** 52 min
- **Started:** 2026-08-28T10:29:50Z
- **Completed:** 2026-08-28T11:22:21Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Closed the 06-UAT test 6 [major] gap: `resource put` of project-root file members failed with HTTP 500 "module folder must have folder flag set" while nested members landed
- Diagnosed the real mechanism with a controlled live experiment matrix on the 8.3.3 rig (6-cell bisect + 3 virgin-project verification cells): the old `member_path` produced a file member literally named `<X>/resources` — the module's reserved resources-container path
- Discovered the adoptable shape live: a module named after the file (`<X>/resources/<X>`) with the existing container-descriptor synthesis imports exit-0 and the gateway re-exports it at the exact member path
- Full CLI round-trip live-proven: put → get reads the written content back from a fresh gateway export; list shows the no-slash name; delete → honest not_found; nested puts unregressed on the same project
- Re-interpreted the UAT layout matrix: its 500s were state contamination from sequenced manual experiments, not structural rules (container descriptors ARE legal; failed imports are atomic)

## Task Commits

Each task was committed atomically:

1. **Task 1: Pin the failure — structure tests for root-level member surgery** - `cc8e5bd` (test — RED, all 3 new tests failed against the broken shape)
2. **Task 2: Fix the surgery so root-level file members land** - `ed944d9` (fix — GREEN + live verification)

## Files Created/Modified
- `crates/ignition-core/src/client/resources.rs` — `member_path` no-slash mapping → `<X>/resources/<X>`; `user_path` symmetric inverse (`rest == collection` → no-slash); module/fn docs and the degenerate-case unit test reworded to the new reality
- `crates/ignition-core/tests/resources_contract.rs` — 3 new structure pins: `root_level_put_synthesizes_module_folder_shape` (member + container descriptor + no-`X/resources`-file fence + no dir entries + neighbor order), `root_level_delete_removes_the_member`, `resource_put_root_level_member_lands_module_folder_shape` (wiremock crown pin on the import body)

## Decisions Made
- Root-level mapping = module named after the file (see key-decisions). Alternatives considered and rejected live: zip-root placement (silently not adopted — no parent descriptor can exist at the root), shared synthetic module (reserves a global module name; dual user spellings), stem-named module (same dual-spelling issue with no advantage)
- The fix intentionally does NOT try to provision the real Perspective `general-properties` resource by name-magic — any module-root file lands through the same shape under its own module; Perspective config provisioning works by putting the properties JSON at the root-level path

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Experiment harness isolation (execution-side)**
- **Found during:** Task 2 (live verification prep)
- **Issue:** The plan's UAT layout matrix — the stated hypothesis space — proved contaminated: experiment A's "resource already exists: .../resources" 500 does NOT reproduce on a virgin project (my first harness reproduced the same contamination class via stale state, then the controlled virgin-project matrix resolved every cell deterministically)
- **Fix:** Ran the decisive matrix on throwaway virgin projects (gap08w/x/y/z) with the adoption oracle = fresh re-export; conclusions supersede the UAT matrix (documented in key-decisions)
- **Files modified:** none (execution methodology only)
- **Verification:** 6/6 bisect trials deterministic; T1/T2/T3 virgin-project cells conclusive
- **Committed in:** n/a (methodology)

---

**Total deviations:** 1 auto-fixed (1 bug — hypothesis-space correction)
**Impact on plan:** None on scope — the plan explicitly deferred "the exact combination" to discovery ("what Task 1's tests now pin"); the discovered combination differs from the matrix's implication but satisfies all three must-have truths exactly.

## Live Verification Evidence (Task 2)

On the UAT rig (ignition-devops, 8.3.3, localhost:9088), with a fresh headless token (`gap0608`, Phase-4 recipe verbatim):

```
$ ign resource put gap08cli perspective-properties.json --file pp.json --yes
put perspective-properties.json (json)

$ ign resource get gap08cli perspective-properties.json
{ "desktopPageTimeoutSeconds": 310, "updateMessage": "provisioned by ign", "updateMode": "Notify" }

$ ign resource list gap08cli
perspective-properties.json/resource.json
perspective-properties.json

# gateway-side store (fresh export):
perspective-properties.json/resources/resource.json          (container descriptor — gateway-adopted)
perspective-properties.json/resources/perspective-properties.json
views/resources/root/view.json + resource.json               (nested put, same session — unregressed)

$ ign resource delete gap08cli perspective-properties.json --yes
deleted perspective-properties.json
# follow-up get: not_found (exit 6)
```

Scratch projects (gap08w/x/y/z/cli) deleted after verification — rig left at `{ign-cli, pterm}`; pterm was only ever exported read-only. A valid API token (`gap0608`) remains provisioned on the rig per the established Phase-4 pattern.

## Issues Encountered
- None in the shipped code. One execution-side anomaly (a one-off "resource already exists" 500 that vanished on repeat with identical zip content) was traced to harness state staleness and eliminated by the virgin-project harness — see Deviations.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Gap 06-UAT test 6 [major] closed; remaining gap-closure plans 06-07 (metrics/409/hint — already landing in parallel), 06-09/06-10/06-11 (tags freshness, UI polish) unaffected
- The e2e_projects live witnesses can now exercise root-level puts if desired
- Phase 7 (interop) can provision Perspective project configuration via root-level puts

---
*Phase: 06-tui-cockpit*
*Completed: 2026-08-28*

## Self-Check: PASSED
