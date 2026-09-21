---
phase: 14-transports-mcp-lsp
plan: 04
subsystem: transports
tags: [lsp, lsp-server, lsp-types, crossbeam-select, completion, hover, diagnostics, ttl-cache, byte-scan, wiremock, dead-gateway]

# Dependency graph
requires:
  - phase: 14-transports-mcp-lsp (14-03)
    provides: the `ign lsp` loop scaffold (narrow capabilities, sync dispatch), GatewayCache TTL snapshot + soft-degrading refresher, and the kick + snapshot-version channels 14-04 consumes
  - phase: 14-transports-mcp-lsp (14-02)
    provides: the contract-harness recipe (CARGO_BIN_EXE spawn + purity noise + assert-based parsing) and the wiremock fixture style
provides:
  - "Real LSP handlers: textDocument/completion merging THREE families from the cached snapshot (providers=Module, tag paths=File scoped by closed-bracket provider context, named-query paths=Function)"
  - "TTL-stamped hover (Pitfall 4): '<word> — <family> — gateway snapshot Ns old (TTL 30s)' — staleness is never silent"
  - "publishDiagnostics derived ONLY from the frame: unknown provider warns, absent tag path hints (age-stamped), named-query miss hints; unhealthy frame publishes EMPTY (dead-gateway honesty)"
  - "The loop select!s over connection.receiver + the refresher version channel — diagnostics republish after EVERY refresh cycle; refresher death swaps in crossbeam never() instead of spinning"
  - "Open-docs map (didOpen/didChange FULL/didClose) as the sole doc-text source; didSave kicks the refresher best-effort"
  - "tests/contract_lsp.rs: Content-Length scripted client byte-scan over the REAL binary + lifecycle/data-plane/dead-gateway contract suite (SC-5 LSP half + SC-3 behavioral half)"
affects: [14-05 (next), 14-06, IDE-03/ignition-nvim (the verified server surface the client patch attaches to)]

# Tech tracking
tech-stack:
  added: [crossbeam-channel 0.5 (workspace+crate; the select! loop requires it directly — supersedes 14-03's "never a direct dep" comment)]
  patterns:
    - "Pure handler layer: every handler is a total fn over (&Snapshot, text, position) — the request path composes ONE cache.snapshot() Arc clone with pure fns, unit-testable with hand-built frames"
    - "Diagnostics honesty ladder: known-but-unpopulated tree → silent; unhealthy frame → empty publish; verdicts only from cached truth"
    - "crossbeam select! loop with never() fallback so a dead refresher channel cannot busy-spin the loop"
    - "Wiremock-fed cache contract: mount the exact populate path (lists + route version handshake + recursive browse + export zip) and poll a completion until non-empty as the first-publish wait"

key-files:
  created:
    - crates/ignition-cli/tests/contract_lsp.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/ignition-cli/Cargo.toml
    - crates/ignition-cli/src/lsp.rs

key-decisions:
  - "The docs map keys on the URI STRING, not lsp_types::Uri — clippy mutable_key_type fires on Uri's interior-mutable innards, and lsp-types itself compares Uris by string; publish_for_doc re-parses the key only at the publish boundary"
  - "Provider context for completion is a CLOSED-bracket heuristic: `[default]Motors/` scopes tag paths to default; an unterminated `[def` flows ALL providers' paths (simple and honest over clever partial matching — plan's own escape hatch)"
  - "Hover matches a bracketed word via its bare inner name (`[default]` → provider default) while paths keep raw form — the word extractor spans the bracket/path alphabet because qualified tag paths ARE the snapshot keys"
  - "Unknown-provider diagnostics WARN; absent tag paths and named-query misses HINT (hint-grade per plan); every verdict message carries the snapshot age stamp (Pitfall 4)"
  - "Dead-gateway diagnostics honesty: an unhealthy snapshot publishes EMPTY — no cached oracle means no verdict, never a wall of false unknown-provider spam (pinned in unit tests AND the contract suite)"
  - "Per-request latency pins in the dead-gateway test measure send→answer per REQUEST, not whole-test wall clock — process startup is not a request and cannot block on the network"

patterns-established:
  - "First-publish wait as completion polling: re-send a completion until non-empty (bounded 60s) — exercises the real request path as the readiness probe"
  - "Byte-scan sabotage is stdout-only: a println! in a handler reds the framing reader; eprintln/tracing (stderr) correctly do not — the purity property is stdout-scoped"

# Metrics
duration: 55 min
completed: 2026-09-16
---

# Phase 14 Plan 04: LSP Handlers + Contract Suite Summary

**Real LSP handlers over the TTL cache — three-family completions, TTL-stamped hover, frame-derived diagnostics on edits and refreshes via a crossbeam select! loop — proven at the binary level by a Content-Length scripted client whose byte-scan, narrow-capability pin, and dead-gateway cache-only proof close SC-3's behavioral half and SC-5's LSP half**

## Performance

- **Duration:** 55 min
- **Started:** 2026-09-16T03:31:57Z
- **Completed:** 2026-09-16T04:27:32Z
- **Tasks:** 2
- **Files modified:** 4 (1 new test file, lsp.rs 802→1757 lines)

## Accomplishments
- All three completion families flow from ONE snapshot read: provider names (Module), tag paths scoped to the closed-bracket provider context (File), named-query paths for known projects (Function); an unhealthy snapshot answers empty, never an error
- Hover stamps staleness into every payload (`[default]Motor — tag path — gateway snapshot 12s old (TTL 30s)`) and spans providers/tag paths/named queries; misses answer null
- Diagnostics publish from the cache after didOpen/didChange AND after every refresh cycle — the loop's `crossbeam_channel::select!` over the message receiver + the refresher's version channel is the loop's only structural change; handlers stayed pure; refresher death swaps in `never()` instead of busy-spinning
- The contract suite drives the REAL binary: byte-scan framing reader (any stray stdout byte reds), narrow-capability pin asserting definition/codeAction/workspace-symbol ABSENCE, wiremock-fed three-family data plane with TTL-stamped hover and clearing publishes, and the permanent dead-gateway proof (requests answer within budget with honest empty results)
- Anti-sabotage proven both directions: removing a completion family reds unit tests; a `println!` in a handler reds the byte-scan; both restored green

## Task Commits

Each task was committed atomically:

1. **Task 1: Real handlers — completions (3 families), TTL-stamped hover, cached diagnostics, select! loop** - `7447bff` (feat)
2. **Task 2: contract_lsp.rs — byte-scan client, lifecycle/data-plane/dead-gateway suite** - `c037ea9` (test)

## Files Created/Modified
- `crates/ignition-cli/src/lsp.rs` — filled handler bodies + the pure handler layer (word_at/completions/hover/scanners/diagnostics_for), select! loop, docs map, crossbeam channels, named-query populate normalization; 802→1757 lines (15 new unit tests, 25 total)
- `crates/ignition-cli/tests/contract_lsp.rs` (NEW, 762 lines) — LspClient harness + byte-scan reader + the three contract suites
- `Cargo.toml` / `crates/ignition-cli/Cargo.toml` — crossbeam-channel promoted to a direct workspace dep (select! requires it; comment updated to supersede 14-03's note)
- `Cargo.lock` — crossbeam-channel locked

## Decisions Made
- URI-string-keyed docs map (clippy mutable_key_type; lsp-types compares Uris by string anyway)
- Closed-bracket provider-context heuristic; unterminated brackets flow all providers (plan's "simple and honest" clause)
- Hint-grade severities for tag-path/NQ misses with age stamps in every verdict message
- Dead-gateway diagnostics publish empty (no truth → no verdicts), pinned at both unit and contract level
- Per-request latency measurement in the dead-gateway proof (startup is not a request)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Named-query populate filter never fired on real export shapes**
- **Found during:** Task 1 (fixture design for the wiremock export zip)
- **Issue:** 14-03's populate filtered `resource_members` output on `starts_with("named-query/")`, but `user_path` maps real export members (`{collection}/resources/named-query/{path}/resource.json`) to `{collection}/named-query/{path}/resource.json` — the prefix never matched, so completion family (c) and the NQ diagnostic class would have been permanently dead against a real gateway
- **Fix:** `named_query_path()` normalizes any path containing the `named-query/` segment to the query path (stripping the `/resource.json` leaf); unit test pins both the real export shape and folder forms
- **Files modified:** crates/ignition-cli/src/lsp.rs
- **Verification:** `named_query_normalization_handles_the_real_export_shape` green; the wiremock contract test feeds a real-shaped member through the full populate path and the NQ family surfaces in completions
- **Committed in:** 7447bff

**2. [Rule 3 - Blocking] Channels switched std::mpsc → crossbeam; crossbeam-channel promoted to a direct dep**
- **Found during:** Task 1 (loop restructure)
- **Issue:** the plan's mandated `crossbeam_channel::select!` cannot compile against std::sync::mpsc receivers, and the crate was a transitive dep only (14-03's Cargo comment even said "never a direct dep")
- **Fix:** kicks + version channels became `crossbeam_channel::unbounded`; `crossbeam-channel = "0.5"` added to the workspace + crate (same 0.5 line lsp-server uses, so Receiver types unify); the 14-03 comment updated with the supersession rationale
- **Files modified:** Cargo.toml, Cargo.lock, crates/ignition-cli/Cargo.toml, crates/ignition-cli/src/lsp.rs
- **Verification:** select! loop compiles against lsp-server 0.10's crossbeam receiver; full suite green
- **Committed in:** 7447bff

**3. [Rule 1 - Bug] Contract-test doc referenced a tag path absent from its own fixture**
- **Found during:** Task 2 (data-plane test (b))
- **Issue:** the didChange fixture used `[default]Motor/Speed`, but the mocked tree carries `[default]Motor` and `[default]Motors/Speed` — the publish correctly flagged it as absent, so the "exactly one diagnostic" assert reded (the handler was RIGHT)
- **Fix:** fixture text corrected to `[default]Motors/Speed`; the publish now carries exactly the unknown-provider warning
- **Files modified:** crates/ignition-cli/tests/contract_lsp.rs
- **Verification:** data-plane test green; the absent-path hint behavior got free extra coverage from the original failure
- **Committed in:** c037ea9

---

**Total deviations:** 3 auto-fixed (1 bug-fix in prior-plan code, 1 blocking dep/channel change, 1 test-fixture bug)
**Impact on plan:** All three were required for the plan's own must_haves to hold (family (c) must fire on real exports; the select! loop is the plan's own design). No scope creep.

## Issues Encountered
- The first full-suite parallel run showed a 15.002s whole-interaction elapsed in the dead-gateway test (per-step budgets never tripped; single-run measured 2.34s). Root cause: the initial whole-test latency assert measured process startup + scheduling under three concurrent spawns, not request latency. Fixed by measuring per-request (send→answer), which is the actual SC-3 property; per-recv budgets still fail loudly on true blocking.
- The byte-scan sabotage note in the plan ("tracing-to-stdout or eprintln") needed correcting in practice: both trace and eprintln land on stderr by design, so the honest sabotage is a `println!` — the purity property is stdout-scoped, and the sabotage run proved exactly that boundary.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- The `ign lsp` server surface is COMPLETE and contract-pinned; 14-05 can wire the ignition-nvim client against a binary whose capabilities, completions, hover, and diagnostics behavior are all machine-proven (the nvim patch's `cmd = { 'ign', 'lsp' }` contract is exactly what the suite drives)
- SC-3 is closed behaviorally: the dead-gateway proof reruns in CI forever; any handler that ever touches Session/runtime on the request path fails by timeout
- SC-5's LSP half is closed: the byte-scan survives IGNITION_LOG=trace + unknown-key config noise and was sabotage-proven; the MCP half closed in 14-02
- Remaining for the phase: 14-05/14-06 (per ROADMAP) — no blockers carried forward

---
*Phase: 14-transports-mcp-lsp*
*Completed: 2026-09-16*

## Self-Check: PASSED

- Files: all 6 key-files exist on disk (contract_lsp.rs NEW at 762 lines — artifact min_lines 200 exceeded; lsp.rs at 1757 lines)
- Commits: 7447bff (Task 1) and c037ea9 (Task 2) both present in git log
- must_have greps: `publish_diagnostics` in lsp.rs ✓; `cache.snapshot()` handler link ✓; `crossbeam_channel::select!` loop ✓; `Content-Length` framing in contract_lsp.rs ✓
- Verification suite at close: `cargo test -p ignition-cli` — 35 test targets, all ok (25 lsp unit tests incl. the three-family/kind/anti-sabotage pins; contract_lsp 3/3 incl. the dead-gateway proof); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean
- Sabotage evidence: completion-family removal reded 3 unit tests (restored); handler println! reded the byte-scan framing reader (restored)
