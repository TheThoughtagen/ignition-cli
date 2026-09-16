---
phase: 14-transports-mcp-lsp
plan: 02
subsystem: testing
tags: [mcp, json-rpc, contract-tests, wiremock, stdio, byte-scan, python-mcp-sdk, uv, conformance-oracle]

# Dependency graph
requires:
  - phase: 14-transports-mcp-lsp
    provides: "`ign mcp serve` (14-01): JSON-RPC 2.0 stdio server, clap-derived catalog, GUARDED_OPS confirm gate, off-loop tools/call + inline ping"
  - phase: 08-session-seam
    provides: the stdout-purity harness recipe (tempdir config + ambient IGNITION_* strip + IGNITION_LOG=trace noise) the byte-scan reuses
provides:
  - "ScriptedClient byte-scan harness over the REAL spawned binary (every stdout line must parse as one JSON-RPC message — the stray-byte wall), recv_timeout budgets so protocol death fails loudly"
  - "Full 2025-06-18 lifecycle contract tests: initialize echo + older-version negotiation (numeric AND string ids), silent notifications/initialized, prompt ping, tools/list catalog pins"
  - "SC-1 round-trip smoke: every wire-guarded verb + 5 read verbs rebuild the bridge argv and must parse against the real clap tree"
  - "SC-2 binary-level proof: envelope-verbatim pin (tools/call == direct CLI stdout byte-for-byte), confirm-refusal envelope AS tool result (IGNITION_YES env-proof), confirm:true executes on the wire (DELETE + confirm=true recorded)"
  - "Error-mapping + survival contract: -32601/-32602, malformed stdin quiet-then-keeps-serving"
  - "SC-5 (MCP) permanent starvation pin: strict first-arrival ordering — ping provably answered before a ≥30s-hung dead-gateway tool result"
  - "tests/mcp_oracle.py — real Python mcp-SDK conformance oracle (uv test-time only) + #[ignore] gate that green-skips without uv"
affects: [14-03 (LSP contract suite reuses the scripted-binary harness shape), 14-04+ (transport slices), phase verification (SC-1/SC-2/SC-5-MCP evidence)]

# Tech tracking
tech-stack:
  added: [] # zero new Cargo dependencies — wiremock/assert_cmd/tempfile/serde_json already dev-deps; mcp SDK is uv-fetched test-time only
  patterns:
    - "Byte-scan readline harness: reader thread MUST serde_json::parse every stdout line before forwarding; a panic in the thread fails the test via channel disconnect"
    - "Strict first-arrival ordering (recv_next, no id-filtering) for out-of-order-response proofs — id-filtered waiting would silently swallow the violation"
    - "Dead-gateway as bound-but-never-accepting std TcpListener: TCP connect succeeds into the backlog, so the client's own 30s timeout is the only escape — a deterministic ≥30s hang"
    - "Fixture-stable envelope equality: wiremock bodies pin every value, making MCP-tool-result == direct-CLI-stdout byte equality deterministic"

key-files:
  created:
    - crates/ignition-cli/tests/contract_mcp.rs
    - crates/ignition-cli/tests/mcp_oracle.py
  modified: []

key-decisions:
  - "The starvation test uses a bound-but-never-accepting TcpListener instead of a dead port (127.0.0.1:1) — a refused connection errors INSTANTLY (no hang to order against), while a backlog-accepted connection hangs until the client's own 30s timeout, making the ping-first ordering deterministic and the test self-contained"
  - "Ordering is asserted with an unfiltered recv_next (FIRST message after the sends must be the ping), not id-filtered expect_response — an id-filtered wait would skip a wrongly-early tool result and silently pass the very bug it pins"
  - "The round-trip smoke walks the real clap tree test-side (builder's leaf rule) to recover positional order + flag shapes the wire schema cannot express, then rebuilds the bridge's exact argv mapping (confirm → --yes) — catalog names cannot drift from parseable argv"
  - "The oracle gate lives as a #[ignore] tokio test (cargo test … mcp_oracle -- --ignored) rather than a doc-commented script step — it reuses the in-test wiremock fixtures and CARGO_BIN_EXE_ign path, green-skips when uv is absent, and keeps Python forever outside Cargo's dependency graph"
  - "Python SDK attribute spellings are read defensively (input_schema/inputSchema, is_error/isError) — the oracle must survive mcp-SDK version drift since uv pulls the latest at run time"

# Metrics
duration: 45min
completed: 2026-09-16
---

# Phase 14 Plan 02: MCP Contract Test Suite Summary

**Scripted-client byte-scan wall over the real `ign mcp serve`: full 2025-06-18 lifecycle, envelope-verbatim tools/call, env-proof confirm gate, permanent ping-starvation pin, and a real Python mcp-SDK conformance oracle — zero new Cargo dependencies**

## Performance

- **Duration:** 45 min
- **Started:** 2026-09-16T02:27:47Z
- **Completed:** 2026-09-16T03:13:18Z
- **Tasks:** 2
- **Files modified:** 2 (both new)

## Accomplishments
- THE byte-scan proven both ways: a scripted client drives the real spawned binary under `IGNITION_LOG=trace` + unknown-key config noise with every stdout line forced through `serde_json` parse; a planted `println!("noise")` in serve went RED and the revert went green
- SC-1 closed at binary level: initialize echo/negotiation (2025-06-18, both id shapes), silent `notifications/initialized` (quiet-window proof), prompt ping, tools/list catalog pins (project_delete confirm-required, status confirm-free, mcp/lsp/tui/completions/edit absent), and a 28-verb round-trip smoke proving catalog names parse as clap argv
- SC-2 closed at binary level: MCP tool result is BYTE-FOR-BYTE the direct CLI envelope; omitted confirm returns the frozen `confirmation_required` envelope AS the isError tool result even with `IGNITION_YES=1` in the spawned env; `confirm:true` is the only `--yes` path and the wiremock recorded the DELETE carrying `confirm=true`
- SC-5 (MCP) pinned permanently: with a tools/call hung ≥30s on a dead gateway, the ping response provably arrived FIRST (strict first-arrival assertion), then the tool result still landed — starvation is structurally impossible and the test fails loudly if that ever changes
- A real mcp-SDK Python client (uv, test-time only) completed initialize → tools/list → tools/call against the wiremock-backed profile and printed `ORACLE OK`; the gate green-skips when uv is absent

## Task Commits

Each task was committed atomically:

1. **Task 1: ScriptedClient harness + protocol lifecycle contract tests** - `7903bb3` (test)
2. **Task 2: tools/call contract + Python mcp SDK oracle** - `e336146` (test)

## Files Created/Modified
- `crates/ignition-cli/tests/contract_mcp.rs` (NEW, 1123 lines) — ScriptedClient byte-scan harness (isolated noisy config, ambient-IGNITION_* strip, recv_timeout budgets, Drop reaping) + 11 protocol/tools-call contract tests + the oracle's #[ignore] gate
- `crates/ignition-cli/tests/mcp_oracle.py` (NEW, 111 lines) — stdlib + mcp-SDK client oracle: clean-env spawn, initialize, tools/list shape checks, tools/call envelope ok:true, `ORACLE OK` exit-0 contract

## Decisions Made
- Dead gateway = bound-but-never-accepting `TcpListener` (see key-decisions) — a refused port would error instantly and make the starvation ordering meaningless
- Strict first-arrival ordering via unfiltered `recv_next` (see key-decisions)
- Round-trip smoke rebuilds argv from a test-side clap walk (see key-decisions)
- Oracle gate as an `#[ignore]` test reusing the wiremock fixtures + `CARGO_BIN_EXE_ign` (see key-decisions)
- Defensive SDK attribute access in the oracle (see key-decisions)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Oracle argv handling — `--` separator reached the script as sys.argv[1]**
- **Found during:** Task 2 (oracle gate run)
- **Issue:** the plan's canonical invocation `uv run --with mcp python tests/mcp_oracle.py -- <path-to-ign>` forwards `--` verbatim into the script's argv (uv does not consume it), so the script executed `--` as the server binary (FileNotFoundError)
- **Fix:** the script filters a literal `--` out of argv before reading the binary path; invocation form unchanged
- **Files modified:** crates/ignition-cli/tests/mcp_oracle.py
- **Verification:** gate green — ORACLE OK under uv 0.6.9
- **Committed in:** e336146 (Task 2 commit)

**2. [Rule 1 - Bug] mcp-SDK attribute spellings (input_schema / is_error)**
- **Found during:** Task 2 (first oracle run)
- **Issue:** the installed mcp SDK exposes snake_case pydantic fields (`input_schema`, `is_error`); the oracle's camelCase assertions crashed with AttributeError/failed asserts inside the SDK's task group
- **Fix:** defensive getattr across both spellings (schema/is_error), verified against the live SDK's `model_fields`
- **Files modified:** crates/ignition-cli/tests/mcp_oracle.py
- **Verification:** gate green after the change
- **Committed in:** e336146 (Task 2 commit)

**3. [Rule 1 - Bug] wiremock API drift — received_requests() returns Vec, not Option**
- **Found during:** Task 2 (first compile of the confirm:true test)
- **Issue:** `.expect(...)` on `guard.received_requests()` — this wiremock version returns the Vec directly
- **Fix:** dropped the needless `.expect`
- **Files modified:** crates/ignition-cli/tests/contract_mcp.rs
- **Verification:** compile + green test
- **Committed in:** e336146 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 × Rule 1 bug — API-surface drift, all in the new test code)
**Impact on plan:** All three were mechanical compatibility fixes to test code; no production files touched, no scope creep.

## Issues Encountered
- The parallel 14-03 agent owns the shared workspace tree and its in-flight `src/lsp.rs` twice blocked the shared bin build (`unresolved import TextDocumentSyncType`, then a missing `GatewayApi` import) — both resolved by 14-03 itself within minutes; this plan retried its builds and never touched any 14-03-owned file
- The plan's sabotage verification required a TEMPORARY edit to `src/mcp.rs` (not in this plan's files list): planted, proven red, and reverted within one verification step — `git diff` confirmed zero residue before either commit

## Anti-Sabotage Evidence
- Byte-scan: `println!("noise")` planted in mcp.rs ping arm → `ping_answers_promptly` RED ("the reader thread died — … the byte-scan rejected a stray byte") → reverted → full suite green (Task 1 verification)

## User Setup Required

None - no external service configuration required. (The oracle needs `uv` only to run explicitly; without it the gate green-skips and the suite stays green.)

## Next Phase Readiness
- The scripted-binary harness (byte-scan readline, isolated noisy config, recv_timeout budgets, strict-ordering recv_next, dead-gateway listener) is the reusable shape 14-03's LSP contract suite planned to mirror — swap readline for Content-Length framing
- SC-1/SC-2/SC-5(MCP) now have permanent binary-level regression walls; phase verification can cite `cargo test -p ignition-cli --test contract_mcp` (11 green + 1 ignored oracle gate) and the recorded oracle run
- Known catalog gap re-recorded for the future: `workspace push`'s core-side guard (13-06) is not advertised as `confirm` in the catalog (planner-locked GUARDED_OPS scope) — the verb still refuses safely through core as an isError result; the round-trip smoke covers it as a read-shaped guarded-on-wire verb only if it ever gains the synthetic confirm

---
*Phase: 14-transports-mcp-lsp*
*Completed: 2026-09-16*

## Self-Check: PASSED

- Files: contract_mcp.rs (1123 lines — artifact min_lines 200 exceeded), mcp_oracle.py (111 lines), SUMMARY.md — all exist on disk
- Commits: 7903bb3 (Task 1) and e336146 (Task 2) both present in git log
- Sabotage residue: `git diff` on src/mcp.rs is empty — the temporary anti-sabotage edit left zero residue
- Verification suite at close: `cargo test -p ignition-cli --test contract_mcp` 11 passed / 1 ignored (oracle gate green-skips by design); explicit `-- --ignored` oracle run green (ORACLE OK under uv 0.6.9); full `cargo test -p ignition-cli` 34/34 test targets 0 failed; `cargo clippy -p ignition-cli --test contract_mcp -- -D warnings` clean; `cargo fmt` applied to the plan's files
