---
phase: 03-project-operations
plan: 01
subsystem: projects
tags: [ignition, projects, crud, clap, wiremock, percent-encoding, reqwest]

# Dependency graph
requires:
  - phase: 02-gateway-health-inspection
    provides: classify() pipeline + post_empty/delete_with_query helpers, require_confirmation destructive dispatch shape (sessions terminate), ListEnvelope/ListQuery, snapbox+wiremock golden harness
provides:
  - 7 GatewayApi project methods (list/find/create/copy/rename/modify/delete) over the native /data/api/v1/projects/* family
  - post_json/put_json pipeline helpers (the body-carrying mutation seam 03-02/03-03 reuse)
  - encode_segment percent-encoding for every {name} path segment (03-03 resource paths reuse per-segment)
  - project_find as 03-02's import collision pre-check
  - `ign project list/new/copy/rename/set/delete` user commands; delete = the phase's --yes proof
affects: [03-project-operations, phase-4-rig, phase-6-tui]

# Tech tracking
tech-stack:
  added: ["percent-encoding 2 (already a transitive dep of url — declared direct)"]
  patterns:
    - "post_json/put_json classify-first body-carrying pipeline helpers (Ok classification IS the success contract for unverified-LOW bodies)"
    - "mutation + find read-back action shape (create/copy/rename/set all re-find; the gateway's answer is the truth)"
    - "serde-skipped display-only field on action results (ProjectSetResult.fields — human rendering without polluting the agent JSON)"
    - "clap ArgGroup requiring >=1 field on `set` (no no-op modify round-trips)"

key-files:
  created:
    - crates/ignition-core/src/client/projects.rs
    - crates/ignition-core/src/actions/projects.rs
    - crates/ignition-core/tests/projects_contract.rs
    - crates/ignition-cli/tests/contract_projects.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/tests/live_gateway.rs
    - README.md
    - Cargo.toml
    - crates/ignition-core/Cargo.toml

key-decisions:
  - "ProjectModify.enabled is Option (skip-if-none) — the plan's 'create fields minus name, same skip discipline' read as: an always-sent enabled on modify would clobber the flag on single-field sets; the Task-2 unit pin {\"title\":\"T\"} demanded it"
  - "ProjectSetResult carries fields-touched as a serde(skip) Vec<String> — the plan's human line `set <fields> on <name>` without deviating the flat six-key agent JSON"
  - "`project set` requires >=1 field via clap ArgGroup (Rule 2 deviation) — a bare set would PUT an empty {} modify of unknown server behavior"
  - "Action results keep ALL six keys present (null when absent) — the stable agent shape; passthrough extras (defaultDb etc.) stay at the client seam"
  - "percent-encoding declared direct at workspace level (transitive of url already — zero new code compiled)"

patterns-established:
  - "Percent-encode every {name} path segment via encode_segment (NON_ALPHANUMERIC) — 03-03 resource paths split on '/' and reuse it"
  - "Destructive project-family dispatch = sessions-terminate shape verbatim (guard before resolve_gateway_api)"
  - "Mutation actions read back via find rather than trusting unverified-LOW response bodies"

# Metrics
duration: 21min
completed: 2026-08-22
---

# Phase 3 Plan 1: Project CRUD Summary

**Full project CRUD (`ign project list/new/copy/rename/set/delete`) over the native `/data/api/v1/projects/*` family with recorded-request wire proofs, percent-encoded name segments, and the phase's first destructive command (`delete`) honoring both guard layers — CLI `--yes` refusal (exit 2, pre-resolution) and the wire's `confirm=true` query param**

## Performance

- **Duration:** 21 min
- **Started:** 2026-08-22T12:58:43Z
- **Completed:** 2026-08-22T13:19:24Z
- **Tasks:** 3
- **Files modified:** 13

## Accomplishments
- 7 GatewayApi capabilities (list/find/create/copy/rename/modify/delete) + `post_json`/`put_json` pipeline helpers, all contract-pinned by wiremock recorded-request proofs (spaced-name `My%20Project` exact path, bare create body exactly `{"name":"x","enabled":true}`, copy `fromName`/`toName`, rename `name`, modify-PUT carries no `name` key, DELETE `confirm=true` + empty body)
- Six serde-only action verbs with mutation→find read-back; stable six-key agent shape (`null` when absent); `SetOptions` only-Somes modify discipline unit-pinned
- `ign project` command tree with the destructive delete dispatch inherited verbatim from sessions-terminate (exit 2 + `confirmation_required` + profile null BEFORE any resolution — binary-smoke-proven); copy/rename/set ungarded per planner decision
- Goldens: list in all 3 render modes, new (exact create body via `body_json` matcher), set `--title` (PUT body exactly `{"title":"T"}`), delete-without-`--yes` envelope, delete-with-`--yes` proving `confirm=true` reached the wire, delete nonexistent exit 6
- `#[ignore]`-gated `live_projects_list` capture hook; README rows + destructive-family docs

## Task Commits

Each task was committed atomically:

1. **Task 1: project CRUD client capabilities (wiremock-pinned) + pipeline helpers** - `971b88b` (feat)
2. **Task 2: project actions (serde-only verb layer)** - `21745a6` (feat)
3. **Task 3: `ign project` command tree + destructive delete dispatch + goldens** - `e1b8eec` (feat)

**Plan metadata:** (see final commit below)

## Files Created/Modified
- `crates/ignition-core/src/client/projects.rs` - ProjectRecord/ProjectCreate/ProjectModify/ProjectCopy/ProjectRenameBody + verified path builders + encode_segment (257 lines)
- `crates/ignition-core/src/client/mod.rs` - 7 trait methods + impls in the ONE impl block; post_json/put_json helpers
- `crates/ignition-core/src/actions/projects.rs` - six verbs, ProjectSummary/ProjectSetResult (fields serde-skipped), ProjectsRig double (600 lines)
- `crates/ignition-core/tests/projects_contract.rs` - 11 wiremock contract tests incl. all recorded-request proofs
- `crates/ignition-cli/src/cli.rs` - ProjectArgs/ProjectCommand tree; set's ArgGroup
- `crates/ignition-cli/src/main.rs` - 6 ActionOutput variants + dispatch; delete guard before resolve_gateway_api
- `crates/ignition-cli/src/render.rs` - human table + five confirmation lines
- `crates/ignition-cli/tests/contract_projects.rs` - 7 binary-level golden tests (489 lines)
- `crates/ignition-core/tests/live_gateway.rs` - live projects/list capture hook
- `README.md` - project command rows; delete joins the `--yes` family; audit-log notes
- `Cargo.toml` / `crates/ignition-core/Cargo.toml` - percent-encoding declared direct

## Decisions Made
- **ProjectModify.enabled is `Option<bool>` (skip-if-none).** The plan sketch said modify = "create fields minus name, same skip discipline"; a literally-always-sent `enabled` would clobber the flag on single-field sets, and the plan's own Task-2 unit pin (`only --title` → exactly `{"title":"T"}`) requires enabled to be omissible. 
- **ProjectSetResult carries `fields: Vec<String>` as `#[serde(skip)]`.** Delivers the plan's human line `set <field(s)> on <name>` while the JSON data stays exactly the flat six-key record (agents never see a `fields` key).
- **Action results keep all six keys present (null when absent)** — the "agents must never key-hunt" stable-shape rule; `defaultDb`/`tagProvider`/`userSource` and other passthrough extras stay at the client seam, not the agent shape.
- **percent-encoding declared at workspace level** (already a transitive dep of url — declaring it direct compiles zero new code).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] `project set` requires at least one field flag (clap ArgGroup)**
- **Found during:** Task 3 (Set command definition)
- **Issue:** A bare `ign project set NAME` with no field flags would PUT an empty `{}` modify — unknown server behavior (item/wire semantics MEDIUM), wasting a round-trip and risking a confusing server error
- **Fix:** `#[command(group(ArgGroup::new("set_fields").required(true).multiple(true)))]` over the six field args — bare set is now a clap usage error (exit 2); covered by an assertion in `project_set_title_success_golden`
- **Files modified:** crates/ignition-cli/src/cli.rs, crates/ignition-cli/tests/contract_projects.rs
- **Verification:** `ign project set x` (no flags) exits 2 in the golden test
- **Committed in:** e1b8eec (part of task commit)

**2. [Rule 1 - Bug] Stub-insertion brace artifact cleaned across 8 rigs**
- **Found during:** Task 1 (doubles chore)
- **Issue:** The scripted insertion of the 7 `unreachable!` stubs produced redundant nested blocks (`{ unreachable!(…) }`) that trip clippy's `unnecessary braces` under `-D warnings`
- **Fix:** one sed normalization; all stubs byte-identical to the house style
- **Files modified:** crates/ignition-core/src/actions/{connections,doctor,inspect,logs,sessions,version}.rs
- **Verification:** clippy -D warnings clean; all 222 workspace tests green
- **Committed in:** 971b88b (part of task commit)

---

**Total deviations:** 2 auto-fixed (1 missing critical validation, 1 mechanical bug)
**Impact on plan:** Both fixes are correctness/hygiene scoped. No scope creep; no architectural change.

## Issues Encountered
None beyond the deviations above — the 11 core contract tests and 7 binary goldens passed on first execution after the build went green.

## Authentication Gates
None — all work was wiremock-first; no live gateway or credentials were required.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 03-02 (export/import) builds directly on this: `project_find` is the collision pre-check, `post_json`/`delete_with_query` demonstrate the query-param+body patterns, and the `ProjectMutationResult` read-back pattern is the model for import outcomes
- 03-03 (resources) reuses `encode_segment` per segment (keeping `/`) and inherits the doubles chore (now 9 rigs)
- Item shapes remain MEDIUM until live capture — the `live_projects_list` hook + `extra` passthrough make that correction cheap the moment a rig token exists (research Open Question 2/4)

---
*Phase: 03-project-operations*
*Completed: 2026-08-22*

## Self-Check: PASSED
