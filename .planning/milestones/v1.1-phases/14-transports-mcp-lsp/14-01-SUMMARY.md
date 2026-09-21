---
phase: 14-transports-mcp-lsp
plan: 01
subsystem: transports
tags: [mcp, json-rpc, stdio, clap-derive, tokio, serde-json, protocol-server, out-of-band]

# Dependency graph
requires:
  - phase: 08-session-seam
    provides: Session::resolve (the one auth/config seam — CORE-09), the mcp/lsp OutOfBand slug reservation (08-06)
  - phase: 13-composite-engine-workspace-historian-edit
    provides: the `edit` OutOfBand seam precedent (main.rs) and the atomic row+command landing pattern (13-08)
provides:
  - "`ign mcp serve` — hand-rolled JSON-RPC 2.0 stdio MCP server (2025-06-18 negotiation pin)"
  - "clap-derived tool catalog (build_catalog in mcp.rs) with bidirectional CI parity test — structurally cannot drift"
  - "GUARDED_OPS const registry (24 guarded verbs) + drift test pinning registry ↔ dispatch sites both directions"
  - "confirm-gate translation: --yes reachable ONLY via the tools/call confirm field; IGNITION_YES structurally unreachable (live-proven)"
  - "frozen envelope passthrough: tool results byte-equal to the chassis' CompactJson renders (unit-pinned)"
  - "concurrent serve loop: tools/call spawned off-loop through mpsc; ping answered inline (SC-5 live-proven)"
  - "OutOfBand row for `mcp` + pinned set [api call, completions, edit, mcp] landed atomically with the clap command"
affects: [14-02 (contract test suite consumes this server), 14-03+ (LSP slice reuses the pattern), ignition-nvim (future MCP client)]

# Tech tracking
tech-stack:
  added: [] # zero new dependencies — serde_json + tokio already in the tree
  patterns:
    - "OutOfBand protocol verb: hidden clap command + own seam after the ONE mode decision + routes.rs row in the same commit"
    - "clap-tree-as-catalog: tool names/descriptions/schemas derived from Cli::command() reflection; parity test against an independent walk"
    - "const registry + marker comments + source-scanning drift test (the 12-03 grep-CI genre applied to a const table)"
    - "single-writer mpsc task owns stdout; out-of-order JSON-RPC responses make non-starvation structural"

key-files:
  created:
    - crates/ignition-cli/src/mcp.rs
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-tui/src/routes.rs
    - crates/ignition-cli/tests/tui_coverage.rs

key-decisions:
  - "`serve` is a restricted positional (value_parser=[\"serve\"]), NOT a subcommand — a subcommand leaf `mcp serve` would yield a SECOND row-requiring node in the tui_coverage walk, forcing a second routes() row and extending the pinned OutOfBand set past its four-member contract; the plan's pinned tests (row path 'mcp', set exactly 4, walk bidirectional-green) are the binding truths and this shape satisfies all of them"
  - "Global clap args (profile/json/compact/yes/verbose) are excluded from every tool schema — their protocol-path effect is forced (json/compact), overridden (profile rides the ambient flag), confirm-gated (yes → synthetic confirm property), or stderr-only (verbose); exclusion also makes the hostile-yes-smuggling probe refuse -32602"
  - "Two raw-stdout streaming verbs refuse at the bridge (rig_logs always; logs --follow): their dispatch arms print through in-dispatch println sinks, and a stray stdout byte is a protocol death (Pitfall 3) — refusal messages point at the terminal form"
  - "GUARDED_OPS carries 24 verbs — the guards living in main.rs dispatch arms per the plan's enumerated site list; workspace push's guard lives in ignition-core (13-06) and is OUT of the registry by planner-locked scope (it still refuses safely via core, riding as an isError result, but the catalog does not advertise confirm for it — follow-up candidate for 14-02's contract work)"
  - "Bridge awaits crate::dispatch directly inside spawned tasks (the research sketch's block_on-per-task shape would panic — cannot start a runtime from within a runtime); serve therefore takes no runtime handle"

patterns-established:
  - "Protocol-mode pattern (for 14-03 LSP): hidden clap command → own main-level seam → OutOfBand row + pinned-set extension in the SAME commit"
  - "Catalog derivation pattern: walk leaf rule identical to tui_coverage.rs + hidden/exclusion filters; parity test walks independently and pins bidirectional set equality"
  - "Drift-test pattern: static literals byte-scanned from include_str! source; dynamic sites carry // guarded:<leaf> markers; marker set == registry paths both directions"
  - "Purity-refusal pattern: verbs whose product is raw stdout refuse at the bridge with -32602 rather than corrupting the protocol stream"

# Metrics
duration: 61 min
completed: 2026-09-16
---

# Phase 14 Plan 01: MCP Transport Summary

**`ign mcp serve` — hand-rolled JSON-RPC 2.0 stdio server with a clap-derived 86-tool catalog (zero hand-written entries), confirm-gated `--yes` translation over a 24-verb GUARDED_OPS registry, and frozen envelope passthrough — zero new dependencies**

## Performance

- **Duration:** 61 min
- **Started:** 2026-09-16T01:18:38Z
- **Completed:** 2026-09-16T02:20:06Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- `ign mcp serve` speaks newline-delimited JSON-RPC 2.0 end to end: initialize with the 2025-06-18 negotiation pin (echo-if-supported), silent notifications/initialized, inline ping, clap-derived tools/list (86 tools, no pagination), and off-loop tools/call returning the frozen envelope verbatim with correct isError
- The tool catalog derives 100% of names/descriptions/schemas from `Cli::command()` — the parity test proves bidirectional set equality against an independent walk, so catalog drift is structurally impossible
- GUARDED_OPS (24 guarded verbs) is the single confirm source; the drift test scans main.rs source (static literals byte-matched, `// guarded:` markers pinned both directions) and was anti-sabotage-verified red-on-removal
- SC-2 proven live over the real binary: omitted confirm → the frozen `confirmation_required` refusal envelope as an isError tool result; `confirm:true` passes the gate; `IGNITION_YES=1` in the environment CANNOT confirm a write
- SC-5 proven live: with a tools/call hung against a dead gateway, ping answered first (out-of-order, id-matched) — non-starvation is structural
- OutOfBand atomicity held: `mcp` row + hidden command + pinned set exactly [api call, completions, edit, mcp] landed in one commit; the bidirectional walk stayed green

## Task Commits

Each task was committed atomically:

1. **Task 1: Clap command + OutOfBand row + GUARDED_OPS registry + catalog builder with parity test** - `d3bddf9` (feat)
2. **Task 2: JSON-RPC 2.0 framing + concurrent serve loop + dispatch bridge (confirm gate, envelope passthrough)** - `868d05e` (feat)

## Files Created/Modified
- `crates/ignition-cli/src/mcp.rs` (NEW, 573 lines) — catalog builder (clap walk, hidden/exclusion filters, arg-reflection schemas, synthetic confirm), JSON-RPC framing, concurrent serve loop, dispatch bridge, 13 unit tests
- `crates/ignition-cli/src/cli.rs` — hidden `Commands::Mcp(McpArgs)` with the restricted `serve` positional
- `crates/ignition-cli/src/main.rs` — `mod mcp`, the Mcp seam beside Edit's, `pub(crate) GUARDED_OPS`, `// guarded:` markers at all 24 dispatch sites, dispatch exhaustiveness arm, the source-scanning drift test
- `crates/ignition-tui/src/routes.rs` — OutOfBand row for `mcp` with 08-06-genre justification; reservation comments updated (mcp fulfilled, lsp still reserved)
- `crates/ignition-cli/tests/tui_coverage.rs` — pinned OutOfBand set extended to exactly four members; doc comments mark the 08-06 mcp reservation FULFILLED

## Decisions Made
- `serve` as a restricted positional instead of a subcommand (see key-decisions) — the plan's prose sketch and its machine-checked pins were mutually exclusive; the pins won
- Globals excluded from tool schemas with fail-closed unknown-key validation at the bridge (defense-in-depth for SC-2)
- Bridge-level refusal for the two raw-stdout streaming verbs (see key-decisions)
- GUARDED_OPS scoped to main.rs dispatch arms per the plan's planner-locked site list; the workspace-push core-side guard documented as a known catalog gap

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `serve` implemented as a restricted positional, not a subcommand**
- **Found during:** Task 1 (step 1-3 reconciliation)
- **Issue:** the plan's sketch (`McpArgs { Serve }` subcommand) makes the tui_coverage walk emit `mcp serve` as the row-requiring leaf — the routes row `mcp` would be an ORPHAN and the pinned set could not be exactly [api call, completions, edit, mcp]; the plan's own must_haves were mutually inconsistent under the subcommand shape
- **Fix:** `serve: String` with `value_parser = ["serve"]` — `ign mcp serve` parses, `ign mcp`/`ign mcp bogus` are clap usage errors (exit 2, pre-protocol), and the walk yields exactly one row-requiring node (`mcp`)
- **Files modified:** crates/ignition-cli/src/cli.rs
- **Verification:** tui_coverage 5/5 green including the extended pin and the bidirectional walk
- **Committed in:** d3bddf9 (Task 1 commit)

**2. [Rule 1 - Bug] Research sketch's block_on-per-task execution replaced with direct await**
- **Found during:** Task 2 (serve loop implementation)
- **Issue:** the research Pattern 3 sketch calls `runtime_handle.block_on(dispatch(...))` inside the spawned tools/call task — `block_on` inside a runtime worker panics ("cannot start a runtime from within a runtime")
- **Fix:** the spawned task awaits `crate::dispatch` directly (already inside main's `block_on` context); `serve` takes no runtime handle
- **Files modified:** crates/ignition-cli/src/mcp.rs
- **Verification:** live hung-gateway test — ping answered while the call was in flight; no panic
- **Committed in:** 868d05e (Task 2 commit)

**3. [Rule 2 - Missing Critical] Bridge refuses raw-stdout streaming verbs**
- **Found during:** Task 2 (dispatch bridge)
- **Issue:** `rig logs` (compose passthrough in EVERY mode) and `logs --follow` (NDJSON) print raw lines through `println!` sinks INSIDE dispatch — called over MCP they would interleave non-message bytes into the protocol stream (Pitfall 3: "a stray stdout byte kills the MCP client", spec MUST)
- **Fix:** execute() refuses both (-32602, message pointing at the terminal form) before any dispatch work; the schema/catalog is untouched so the pinned exclusion set and parity stay intact
- **Files modified:** crates/ignition-cli/src/mcp.rs
- **Verification:** execute_refuses_before_dispatch unit test; tui_coverage/routes pins untouched
- **Committed in:** 868d05e (Task 2 commit)

**4. [Rule 2 - Missing Critical] Fail-closed unknown-argument validation at the bridge**
- **Found during:** Task 2 (dispatch bridge)
- **Issue:** global args are excluded from tool schemas, but nothing stopped a hostile client from sending `{"yes": true}` in arguments — the 1:1 property→token map would have emitted `--yes`, bypassing the confirm gate (SC-2 verbatim violation)
- **Fix:** execute() validates every argument key against the leaf's known args (+`confirm` on guarded leaves) and refuses unknown keys with -32602; unit test pins the hostile-yes probe
- **Files modified:** crates/ignition-cli/src/mcp.rs
- **Verification:** execute_refuses_before_dispatch — the `yes` probe refuses -32602; live IGNITION_YES env test also red
- **Committed in:** 868d05e (Task 2 commit)

---

**Total deviations:** 4 auto-fixed (1 blocking, 1 bug, 2 missing-critical)
**Impact on plan:** All four necessary for correctness/safety (protocol purity + the SC-2 gate); no scope creep. The plan's machine-checked must_haves are all satisfied.

## Issues Encountered
- `target/debug/ign` in the repo is a stale Sep-10 artifact — the global `~/.cargo/config.toml` redirects builds to `/Users/pmannion/Library/Caches/cargo-target`; smoke tests ran against the fresh cache binary. (Environment note for 14-02's contract suites: use `Command::cargo_bin("ign")`/env.cargo_bin, never the repo-relative path.)
- The first writer-task anti-sabotage attempt didn't compile (unused receiver), so the stale binary answered — redone compilably (`drop(resp_rx)` + no-op spawn): 0 responses sabotaged, 1 restored.

## Anti-Sabotage Evidence
- GUARDED_OPS drift test: removed ("restart", "restart") → red; restored → green (d3bddf9 verification)
- Catalog parity: bogus `starts_with("status")` filter in the builder → red; removed → green (d3bddf9 verification)
- Writer task: spawn disabled (compilable form) → 0 responses over the real binary; restored → 1 (868d05e verification)
- SC-5 live: hung-gateway tools/call + ping → response order [ping, tool result]

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- The Phase-14 protocol-mode pattern is proven end-to-end and reusable by the LSP slice: hidden command → main seam → OutOfBand row atomicity, Session-fed in-process dispatch, scripted-binary verification shape
- 14-02's contract suite consumes this server: scripted MCP client over the spawned binary (byte-scan per line), tools/list parity, confirm-gate refusals, ping-during-in-flight ordering, optional Python `mcp` SDK conformance oracle
- Known catalog gap for 14-02 to consider: `workspace push`'s guard lives in ignition-core (13-06), so GUARDED_OPS (main.rs-scoped, planner-locked) does not advertise `confirm` for it — the verb still refuses safely through core and rides as an isError result
- `lsp` OutOfBand row remains reserved (08-06) for its own plan — rows without commands fail the walk by design

---
*Phase: 14-transports-mcp-lsp*
*Completed: 2026-09-16*

## Self-Check: PASSED

- Files: all 5 key-files exist on disk (mcp.rs NEW at 956 lines — artifact min_lines 250 exceeded)
- Commits: d3bddf9 (Task 1) and 868d05e (Task 2) both present in git log
- must_have greps: `Mcp(McpArgs)` hidden variant at cli.rs:224 (the plan's `rg 'Commands::Mcp' cli.rs` check was authoring-imprecise — the declaration is unqualified inside the enum; the qualified form lives at the main.rs/mcp.rs usage sites, which match); `"mcp"` row in routes.rs ✓; GUARDED_OPS in main.rs ✓; mcp pin in tui_coverage.rs ✓; `try_parse_from` bridge in mcp.rs ✓
- Verification suite at close: `cargo test -p ignition-cli` 33/33 test targets green (13 bin unit tests incl. parity/drift/envelope pins; tui_coverage 5/5 with the four-member pin); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean; zero Cargo.toml changes
