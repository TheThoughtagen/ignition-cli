---
phase: 03-project-operations
plan: 02
subsystem: projects
tags: [ignition, projects, export, import, zip, streaming, reqwest, wiremock, collision-policy, snapbox]

# Dependency graph
requires:
  - phase: 03-project-operations (plan 01)
    provides: project_find (the collision pre-check), post_json/put_json/delete_with_query pipeline patterns, encode_segment, ProjectsRig double, the destructive dispatch shape
provides:
  - download_to_file streaming pipeline helper (classify → bytes_stream → tokio::fs) — any future large-binary download reuses it
  - project_export_to_file (120 s) / project_import (300 s, raw application/zip body + overwrite query param) GatewayApi methods
  - 2 additive error slugs — project_exists (exit 6, action-built) and invalid_import_file (exit 2, usage-class)
  - static EXPORT_INCLUDES/EXCLUDES consts + ExportScope carried in BOTH export and import JSON data (roadmap criterion 4)
  - `ign project export NAME [-o FILE]` / `ign project import NAME --file PATH|- [--collision-policy abort|overwrite]` user commands with the full guard stack
  - CollisionPolicy conventions proven end-to-end (abort = find pre-check BEFORE upload; overwrite = --yes-guarded, no pre-check; merge = not a value, Designer-only)
affects: [03-project-operations (03-03 resources + e2e), phase-5-webdev-tags (project-zip import deploy spike), phase-6-tui]

# Tech tracking
tech-stack:
  added:
    - "reqwest `stream` feature (bytes_stream — feature-gated)"
    - "tokio `fs` + `io-std` + `io-util` features (streaming writes, stdin import)"
    - "futures-util 0.3 declared direct (transitive of reqwest stream already — zero new code compiled)"
  patterns:
    - "download_to_file: classify-FIRST then chunk-loop streaming — the export download pipeline that never buffers (Pitfall 2's structural answer)"
    - "import = buffered by design: Vec<u8> body ⇒ known Content-Length + Content-Type application/zip + overwrite QUERY param, per-request 300 s timeout (Pitfall 3's answer)"
    - "ImportOutcome opaque-success: object-JSON passes through, anything else normalizes to {\"status\":\"success\"} (agents always see a stable object)"
    - "export default naming: <name>.zip.part stream → atomic rename to sanitized Content-Disposition basename (path-strip; .\"..\"/empty refuse → fallback)"
    - "collision pre-check = action-layer find (abort) with the server as overwrite's authority — pre-check refusals carry the replace-not-merge hint"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/projects.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/actions/projects.rs
    - crates/ignition-core/tests/projects_contract.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-cli/tests/contract_projects.rs
    - crates/ignition-core/tests/live_gateway.rs
    - README.md
    - Cargo.toml
    - crates/ignition-core/Cargo.toml

key-decisions:
  - "ImportOutcome normalizes non-object 2xx bodies (incl. restart's literal `true`) to {\"status\":\"success\"} — \"parse if JSON\" alone would leak Value::Bool into the agent shape; objects pass through verbatim"
  - "ExportMeta.filename is Option<String> (plan sketched String): Content-Disposition is optional on the wire — the action resolves None → <name>.zip fallback; sanitize_basename strips path components from the header value before it names a local file"
  - "File/stdin read errors map to invalid_import_file (exit 2) — a bad --file path names what the caller must fix, same usage-class reasoning as the magic/size guards"
  - "project set's import-timeout hint stays OUT of the error contract (frozen): the \"verify with `ign project list`\" guidance rides the README timeout paragraph per the plan's planner note"
  - "snapbox `str!` normalizes backslashes in ACTUAL output to forward slashes (cross-platform path handling) — the `PK\\x03\\x04` message text goldens as `PK//x03//x04` (third snapbox gotcha, recorded inline at the golden)"

patterns-established:
  - "Streaming body consumption lives in download_to_file (the ONLY new body-consumption site): classify before any chunk, Network on stream error, Internal on local write error"
  - "Guard order for guarded-but-conditional destructives: the policy check (matches!(collision_policy, Overwrite)) gates require_confirmation BEFORE resolve_gateway_api — abort-policy arms skip the guard entirely"
  - "Doubles chore v2: python-scripted stub insertion keyed on each rig's project_delete stub (handles per-rig indentation); ProjectsRig upgraded to record find/export/import calls"

# Metrics
duration: 29min
completed: 2026-08-22
---

# Phase 3 Plan 2: Project Export/Import Summary

**Streaming ZIP export (`bytes_stream` → `tokio::fs`, zero `Vec<u8>` buffering, 120 s budget) + buffered stdin/file import (raw `application/zip` body, known Content-Length, 300 s budget) with an abort-by-default collision policy proven at both guard layers, and static `{includes, excludes}` scope metadata in every output naming tag-providers as gateway-config-not-project-export**

## Performance

- **Duration:** 29 min
- **Started:** 2026-08-22T13:22:10Z
- **Completed:** 2026-08-22T13:51:49Z
- **Tasks:** 3
- **Files modified:** 21 (13 substantive + 8 rig-stub files)

## Accomplishments
- `download_to_file` pipeline helper (classify-first → `bytes_stream` chunk loop → `AsyncWriteExt::write_all` → flush) + the workspace dep additions research flagged: reqwest `stream`, tokio `fs`/`io-std`/`io-util`, futures-util declared direct
- `project_export_to_file` / `project_import` client methods with per-request timeout overrides (120 s / 300 s — no second client, no global change) — wiremock-proven byte-for-byte export and recorded-request import proofs (exact encoded path, `overwrite=true`/`false` query variants, `application/zip`, `Content-Length`)
- 2 additive slugs in the exit table's two places: `project_exists` (exit 6, action-built like GatewayTooOld, hint names `--collision-policy overwrite` AND the replace-not-merge warning) and `invalid_import_file` (exit 2, PK-magic + 512 MB + unreadable-source guards, all BEFORE any network I/O)
- `ign project export/import` dispatch: overwrite guarded pre-resolution (exit 2, profile null — the LOCKED sessions-terminate shape), abort pre-checked via find with ZERO uploads on refusal (expect(0) mock proof), stdin import, export default naming via sanitized disposition basename with atomic `.part` rename
- 7 new binary goldens + 6 new client contract tests + 8 new action unit tests; live round-trip hook (`IGNITION_LIVE_MUTATIONS=1`) previewing 03-03's e2e loop

## Task Commits

Each task was committed atomically:

1. **Task 1: streaming download_to_file + export/import client methods** - `9e3a1e3` (feat)
2. **Task 2: additive error slugs + export/import actions with scope metadata** - `3e23762` (feat)
3. **Task 3: `ign project export/import` dispatch + guards + goldens** - `712ea2a` (feat)

**Plan metadata:** (see final commit)

## Files Created/Modified
- `crates/ignition-core/src/client/mod.rs` - download_to_file helper + 2 trait methods/impls in the ONE impl block
- `crates/ignition-core/src/client/projects.rs` - PROJECT_EXPORT/IMPORT_TIMEOUT consts, ExportMeta/ImportOutcome, export/import path builders
- `crates/ignition-core/src/error.rs` - ProjectExists + InvalidImportFile variants wired through code/exit_code/hint/enumerated test
- `crates/ignition-core/src/actions/projects.rs` - scope consts, CollisionPolicy, magic/size guards, export/import actions, sanitize/fallback helpers, recording rig (now ~1130 lines)
- `crates/ignition-core/tests/projects_contract.rs` - 6 export/import wiremock proofs incl. Content-Length + opaque-success fallback
- `crates/ignition-cli/src/cli.rs` - Export/Import commands + CollisionPolicy ValueEnum + From conversion
- `crates/ignition-cli/src/main.rs` - dispatch arms (stderr progress, pre-resolution guard, stdin/file read), 2 ActionOutput variants
- `crates/ignition-cli/src/render.rs` - export (artifact + scope lines) / import human renderers
- `crates/ignition-cli/tests/contract_projects.rs` - 7 goldens + ign_in/ign_stdin/zip_fixture helpers (now ~860 lines)
- `crates/ignition-core/tests/live_gateway.rs` - mutation-gated export→import round-trip + live_mutations_enabled gate
- `README.md` - exit-table rows, command rows, timeout/scope/collision sections, destructive-ops update
- `Cargo.toml` / `crates/ignition-core/Cargo.toml` - the streaming dep additions
- 8 action-module rig files - the doubles chore (2 stubs each)

## Decisions Made
- **ImportOutcome normalizes non-object bodies.** "Parse if JSON" alone turned restart's literal `true` into `Value::Bool(true)` in the agent shape — object-JSON passes through; everything else becomes `{"status":"success"}` (caught by a first-run contract-test failure, fixed at the impl).
- **ExportMeta.filename is `Option<String>`** (the plan's sketch said `String`): the disposition header is optional on the wire, and the action needs the absent case to pick the `<name>.zip` fallback anyway.
- **Local read errors ride `invalid_import_file`.** A missing/unreadable `--file` path is the same usage-class caller error as a non-ZIP payload.
- **The import-timeout verify guidance lives in the README**, not the error hint — the error contract is frozen and the plan's planner note explicitly routed it there.
- **snapbox's backslash normalization** (actual `\` → `/`) recorded at the golden: message text `PK\x03\x04` goldens as `PK//x03//x04`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Flaky poll test: fast_cfg deadline 500 ms → 5 s**
- **Found during:** Task 1 (workspace verification)
- **Issue:** `poll::tests::transient_errors_are_retried_then_done` failed intermittently on clean HEAD too — the Network/Restarting steps build REAL transport errors (TCP connect to a refused port), which can take tens of ms each under parallel test load and blow the 500 ms deadline
- **Fix:** deadline bumped to 5 s with a comment naming the mechanism (the config's fast path is the sleep/backoff, not the deadline); verified 8/8 green runs after
- **Files modified:** crates/ignition-core/src/poll.rs
- **Verification:** 8 consecutive green runs of the poll module + full workspace green
- **Committed in:** 9e3a1e3 (part of task commit)

**2. [Rule 1 - Bug] ImportOutcome leaked `Value::Bool` into the agent shape**
- **Found during:** Task 1 (first contract-test run)
- **Issue:** the literal `true` 2xx body parses as valid JSON, so "parse if JSON" returned `Bool(true)` — not the documented fallback object
- **Fix:** parse accepts only object JSON; everything else (bool/string/non-JSON) normalizes to `{"status":"success"}`
- **Files modified:** crates/ignition-core/src/client/mod.rs
- **Verification:** `project_import_non_json_body_falls_back_to_success` green
- **Committed in:** 9e3a1e3 (part of task commit)

---

**Total deviations:** 2 auto-fixed (2 bugs — one pre-existing flake surfaced by this plan's test load, one first-run behavior correction)
**Impact on plan:** Both are correctness-scoped with zero scope creep. The poll fix repairs a pre-existing flake (verified failing on clean HEAD before the fix).

## Issues Encountered
None beyond the deviations — the 6 client contract tests, 8 action unit tests, and 7 binary goldens passed on first execution after each build went green (the ImportOutcome fix above was the one iteration).

## Authentication Gates
None — all work was wiremock-first; the live round-trip hook is `#[ignore]`-gated and needs `IGNITION_LIVE_URL`/`IGNITION_LIVE_TOKEN`/`IGNITION_LIVE_MUTATIONS=1` when a rig token next exists.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 03-03 (resources + e2e) inherits: `encode_segment` for resource paths (keeping `/`), the recording-rig pattern, the import/export actions its e2e loop drives, and the live-mutations gate convention (`IGNITION_LIVE_MUTATIONS=1`)
- Export/import response bodies remain MEDIUM until live capture — the round-trip hook pins them the moment a token exists (research Open Question 3, incl. overwrite's wipe semantics)
- Phase 5's WebDev deploy spike can evaluate the project-zip import path with this plan's import as the working primitive

---
*Phase: 03-project-operations*
*Completed: 2026-08-22*

## Self-Check: PASSED
