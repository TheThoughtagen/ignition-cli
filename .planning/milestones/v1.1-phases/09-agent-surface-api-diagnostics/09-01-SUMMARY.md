---
phase: 09-agent-surface-api-diagnostics
plan: 01
subsystem: api
tags: [error-taxonomy, exit-codes, classify, reqwest, wiremock, gateway-client]

# Dependency graph
requires:
  - phase: 08-foundations-session-core-config-contract
    provides: executable Three-Place slug rule (readme_exit_table_agreement + exit_code_mapping_enumerated) and the frozen 1–7 exit taxonomy
  - phase: 02-core-client-classifier
    provides: the classify() chokepoint, send_and_classify pipeline, IgnitionMock wiremock harness
provides:
  - CoreError::GatewayClientError variant — slug gateway_client_error, exit 2, carries the gateway's verbatim 4xx body (4 KiB cap + explicit ASCII truncation marker via truncate_api_body)
  - pub send_and_classify_for_api — the api-call-scoped pipeline entry 09-03's `ign api call` consumes
  - api-call-scoped classify fallback arm (api_call parameter; curated pipeline passes false)
  - wiremock contract file pinning the full api-call exit partition + curated non-leak regression
  - README exit-2 row carries gateway_client_error (P9)
affects: [09-02, 09-03, 14-transports-mcp, exit-code contract consumers, envelope goldens]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Parameter-scoped classifier arm: the catch-all cannot fire without the api_call flag — scope-by-parameter beats scope-by-URL-sniffing or post-hoc Internal re-mapping"
    - "Cap-at-construction: truncate_api_body is the ONE body-capping site, so the variant always carries its final form (unit-tested)"

key-files:
  created:
    - crates/ignition-core/tests/api_classify_contract.rs
  modified:
    - crates/ignition-core/src/error.rs
    - crates/ignition-core/src/client/classify.rs
    - crates/ignition-core/src/client/mod.rs
    - README.md

key-decisions:
  - "GatewayClientError rides exit 2 (usage class) with its own slug — the poll_interval_too_small additive-slug precedent; the 1–7 taxonomy stays frozen"
  - "The catch-all is parameter-scoped (api_call: bool on classify), not global — curated commands' 4xx/exit-1 semantics provably unchanged (non-leak regression pinned)"
  - "All three classify call sites pass api_call=false (plan said one existed; webdev_route_call and webdev_route_probe were the two others)"
  - "send_and_classify_for_api is pub — integration tests and 09-03's action layer consume it across the crate boundary"
  - "Body cap (4 KiB) + ASCII marker applied at construction via truncate_api_body; marker is ASCII-pinned for golden safety"

patterns-established:
  - "Api-call pipeline entry: send_and_classify_for_api(request, url) is the single head every later `ign api call` task rides"
  - "Exit-partition-as-table: the full status→(exit,slug) mapping is pinned as exhaustive wiremock tests, not spot checks"

# Metrics
duration: 210min
completed: 2026-09-07
---

# Phase 9 Plan 01: Agent Surface API Diagnostics Summary

**GatewayClientError (exit 2, slug gateway_client_error, verbatim 4 KiB-capped body) wired through an api-call-scoped classify arm with the full exit partition wiremock-pinned and the curated pipeline provably unchanged**

## Performance

- **Duration:** 210 min
- **Started:** 2026-09-07T01:44:49Z
- **Completed:** 2026-09-07T05:14:53Z
- **Tasks:** 2
- **Files modified:** 5 (2 modified + 1 created source/test, + README)

## Accomplishments

- New `CoreError::GatewayClientError` variant: an unclassified gateway 4xx on the api-call path is now structurally impossible to render as internal/exit-1 — it rides exit 2, slug `gateway_client_error`, with the gateway's own body verbatim (capped at 4 KiB with an explicit `... [truncated]` marker)
- The Three-Place slug rule landed in ONE atomic task: enum variant + `code()`/`exit_code()`/`hint()` arms + `exit_code_mapping_enumerated` literal triple + `EXIT_SLUG_LITERALS` row + error.rs doc-taxonomy row + README exit-2 row — both CI agreement tests green
- `pub send_and_classify_for_api` is the api-call pipeline entry (identical transport-error mapping; `api_call = true`), ready for 09-03's `ign api call` with zero CLI surface in this plan
- Full exit partition pinned as wiremock contract tests: 400/405/409/415 → GatewayClientError exit 2; 401/403 → Auth exit 5; 404 → NotFound exit 6; 503 → GatewayRestarting exit 6; 500 → Internal exit 1; Jetty-HTML 400 rides verbatim; oversized bodies truncate to cap + marker

## Task Commits

Each task was committed atomically:

1. **Task 1: GatewayClientError variant + exit-2 slug — Three-Place rule in ONE task** - `e6d6644` (feat)
2. **Task 2: api-call-scoped classify arm + send_and_classify_for_api + partition contract tests** - `43ba465` (feat)

## Negative Proofs (per plan)

- **Task 1:** temporarily dropped the README `gateway_client_error` slug → `readme_exit_table_agreement` FAILED with exactly `README exit-2 row is missing slug "gateway_client_error"` → restored → green
- **Task 2:** temporarily dropped the `api_call &&` guard in the classify fallback → `curated_pipeline_keeps_internal_on_unclassified_400` FAILED (the curated 400 leaked into `gateway rejected the api call (HTTP 400 …)`) → restored → green

## Files Created/Modified

- `crates/ignition-core/src/error.rs` — GatewayClientError variant, GATEWAY_CLIENT_BODY_CAP_BYTES/TRUNCATION_MARKER consts, truncate_api_body helper, code()/exit_code()/hint() arms, enumerated triple, literal-table row, doc-taxonomy row, truncates_at_cap_with_marker unit test
- `crates/ignition-core/src/client/classify.rs` — api_call parameter, dispatch-doc rule 6b, catch-all guard before the HTML-sniff fallback
- `crates/ignition-core/src/client/mod.rs` — send_and_classify passes false; webdev_route_call/webdev_route_probe pass false; new pub send_and_classify_for_api
- `crates/ignition-core/tests/api_classify_contract.rs` — 5 wiremock contract tests: partition (catch-all half + preserved-arms half), Jetty-HTML verbatim, truncation, curated non-leak
- `README.md` — exit-2 row carries `gateway_client_error` (P9)

## Decisions Made

- Exit 2 / slug `gateway_client_error` per plan (usage class: a 4xx is the caller's problem); additive-slug mechanism keeps the frozen taxonomy
- Parameter-scoped catch-all (research Pattern 1 option (a)): explicit `api_call: bool` beats string-parsing an Internal error message
- Cap enforced at construction (`truncate_api_body`), char-boundary-safe, marker ASCII-pinned
- `send_and_classify_for_api` public: integration tests pin the partition through it and 09-03 consumes it from the ignition-cli crate

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] classify() had three call sites, not one**
- **Found during:** Task 2 (signature change)
- **Issue:** The plan said "update the ONE existing call site (send_and_classify at client/mod.rs:677-687)", but `classify::classify` is also called by `webdev_route_call` (client/mod.rs:942) and the `webdev_route_probe` fallback (client/mod.rs:1013); the signature change would not compile without touching them
- **Fix:** Both additional curated call sites pass `api_call = false` — behavior byte-for-byte identical (and exactly what the plan's non-leak intent requires)
- **Files modified:** crates/ignition-core/src/client/mod.rs
- **Verification:** workspace green including all webdev contract tests
- **Committed in:** 43ba465 (Task 2 commit)

**2. [Rule 2 - Missing Critical] send_and_classify_for_api made pub**
- **Found during:** Task 2 (test authoring)
- **Issue:** The plan's contract tests call `send_and_classify_for_api` directly from `tests/` — integration tests can only see the crate's public API, so a private method would not compile
- **Fix:** `pub async fn send_and_classify_for_api` with a doc comment naming 09-03 as the production consumer (also required later: ignition-cli is a separate crate)
- **Files modified:** crates/ignition-core/src/client/mod.rs
- **Verification:** api_classify_contract tests compile and pass through the public entry
- **Committed in:** 43ba465 (Task 2 commit)

**3. [Rule 1 - Bug] Clippy doc-lint failures on the new module-doc list item**
- **Found during:** Task 2 verification (cargo clippy -D warnings)
- **Issue:** The "6b." pseudo-list marker is not valid CommonMark ordered-list syntax; clippy::doc_lazy_continuation/doc_overindented_list_items failed the build across three rewrites (missing blank-line separator, wrong continuation indent)
- **Fix:** Restructured as a `**6b.**` bold-prefixed paragraph with a blank-line separator from the list — same content, parseable markdown
- **Files modified:** crates/ignition-core/src/client/classify.rs
- **Verification:** cargo clippy --all-targets -- -D warnings green
- **Committed in:** 43ba465 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking, 1 missing-critical, 1 bug)
**Impact on plan:** All three were forced by the plan's own verification gates (compile, public-API test access, clippy -D warnings). No scope creep; no behavior beyond the plan.

## Issues Encountered

- **Transient workspace flake:** `from_export_legacy_layout_and_filter` (crates/ignition-cli/tests/contract_tags.rs:1979) failed ONCE inside the full parallel `cargo test --workspace` run (stdout contained "T1"), then passed in isolation, passed with the full 30-test binary, and the complete workspace re-run finished green (WS_EXIT:0, 51 test binaries). The test is pure offline fixture parsing (no HTTP/classify path), so the single failure is parallel-execution interference under load, unrelated to this plan's diff. Worth an audit if it recurs.
- Build times are long on this machine (clippy --all-targets ~4-5 min; full workspace suite ~15+ min) — verification was executed via background runs with polling; no functional impact.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 09-02 (live rig wire-captures) landed its docs commit already; 09-03's `ign api call` consumes `send_and_classify_for_api` + `GatewayClientError` exactly as planned — the happy path can no longer ship with an exit-1 storm on unclassified gateway rejections
- README exit table now carries `gateway_client_error (P9)`; both Three-Place tests enforce it against future edits
- Blockers: none

---
*Phase: 09-agent-surface-api-diagnostics*
*Completed: 2026-09-07*

## Self-Check: PASSED

- All 5 key files exist on disk (created + modified)
- Both task commits verified in git history: e6d6644, 43ba465
- Workspace suite green at execution completion (WS_EXIT:0, 51 test binaries)
