---
phase: 05-webdev-backend-tag-operations
plan: 05
subsystem: api
tags: [webdev, tagconfig, ignition, udt, json, collision-policy, clap, wiremock, snapbox]

# Dependency graph
requires:
  - phase: 05-04
    provides: the require_routes precondition template, the tags family CLI chassis, browse/read/write actions, provider CRUD
  - phase: 05-03
    provides: webdev_route_call generic seam + webdev_precondition shared helper
  - phase: 05-01
    provides: the tagConfig route source (seven-action dispatcher) in the embedded bundle
  - phase: 03-02
    provides: the LOCKED collision conventions (abort pre-check / overwrite guarded no-pre-check) + CollisionPolicy enum
provides:
  - "tag config CRUD (get/create/edit/delete) riding the tagConfig route — the surgical get→edit-file→write-back loop"
  - "stringified value/defaultValue re-parse (recursive, object/array-only) so agents see real JSON"
  - "UDT types list + recursive definitions (parameters + nested children, same re-parse)"
  - "bulk tags export (file/stdout, default <last-segment>.json, parsed+validated payload) and import with the Phase-3 collision matrix mapped onto configure's a/o"
  - "tag_collision additive exit-6 slug (error.rs + README, enumerated-test pinned)"
  - "the fourth sanctioned stdout exception (tags export -o - raw payload, pipeable into import --file -)"
  - "the live round-trip e2e gate (create → read → export → abort/abort-refusal/overwrite import → read-back oracle)"
affects: [05-06 (alarms/tagHistory inherit the precondition + family patterns), TUI phase (tag surface consumption), MCP parity surface]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "stringified-JSON re-parse: recursive object/array-only rewrite of value/defaultValue strings at the action layer (semantics preserved — scalar parses and unparseable strings stay strings)"
    - "path→configure operand split: basePath+name derived from the tag path (bare paths ride [default]; path-derived name wins over the definition's)"
    - "collision matrix on the route seam: browse pre-check (read) refusing tag_collision before any configure write — the 03-02 find-precheck shape mapped onto WebDev"
    - "response-queue action double: the TagsRig serves sequential per-call payloads (the export→import round-trip unit)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-core/src/client/tags.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/tests/tags_contract.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_tags.rs
    - crates/ignition-cli/tests/e2e_webdev.rs
    - README.md

key-decisions:
  - "Export modes: -o FILE writes the file, no -o defaults to <last-segment>.json in the cwd, -o - prints the raw pretty payload in EVERY mode — the FOURTH sanctioned stdout exception, chosen so `tags export -o - | tags import --file -` pipes (symmetric with the --file - stdin convention)"
  - "create = configure 'a' (server-side abort backstop), edit = configure 'o' scoped to the single node and NOT --yes-guarded (single-node edit ≠ project-wide destructive); config delete IS the 7th guarded destructive verb"
  - "Export payloads ride VERBATIM (stringified values intact) for import fidelity — the re-parse is a getConfig/getUDTDefinition read-side presentation only"
  - "tag_collision is additive exit 6 with the colliding names in the message and --collision-policy overwrite in the hint (two-places rule synced)"
  - "Import reuses actions::projects::CollisionPolicy verbatim (one enum, the same two labels) rather than forking a tags-local copy"

patterns-established:
  - "Route-action response shapes validated with Internal-class honesty errors (missing config/deleted/payload/definition keys) — never silently defaulted"
  - "JSON document inputs (--file PATH|-) read AND parsed pre-resolution in dispatch (read_json_input), extending the resource-put byte-source precedent"

# Metrics
duration: 39min
completed: 2026-08-25
---

# Phase 5 Plan 5: Tag Config CRUD, UDTs, Bulk Export/Import Summary

**Tag config CRUD + UDT types/definitions + bulk export/import over the tagConfig route, with the recursive stringified-JSON re-parse, the Phase-3 collision matrix mapped onto configure's 'a'/'o' (zero-write abort proof), and the live export→import round-trip gate.**

## Performance

- **Duration:** 39 min
- **Started:** 2026-08-25T11:19:02Z
- **Completed:** 2026-08-25T11:58:22Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

- **TAGS-05 — the surgical tag-edit loop**: `tags config get|create|edit|delete` ride the tagConfig route's getConfig/configure/deleteTags actions with path-derived basePath+name operands; configs come back with stringified `value`/`defaultValue` re-parsed into real JSON (recursively — nested children and UDT parameters included; scalar-parse and unparseable strings stay strings).
- **TAGS-06 — UDT surface**: `tags udt types` (the `_types_` browse) and `tags udt def` (recursive definitions with the same re-parse).
- **TAGS-09 — provider portability**: `tags export` (payload parsed + validated as a subtree list, written pretty to `-o FILE` / default `<last-segment>.json` / raw to stdout via `-o -`) and `tags import` with the LOCKED collision matrix — abort pre-checks via browse and refuses `tag_collision` (exit 6, names + overwrite hint) BEFORE any write; overwrite is `--yes`-guarded with no pre-check.
- **The live round-trip gate** (e2e_webdev.rs): create Int4/123 → read Good/123 → export → import (abort success, abort collision refusal, overwrite `--yes`) → read `[p5import]T1 == 123/Good` → cleanup. The research-proven loop, binary-driven.

## Task Commits

Each task was committed atomically:

1. **Task 1: config get/create/edit/delete actions** - `a64e134` (feat)
2. **Task 2: UDT types/def + bulk export/import with collision policy** - `ca48e9d` (feat)
3. **Task 3: CLI arms, goldens, live round-trip gate** - `f073f0b` (feat)

**Plan metadata:** (see final commit)

## Files Created/Modified

- `crates/ignition-core/src/actions/tags.rs` - config CRUD, UDT, export/import actions; reparse_stringified + split_base_path + default_export_file_name pure helpers; the response-queue test rig + 16 new unit tests
- `crates/ignition-core/src/error.rs` - TagCollision variant (exit 6, `tag_collision` slug, overwrite hint), enumerated-test case
- `crates/ignition-core/src/client/tags.rs` - module docs (tagConfig half rides the generic seam — no new client models)
- `crates/ignition-core/tests/tags_contract.rs` - wiremock pins: getConfig re-parse fixture, create/edit body pins, deleteTags pin, UDT pins, export pin, the zero-configure-writes collision proof, overwrite no-precheck pin
- `crates/ignition-cli/src/cli.rs` - TagsConfigCommand / TagsUdtCommand / Export / Import arms
- `crates/ignition-cli/src/main.rs` - dispatch (guards, read_json_input pre-resolution, export -o resolution), 8 new ActionOutput variants
- `crates/ignition-cli/src/render.rs` - the fourth stdout exception + human renderers (pretty JSON config/def, udt name list, artifact/counts lines)
- `crates/ignition-cli/tests/contract_tags.rs` - 7 new golden tests (14 total)
- `crates/ignition-cli/tests/e2e_webdev.rs` - the live round-trip gate + stdin/err-envelope helpers
- `README.md` - 8 command rows, configure-shape traps table, bulk transfer section, exit-table sync, streaming exception #4, destructive-ops sync

## Decisions Made

- **Export mode resolution**: `-o FILE` → file; no `-o` → default `<last-segment>.json` in the cwd (export-streaming convention); `-o -` → the raw pretty payload on stdout in EVERY mode (the fourth sanctioned stdout exception) — this makes `tags export -o - | tags import --file -` a working pipe, symmetric with the `--file -` stdin convention elsewhere. The plan's "out file or stdout (file default `<last-segment>.json`)" maps onto exactly these three modes.
- **Create vs edit policy chars**: create = `'a'` (abort — refusing to clobber is the server-side backstop), edit = `'o'` scoped to the single named node and NOT `--yes`-guarded (a single-node edit is not a project-wide destructive — the guard set's line); `tags config delete` and `tags import --collision-policy overwrite` ARE guarded pre-resolution.
- **Export payload verbatim, read-side re-parse only**: the export/import interchange keeps the gateway's stringified values untouched (configure accepts them natively — round-trip fidelity); the re-parse exists so getConfig/getUDTDefinition consumers see real JSON.
- **CollisionPolicy reuse**: `tags_import` takes `actions::projects::CollisionPolicy` verbatim (same two values, same labels) — one enum across both import verbs.
- **tag_collision shape**: provider + colliding names in the message (agents see WHAT collided) + `--collision-policy overwrite` in the hint; endpoint carries the pre-check browse URL.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] clap-idiomatic guard match arm**
- **Found during:** Task 3 (CLI dispatch)
- **Issue:** clippy's `redundant_guards` rejected the `if matches!(...)` guard on the import-overwrite arm
- **Fix:** direct pattern match on `collision_policy: CollisionPolicy::Overwrite` in the match arm
- **Files modified:** crates/ignition-cli/src/main.rs
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` clean
- **Committed in:** f073f0b (Task 3 commit)

**2. [Rule 1 - Bug] Test-rig ergonomics for sequential route responses**
- **Found during:** Task 2 (round-trip unit test)
- **Issue:** the TagsRig served one fixture for ALL route calls — the export→import round-trip needs sequential different answers (export payload, then browse, then configure)
- **Fix:** added a per-call response queue to the rig (pops the front; exhausted queue reuses the last answer)
- **Files modified:** crates/ignition-core/src/actions/tags.rs
- **Verification:** export_import_round_trip_shapes_match passes — configure tags == parsed export payload verbatim
- **Committed in:** ca48e9d (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both mechanical — no scope creep, no contract drift.

## Issues Encountered

- snapbox `str!` backslash normalization hit the export stdout golden (the documented 03-02 gotcha #3) — the stringified value's `\"` escapes golden as `/"`; recorded inline at the golden. serde_json's default BTreeMap key ordering (alphabetical) required golden ordering care — noted in the fixed goldens.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- TAGS-05/06/09 complete — the tag surface is whole: providers, browse/read/write, config CRUD, UDTs, bulk transfer, all riding the deployed bundle.
- 05-06 (alarms + tagHistory) is the last plan of the phase and inherits every pattern here: the require_routes precondition, the route-call action shape, and the family's CLI chassis.
- The live e2e gates (webdev loop, tags provider loop, config round-trip) are opt-in and green no-ops without env vars; the round-trip gate doubles as TAGS-09's live proof when a rig is available.

---
*Phase: 05-webdev-backend-tag-operations*
*Completed: 2026-08-25*

## Self-Check: PASSED

All key-files modified exist on disk; all three task commits (a64e134, ca48e9d, f073f0b) present in git log.
