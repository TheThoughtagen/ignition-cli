---
phase: 05-webdev-backend-tag-operations
plan: 07
subsystem: api
tags: [ignition, project-import, zip-surgery, resource-descriptor, e2e, wiremock]

# Dependency graph
requires:
  - phase: 05-02
    provides: resource family on export-zip surgery (replace_member/remove_member primitives)
  - phase: 05-03
    provides: webdev deploy's project_import caller + the 200-denial precedent
  - phase: 03-02
    provides: project import/export machinery + collision matrix
provides:
  - import-denial seam: every project_import caller (resource put/delete, project import, webdev deploy) refuses exit 6 import_denied on HTTP-200 {success:false} bodies
  - put-new landing: append-when-absent lands via parent-folder resource.json descriptors (merge-or-synthesize), live-proven on 8.3.3
  - additive slug import_denied (exit 6) with the gateway's problem text verbatim
  - structurally-corrupt import zips refuse invalid_import_file exit 2 before upload (the truncated-zip data-loss guard)
  - the never-before-run e2e_projects live loop is GREEN and permanently extended with the put-new-on-used-project witness
affects: [05-08 (needs the live rig + healthy routes), phase-6 TUI (resource verbs now honest), agents keying on import_denied]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "200-body denial detection on the import family: ONE seam in ReqwestGatewayApi::project_import fixes all four callers — per-caller checks forbidden (the WebDev 05-01 precedent applied to imports)"
    - "folder-descriptor landing rule (live-proven): a new file member lands only when its immediate parent folder's resource.json lists its basename; bare appends are silently ignored with success:true"
    - "import full-structure validation before upload: walk + decompress every member (truncated zips wipe targets wearing success:true)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/projects.rs
    - crates/ignition-core/src/client/resources.rs
    - crates/ignition-core/src/actions/projects.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-cli/tests/contract_resources.rs
    - crates/ignition-cli/tests/contract_projects.rs
    - crates/ignition-cli/tests/e2e_projects.rs
    - README.md

key-decisions:
  - "ImportDenied variant placed near InvalidImportFile (import family) but exit 6 target_state; slug import_denied; hint names project export as the hand-edit baseline"
  - "import_denied() refuses ONLY on explicit bool success:false — missing key/true/string-'false'/fallback stay opaque-success (never refuse on absence of proof)"
  - "Spike candidate 1 (dir-entry ancestors) DISPROVEN live; winner = candidate 3 format insight: parent-folder resource.json descriptors (merge when present, synthesize when absent)"
  - "Synthesized descriptor shape pinned to the live-accepted wire form: scope G, version 1, restricted false, overridable true, files:[basename], attributes {}; descriptor rides BEFORE the appended member"
  - "remove_member leaves descriptors untouched — the gateway itself prunes stale files entries (variant-G2 wire truth)"
  - "Appending a member whose basename IS resource.json authors its descriptor explicitly — no second synthesized descriptor"
  - "Corrupt parent descriptor on the append path refuses internal rather than shipping a silently-ignored import"

patterns-established:
  - "Denial seam: check the parsed 200 body at the ONE client method, not at callers"
  - "Landing proofs: read-back verification in e2e (never trust the import success body alone)"

# Metrics
duration: 196min
completed: 2026-08-27
---

# Phase 5 Plan 07: Gap Closure — Import-Denial Seam + Put-New Landing Summary

**Import denials surface as exit 6 `import_denied` at one seam (all four callers), put-new lands via parent-folder `resource.json` descriptors (spike-proven live on 8.3.3), corrupt import zips refuse before upload, and the never-run e2e live loop is green with a new put-new-on-used-project witness.**

## Performance

- **Duration:** 196 min (spike-inclusive)
- **Started:** 2026-08-26T22:10:30Z
- **Completed:** 2026-08-27T01:26:43Z
- **Tasks:** 3
- **Files modified:** 9 (+1 planning artifact)

## Accomplishments
- **Denial honesty**: `project_import` parses the 200 body and refuses on explicit `success:false` — resource put/delete, `project import`, and webdev deploy all inherit via the single seam; wiremock goldens pin slug/exit/problem-text/endpoint for put, delete, and the import verb.
- **Put-new actually lands**: live spike on the commissioned 8.3.3 rig characterized the landing rule (folder descriptors), the surgery implements merge-or-synthesize, and the CLI-proven round-trip works on fresh AND used projects.
- **Data-loss guard**: truncated zips (valid magic, broken tail) refuse `invalid_import_file` exit 2 before any upload — the gateway would otherwise answer `success:true changes:[]` and wipe the target.
- **The live gate**: `e2e_projects` loop ran live for the FIRST time (green, 5.6s) — this run is the escape hatch that let Gap 1 ship, now permanently closed.

## Task Commits

1. **Task 1: Import-denial seam (error variant, pure helper, seam, goldens, README)** — `0834e81` (feat)
2. **Task 2: put-new landing via descriptors (spike → surgery implementation)** — `719924d` (feat)
   - **Rule 2 deviation: truncated-zip import guard** — `9fb9152` (fix)
3. **Task 3: live e2e gate green + used-project witness** — `8dab0a5` (test)

## Files Created/Modified
- `crates/ignition-core/src/error.rs` — `ImportDenied` variant (exit 6, slug `import_denied`), doc table + enumerated test
- `crates/ignition-core/src/client/projects.rs` — pure `import_denied()` helper + unit pins
- `crates/ignition-core/src/client/mod.rs` — the seam in `project_import`
- `crates/ignition-core/src/client/resources.rs` — descriptor surgery (merge/synthesize) in `rewrite_zip` + 5 new unit pins
- `crates/ignition-core/src/actions/projects.rs` — full-structure import validation + real-zip test fixtures
- `crates/ignition-cli/tests/contract_resources.rs` — denial fixture, put/delete denial goldens
- `crates/ignition-cli/tests/contract_projects.rs` — import denial test, truncated-zip test, real-zip fixture
- `crates/ignition-cli/tests/e2e_projects.rs` — step 10b witness + scratch3 wipe witness
- `README.md` — exit table + import row (validation + denial)

## The Spike Record (which candidate won, with the wire evidence)

All probes ran live on `ignition-devops` (8.3.3, port 9088) with scratch projects `spike07a–d` (deleted after):

| Candidate | Wire answer | Verdict |
|---|---|---|
| **1. Directory entries** (append + explicit dir-entry ancestors, with/without) | `{"success":true,"changes":[…]}` — **member did NOT land** (both shapes, fresh project) | **DISPROVEN** — and worse than the UAT's denial: a silent success-true no-op |
| 2. Two-pass identical import | moot — pass 1 was a silent no-op, not a denial; nothing to re-POST against | skipped as disproven by 1 |
| **3. Format inspection** (real `ign-cli` export) | every resource folder carries `resource.json` with a `files` array; fresh projects carry `ignition/global-props/resource.json` | **WINNER** — the landing rule |
| Variant D (synthesize parent descriptor) | `success:true` + member LANDED (fresh project) | ✅ |
| Variant E1 (merge into existing descriptor, used project) | `success:true` + landed | ✅ |
| Variant E3 (two-level new chain, descriptor at immediate parent) | `success:true` + landed | ✅ |
| Variant G2 (delete leaving stale `files` entry) | `success:true` + deleted; descriptor came back with the entry **pruned by the gateway** | delete needs no surgery |
| BONUS (corrupt/truncated zip) | `{"success":true,"changes":[]}` + **project wiped to bare project.json** | → Rule 2 fix (9fb9152) |

**Landing rule (pinned)**: an overwrite-import lands a NEW file member only when its immediate parent folder's `resource.json` exists and lists the basename. Intermediate plain folders carry nothing (webdev `cli/` precedent). A target whose basename IS `resource.json` authors its descriptor explicitly.

## Live Gate Transcript Highlights (first-ever run, GREEN 5.6s)

project new → list-contains → resource list (fresh, empty) → **put scratch --yes (fresh project) → LANDS → get round-trip (json, e2e:true)** → list contains → two-sided put honesty (get OLD → export → put NEW → get NEW) → put scratch2 (gateway-only) → **10b. put uat2/second.py on the USED project → LANDS → get reads exact content (text) → list shows member + synthesized `uat2/resource.json`** → abort import (`project_exists`) → overwrite import +`--yes` → scratch survived with OLD content; scratch2 AND scratch3 `not_found` (replace-not-merge, Pitfall 4) → delete scratch → `not_found` → rename → copy → cleanup. All steps green, including the pre-existing sections — which is also Task 1's regression guard (the seam did not over-fire on real success bodies).

## Rig / Token State for the 05-08 Executor

- Rig `ignition-devops` UP and healthy (docker, port 9088, profile `uat`); `ign webdev status`: all four routes **present at 1.0.0, ok:true**.
- Token: `uattok:…` at `/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/opencode/token.txt` (env `IGNITION_TOKEN` format `NAME:key`). **Trial had ~41 min left at plan end (2470s at 01:26Z)** — if expired: `ign rig reset --yes`, re-provision the headless token per the recipe in `.planning/phases/04-rig-lifecycle-trial-state/04-VERIFICATION.md` (addendum), re-create the `uat` profile, then `ign webdev deploy` to leave routes healthy.
- Spike scratch projects (spike07a–d) were deleted; the loop cleans its own timestamped projects.

## Decisions Made
- Seam over per-caller checks (one `project_import` fix point); explicit-bool-only denial detection; descriptor merge/synthesize with the live-proven shape and ordering; gateway-owned descriptor reconciliation on delete; internal refusal on corrupt descriptors during append. See key-decisions above for the full set.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Truncated-zip import guard**
- **Found during:** Task 2 (live spike — the variant-G build crash produced a truncated zip by accident)
- **Issue:** the gateway accepts a truncated zip (valid PK magic, broken tail) with `{"success":true,"changes":[]}` and on overwrite REPLACES the project with the partial contents — silent data loss; the CLI's import guard only checked magic + size
- **Fix:** `validate_import` walks and decompresses every member (`zip::ZipArchive`) before any network; corruption refuses `invalid_import_file` exit 2; test fixtures upgraded to real archives (3 goldens moved: byte counts now `[..]`-elided)
- **Files modified:** crates/ignition-core/src/actions/projects.rs, crates/ignition-cli/tests/contract_projects.rs, README.md
- **Verification:** unit + binary pins (truncated fixture refuses pre-network, both layers); full suite + clippy green
- **Committed in:** 9fb9152

**2. [Rule 1 - Bug] Spike disproved the plan's primary candidate (dir-entry ancestors)**
- **Found during:** Task 2 (variant A/B live probes)
- **Issue:** dir-entry appends still silently no-op (success:true, nothing lands) — the plan's expected fix shape could never work
- **Fix:** the plan's own spike protocol resolved it: candidate 3 (format inspection) revealed the folder-descriptor landing rule; implemented merge-or-synthesize per the objective's "a put-new surgery shape the gateway importer accepts"
- **Files modified:** crates/ignition-core/src/client/resources.rs
- **Verification:** live CLI round-trips on fresh + used projects; 5 new unit pins; e2e gate green
- **Committed in:** 719924d

---

**Total deviations:** 2 auto-fixed (1 missing critical, 1 bug/spike-course)
**Impact on plan:** Both essential — the first prevents silent data loss in the same code path family; the second is the plan's own spike decision procedure working as designed. No scope creep.

## Issues Encountered
- **The silent no-op is deeper than the UAT's denial**: the UAT wire repro saw `success:false`; today's probes of the same append shape saw `success:true` with nothing landing (twice as dishonest). Both halves are now closed: the seam catches explicit denials; the descriptor surgery makes appends actually land.
- Env contamination during the final suite run (my exported `IGNITION_URL`/`IGNITION_TOKEN` made `contract_version` hit the live gateway) — re-ran clean: all 33 targets green. No product issue; documented here so future executors run the suite with a clean env.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- UAT Gap 1 CLOSED end-to-end: put-new lands (fresh + used, live-proven), denials are honest at every caller, the gate that would have caught the gap is green and permanent.
- 05-08 (Gap 2, alarms view→ack loop) can run immediately: rig up, routes healthy, token in `/var/folders/jy/nmh43099607fl9kmv2s8gbdh0000gn/T/opencode/token.txt` — watch the trial clock (~41 min at plan end; reset recipe in 04-VERIFICATION.md addendum).

---
*Phase: 05-webdev-backend-tag-operations*
*Completed: 2026-08-27*

## Self-Check: PASSED
