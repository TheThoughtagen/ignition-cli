---
phase: 03-project-operations
plan: 03
subsystem: api
tags: [ignition, resources, rest, percent-encoding, content-sniffing, assert_cmd, e2e, openapi, wiremock]

# Dependency graph
requires:
  - phase: 03-01
    provides: encode_segment per-segment encoder, project CRUD family, destructive dispatch shape
  - phase: 03-02
    provides: export/import machinery (e2e fixtures), ProjectsRig export/import recording
provides:
  - ign resource list/get/put/delete — the surgical edit loop (PROJ-05)
  - ResourceBinary refusal (exit 6) fencing data.bin-class resources out of the JSON loop
  - put_bytes raw-body pipeline helper on GatewayApi's impl
  - e2e harness skeleton (tests/e2e_projects.rs) Phases 4–5 extend
  - openapi live-capture gate closing the resource-family wire-truth question
affects: [04-rig, 05-webdev-tags, 07-interop]

# Tech tracking
tech-stack:
  added: [reqwest as ignition-cli dev-dep (openapi capture fetch)]
  patterns:
    - "content sniffer: NUL-in-first-8KiB → Binary refusal; UTF-8+JSON-parse → Json; UTF-8 → Text (classify discipline inverted)"
    - "resource paths: per-segment encode_segment split on '/', slashes preserved (mcp quote(path, safe='/'))"
    - "e2e mutation gating: IGNITION_LIVE_MUTATIONS=1 on top of URL+TOKEN (02-04 precedent, now the harness convention)"

key-files:
  created:
    - crates/ignition-core/src/client/resources.rs
    - crates/ignition-core/src/actions/resources.rs
    - crates/ignition-core/tests/resources_contract.rs
    - crates/ignition-cli/tests/contract_resources.rs
    - crates/ignition-cli/tests/e2e_projects.rs
  modified:
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/actions/mod.rs
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - README.md

key-decisions:
  - "Resource get result is the flat stable shape {project, path, content_kind, content} (all keys always present) rather than the plan's enum sketch — the family convention (identity fields + agents never key-hunt) wins over the incidental sketch"
  - "Added InvalidInput (exit 2, slug invalid_input) for put's unreadable --file/stdin — reusing import-specific invalid_import_file would mislabel; additive slugs are the sanctioned growth path"
  - "Resource paths over-encode (dots ride as %2E etc.) — encode_segment is THE one encoder per the locked 03-01 decision; over-encoding is safe (server decodes before matching)"
  - "put is NOT --yes-guarded (explicit-content upsert per planner decision); delete is the family's destructive verb with the LOCKED pre-resolution guard"
  - "Sniffer boundary pinned honestly: a lone NUL past the 8 KiB window in otherwise-UTF-8 input classifies Text — real data.bin magic lands well inside the window"

patterns-established:
  - "Binary fencing: get AND put both refuse via ResourceBinary before any transformation — binary resources belong to export/import only"
  - "e2e two-sided replace-not-merge pin: pre-export resource survives AND post-export resource is not_found after overwrite import"
  - "Openapi capture gate: #[ignore] test writing a trimmed family extract into the phase dir the moment a token exists"

# Metrics
duration: 29min
completed: 2026-08-22
---

# Phase 3 Plan 3: Surgical Resource Loop + E2E Harness Summary

**`ign resource list/get/put/delete` surgical edit loop with binary-resource refusal (exit 6), wiremock-pinned MEDIUM-confidence client capabilities, and the phase's assert_cmd e2e harness + openapi live-capture gate**

## Performance

- **Duration:** 29 min
- **Started:** 2026-08-22T13:54:44Z
- **Completed:** 2026-08-22T14:24:03Z
- **Tasks:** 3
- **Files modified:** 12 (5 created, 7 modified)

## Accomplishments
- The surgical resource loop (PROJ-05): a user edits one view/script from the terminal (get → edit → put) without re-importing anything, deletes surgical resources under the LOCKED --yes guard
- Binary resources fenced off in BOTH directions (get and put refuse exit 6 `resource_binary` with the export/import hint) — a data.bin resource can never be corrupted through the JSON loop (Pitfall 7)
- The e2e harness skeleton exists and is green by default: the full create→edit→export→import(abort/overwrite)→rename→copy→delete loop pins the replace-not-merge contract two-sidedly; the openapi-extract gate closes the resource-endpoint wire-truth question (research Open Question 1) the moment a rig token exists

## Task Commits

Each task was committed atomically:

1. **Task 1: resource-family client capabilities (wiremock-first, MEDIUM-flagged)** - `7c5e829` (feat)
2. **Task 2: resource actions (binary sniffer) + `ign resource` dispatch + goldens** - `5d4b50d` (feat)
3. **Task 3: e2e harness skeleton + openapi live-capture gate** - `0e118e3` (feat)

**Plan metadata:** `ea83c80` (docs: complete plan)

## Files Created/Modified
- `crates/ignition-core/src/client/resources.rs` - ResourceEntry (typed path + passthrough) + ResourceContent (raw bytes) + per-segment path encoder; MEDIUM-flagged
- `crates/ignition-core/src/client/mod.rs` - 4 trait methods + impls + put_bytes raw-body pipeline helper
- `crates/ignition-core/src/error.rs` - ADDITIVE ResourceBinary (exit 6) + InvalidInput (exit 2), taxonomy-enumerated
- `crates/ignition-core/src/actions/resources.rs` - classify_content sniffer + list/get/put/delete actions with stable agent shapes
- `crates/ignition-cli/src/cli.rs` - Commands::Resource (list/get/put/delete; path keeps slashes)
- `crates/ignition-cli/src/main.rs` - dispatch: delete guarded pre-resolution; put reads file/stdin in dispatch
- `crates/ignition-cli/src/render.rs` - one path per line (list), pretty JSON/raw text (get), put/deleted lines
- `crates/ignition-core/tests/resources_contract.rs` - 8 wiremock pins incl. recorded-request encoding proofs
- `crates/ignition-cli/tests/contract_resources.rs` - 11 goldens incl. binary refusals + exit-2 confirmation
- `crates/ignition-cli/tests/e2e_projects.rs` - the #[ignore] e2e loop + openapi capture gate
- `README.md` - resource rows, exit-table slugs, surgical-edit example, MEDIUM caveat, destructive-ops update

## Decisions Made
- **Flat get-result shape over the plan's enum sketch** — `{project, path, content_kind, content}` keeps the family convention (identity fields, all keys always present); the plan's `ResourceGetResult::Json(Value)` sketch was incidental, the stable-agent-shape contract is locked
- **New `invalid_input` slug (exit 2)** — put's unreadable byte source is caller-fault usage-class; the import-specific `invalid_import_file` hint would actively mislead. Additive slugs are the sanctioned growth path (FROZEN contract)
- **Over-encoding embraced** — resource paths ride through the one locked encoder (NON_ALPHANUMERIC), so `.` → `%2E`, `-` → `%2D`; safe (server decodes before matching) and it keeps exactly one encoder in the codebase
- **Sniffer's honest boundary** — a NUL past the first 8 KiB in otherwise-valid UTF-8 classifies Text (NUL is valid UTF-8); the unit test pins this boundary rather than pretending it doesn't exist

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added `InvalidInput` error variant for put's byte source**
- **Found during:** Task 2 (dispatch: put reads `--file`/stdin)
- **Issue:** Plan specified reading input in the dispatch layer but no error class for an unreadable file/stdin — `invalid_import_file`'s slug/hint are import-specific and would mislabel; `Internal` would misclassify a caller fault as a bug
- **Fix:** ADDITIVE `CoreError::InvalidInput` (exit 2, slug `invalid_input`, hint names `--file`/stdin); enumerated test + README exit-table synced (two-places rule); golden-pinned (`resource_put_missing_file_exits_2_golden`)
- **Files modified:** crates/ignition-core/src/error.rs, README.md, crates/ignition-cli/tests/contract_resources.rs
- **Verification:** exit-2 golden with zero network I/O
- **Committed in:** 5d4b50d (Task 2 commit)

**2. [Rule 3 - Blocking] Added reqwest as ignition-cli dev-dependency**
- **Found during:** Task 3 (openapi-capture gate)
- **Issue:** The capture test needs a plain authed HTTP GET; ignition-cli had no reqwest dep (wiremock's transitive copy isn't importable)
- **Fix:** `reqwest = { workspace = true }` under [dev-dependencies]
- **Files modified:** crates/ignition-cli/Cargo.toml, Cargo.lock
- **Verification:** capture test compiles + skips cleanly without env
- **Committed in:** 0e118e3 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 missing critical, 1 blocking)
**Impact on plan:** Both required for correctness/completion; no scope creep. The new slug is additive per the FROZEN contract's sanctioned growth path.

## Issues Encountered
- snapbox's `str!` backslash normalization struck again: escaped quotes in the resource_binary message goldens must be spelled `/"` (the 03-02 `PK//x03//x04` precedent now covers quotes too)
- Initial wiremock put-fixture used the un-encoded path (`script-python`) while the client correctly over-encodes `-` → `%2D`; the exact-path matcher caught it — the test now pins the encoded form (the recorded-request discipline working as designed)
- A first-draft sniffer unit test assumed `from_utf8` rejects NUL (it does not — 0x00 is valid UTF-8); the test now pins the honest 8-KiB-window boundary instead

## User Setup Required

**Live verification is opt-in (NOT required — wiremock covers the contract).** See the plan frontmatter `user_setup`:
- `IGNITION_LIVE_URL` (e.g. http://localhost:18088 — the `ign-research` rig is RUNNING) + `IGNITION_LIVE_TOKEN` (Gateway UI → Platform → Security → API Keys → Create, Basic Token, UNCHECK 'Require secure connections' for http rigs, copy the FULL name:key string) unlocks the openapi capture + read checks
- `IGNITION_LIVE_MUTATIONS=1` additionally unlocks the full e2e loop
- Run: `cargo test -p ignition-cli --test e2e_projects -- --ignored` — the openapi extract lands in this phase dir and closes Open Question 1

## Next Phase Readiness
- Phase 3 is COMPLETE: projects (03-01), export/import (03-02), and the surgical resource loop (03-03) replace the webpage's project management end-to-end
- The e2e harness skeleton is the stated deliverable Phases 4 (rig) and 5 (webdev deploy) extend — shared env helpers + one test per capability loop
- Open items (all gated, none blocking): live capture of the resource family + populated list shapes; the ~1-min token creation documented in user_setup

---
*Phase: 03-project-operations*
*Completed: 2026-08-22*

## Self-Check: PASSED

All 6 key files exist on disk; all 3 task commits (7c5e829, 5d4b50d, 0e118e3) verified in git log.
