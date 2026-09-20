---
phase: 05-webdev-backend-tag-operations
plan: "02"
subsystem: api
tags: [zip, ignition, resource-surgery, export-import, wiremock, rust]

# Dependency graph
requires:
  - phase: 03-project-operations
    provides: streaming project export, buffered import with overwrite query param, collision conventions, resource UX contract (commands/flags/JSON shapes)
provides:
  - Working `ign resource list/get/put/delete` against any real 8.3 gateway via export-zip member surgery (closes the Phase 3 cross-phase blocker)
  - Pure zip-surgery helpers (resource_members, read_member, replace_member, remove_member) in ignition-core — reusable for any export-zip member manipulation
  - --yes-guarded resource put/delete whose refusal messages name the overwrite-import consequence
  - Live-runnable e2e resource witnesses (e2e_projects loop re-pinned on the surgery verbs)
affects: [05-webdev-deploy (reuses zip dep + surgery for deploy zips), e2e_rig resource-witness step (unblocked)]

# Tech tracking
tech-stack:
  added: ["zip 8.6 (deflate-only, default-features off) — the ONLY new dependency, research-approved", "tempfile promoted from dev-dep to regular dep of ignition-core (already in workspace graph)", "zip as dev-dep of ignition-cli (golden fixtures — same crate, no new lockfile entry)"]
  patterns: [export→surgery→import(overwrite) orchestration at the actions layer, user-path↔member-path mapping (<collection>/<rest> ↔ <collection>/resources/<rest>), member-level (not byte-level) zip assertions in contract tests, guarded-verb operation strings carrying the destructive consequence]

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/resources.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/resources.rs
    - crates/ignition-core/src/actions/{projects,sessions,version,doctor,connections,inspect,logs,rig}.rs
    - crates/ignition-core/tests/resources_contract.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/tests/contract_resources.rs
    - crates/ignition-cli/tests/e2e_projects.rs
    - crates/ignition-cli/tests/e2e_rig.rs
    - README.md
    - Cargo.toml
    - crates/ignition-core/Cargo.toml
    - crates/ignition-cli/Cargo.toml

key-decisions:
  - "Resource family re-pointed onto export-zip surgery: UX unchanged, transport swapped — no per-resource REST routes exist on 8.3 (triple-verified); the Phase 3 blocker is CLOSED"
  - "zip 8.6 exactly as planned (research said 6.x, crates.io latest stable is 8.6.0); deflate-only, fixed SimpleFileOptions defaults = deterministic rewrites"
  - "resource put joined the --yes-guarded set — its member surgery implicitly overwrite-imports the whole project (03-03's unguarded put superseded); put/delete refusal MESSAGES name the consequence via the operation string (the shared ConfirmationRequired hint stays frozen)"
  - "Prefix filter moved client-side (member starts_with) — the old server-side path query param rode routes that never existed"
  - "not_found for missing members carries endpoint:null — there was no 404 URL, there was a missing zip member (surgery-level honesty)"
  - "tempfile promoted from dev to regular dep of ignition-core for the temp export file (plan's sanctioned option — already in the workspace graph, zero new lockfile entries)"

patterns-established:
  - "Zip-member surgery helpers are PURE (no GatewayApi surface, no I/O beyond the zip crate) — unit-testable without a gateway; orchestration stays in the actions layer"
  - "Contract tests assert request SEQUENCES (exactly-one-export/zero-imports for reads; export→overwrite-import for writes) with member-level body round-trips through the same public surgery helpers"

# Metrics
duration: 38min
completed: 2026-08-24
---

# Phase 5 Plan 2: Resource Family Re-point (Export-Zip Surgery) Summary

**`ign resource` list/get/put/delete re-pointed from nonexistent REST routes onto project-export zip member surgery (zip 8.6, export → member surgery → overwrite-import), closing the Phase 3 cross-phase blocker and making the e2e witness loop live-runnable for the first time since Phase 3**

## Performance

- **Duration:** 38 min
- **Started:** 2026-08-24T14:25:05Z
- **Completed:** 2026-08-24T15:02:41Z
- **Tasks:** 3
- **Files modified:** 16

## Accomplishments
- All four resource ops now work against any real 8.3 gateway: the Phase 3 `/projects/{p}/resources/**` defect (STATE.md cross-phase blocker) is CLOSED
- Pure zip-surgery helpers (list/read/replace-inject/remove) with full round-trip unit coverage, including append-when-absent (put upsert) and directory-entry preservation
- Binary fencing survives the transport: get sniffs the MEMBER bytes (exit 6 resource_binary), put refuses binary input before any network I/O
- resource put/delete are --yes-guarded with consequence-naming refusal messages; e2e witnesses (projects loop + rig pre-witness) re-pinned on the surgery verbs with two-sided put honesty

## Task Commits

Each task was committed atomically:

1. **Task 1+2: zip surgery helpers + trait cleanup + actions re-point + sequence-pin contract tests** - `4506e07` (feat) — committed together because Task 1's delete of the REST path builders leaves the crate uncompilable without Task 2's trait/doubles/actions rewrite (see Deviations)
2. **Task 3: guarded put + goldens + e2e witnesses + README** - `369d183` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified
- `crates/ignition-core/src/client/resources.rs` — rewritten: pure zip-member surgery helpers (resource_members, read_member, replace_member, remove_member) + user↔member path mapping + round-trip units; old REST path builders deleted
- `crates/ignition-core/src/client/mod.rs` — GatewayApi trait: project_resources/get/put/delete methods + orphaned put_bytes pipeline deleted
- `crates/ignition-core/src/actions/resources.rs` — rewritten: export→surgery→import(overwrite) orchestration; classify_content sniffer retained (member bytes now)
- `crates/ignition-core/src/actions/{projects,sessions,version,doctor,connections,inspect,logs,rig}.rs` — 31 unreachable! trait stubs removed across 10 test doubles (the doubles shrink)
- `crates/ignition-core/tests/resources_contract.rs` — rewritten as request-SEQUENCE pins at the actions layer
- `crates/ignition-cli/src/main.rs` — resource put gains the pre-resolution --yes guard; put/delete operation strings name the overwrite-import consequence
- `crates/ignition-cli/tests/contract_resources.rs` — goldens re-pointed onto export/import fixtures; new put-refusal golden; member-level import-body assertions
- `crates/ignition-cli/tests/e2e_projects.rs` — loop re-pinned: list empty→populated, two-sided put honesty, delete witness, replace-not-merge retained
- `crates/ignition-cli/tests/e2e_rig.rs` — pre-witness put gains --yes
- `README.md` — resource section rewritten (mechanism, perf honesty, concurrent-edit race, guarded verbs); destructive-ops list updated
- `Cargo.toml`, `crates/ignition-core/Cargo.toml`, `crates/ignition-cli/Cargo.toml` — zip 8.6 workspace dep; tempfile promoted (core, regular); zip dev-dep (cli)

## Decisions Made
- Refusal consequence text rides the OPERATION string (flows into the message) rather than a new error variant — the shared ConfirmationRequired hint stays byte-frozen for all other guarded verbs (no golden churn outside this family)
- put's guard fires AFTER the input read (plan's stated order: read --file/stdin → guard → resolution) — so a missing --file still exits 2 invalid_input and binary input WITH --yes still exits 6 resource_binary; the binary-put-without-yes case now refuses at the guard (documented in the golden as the new put-refusal shape)
- e2e put honesty is witnessed through `resource get` before/after the second put (get rides the same export the surgery operates on) — the plan's "export before/after put contains member M" verified without unzipping in the test process
- zip dev-dep added to ignition-cli for golden fixtures (the plan's "no new dependency" must-have holds — same crate the core already depends on, lockfile unchanged)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 1+2 committed as one atomic commit**
- **Found during:** Task 1 execution
- **Issue:** Task 1 deletes the REST path builders, but the GatewayApi trait impls, 10 test doubles, and actions/resources.rs still referenced them — the crate cannot compile between the plan's Task-1 and Task-2 boundaries (Task 1's own verify command requires the lib to compile). Symmetrically, Task 2's `cargo test --workspace` verify cannot pass until Task 3's CLI goldens land.
- **Fix:** Split by content instead of by task: commit `4506e07` = all ignition-core changes (Tasks 1+2), commit `369d183` = all ignition-cli + README changes (Task 3). Each commit compiles and its scoped verification passes; the full-workspace green proof lands with the second commit.
- **Files modified:** (both commits' files, as listed above)
- **Verification:** `cargo test -p ignition-core` + clippy clean at commit 1; `cargo test --workspace` (30 suites) + clippy clean at commit 2
- **Committed in:** 4506e07, 369d183

**2. [Rule 3 - Blocking] e2e_rig.rs pre-witness put needed --yes**
- **Found during:** Task 3
- **Issue:** e2e_rig.rs's pre-snapshot witness spawns `resource put` without --yes; with put now guarded, the live e2e_rig gate would fail at the refusal (STATE.md notes this gate's resource-witness step was blocked by this very plan's defect — it must run green now)
- **Fix:** Added `--yes` to the pre-witness put spawn (file not in the plan's files_modified; mechanical, required for the must-have "e2e witness loop runs green live")
- **Files modified:** crates/ignition-cli/tests/e2e_rig.rs
- **Verification:** workspace tests green; e2e_rig compiles with the flag
- **Committed in:** 369d183

**3. [Rule 1 - Bug] Two zip-writing test snippets needed std::io::Write in scope**
- **Found during:** Task 1 (compile)
- **Issue:** The top-level `use std::io::Write` doesn't reach the #[cfg(test)] module; zip writer calls failed E0599
- **Fix:** `use std::io::Write as _;` in the tests module (and in each integration test file that builds fixture zips)
- **Files modified:** crates/ignition-core/src/client/resources.rs (tests), crates/ignition-core/tests/resources_contract.rs, crates/ignition-cli/tests/contract_resources.rs
- **Verification:** compile + tests green
- **Committed in:** 4506e07, 369d183 (part of task commits)

---

**Total deviations:** 3 auto-fixed (2 blocking, 1 bug)
**Impact on plan:** Boundary/compile-level adaptations only — the plan's content, contracts, and must-haves all landed exactly as specified. No scope creep.

## Issues Encountered
- The plan's Task-1 verify command (`cargo test -p ignition-core resources`) transitively requires Task-2 deletions — resolved by the merged-commit split above, not a code problem
- wiremock 0.6 has no `headers_matching` matcher; the import mock pins `content-type: application/zip` via the exact `header` matcher instead (the client sends the exact value)

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The Phase 3 cross-phase blocker (STATE.md) is CLOSED by this plan: resource ops work against any real 8.3 gateway; the e2e_rig resource-witness step is unblocked
- zip 8.6 + the surgery helpers are available for 05-01's deploy-zip writing and any later export-zip manipulation (05-webdev deploy reuses the dependency)
- Wave-1 sibling plans (05-01 landed in parallel; 05-03+) proceed independently

---
*Phase: 05-webdev-backend-tag-operations*
*Completed: 2026-08-24*

## Self-Check: PASSED
