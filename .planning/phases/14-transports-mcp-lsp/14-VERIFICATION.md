---
phase: 14-transports-mcp-lsp
verified: 2026-09-16T08:55:00Z
status: passed
score: 5/5 must-haves verified
gaps: []
notes:
  - "ignition-nvim patch lives on unmerged branch claude/ign-lsp-live-client (0d6bd55); merge sequencing is user-owned per 14-06 disposition"
  - "nvim visual confirmation and protocolVersion manual-log inspection were waived by the user at the 14-06 checkpoint (closed on machine evidence / user waiver, honestly labeled in 14-06-SUMMARY.md)"
  - "contract_mcp's Python SDK oracle is #[ignore]-gated and green-skipped in this run (acceptable per plan contract; it ran OK twice during the phase per 14-02/14-06 summaries)"
---

# Phase 14: 14-transports-mcp-lsp Verification Report

**Phase Goal:** AI agents and ignition-nvim drive the now-stable command surface over protocols: `ign mcp serve` proves the protocol-mode pattern once (hand-rolled JSON-RPC 2.0 stdio, stdout purity, clap-derived catalog), and `ign lsp` reuses it verbatim to feed live gateway truth to nvim — completing the ignition-mcp replacement.
**Verified:** 2026-09-16T08:55:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | An AI agent completes initialize → tools/list → tools/call over `ign mcp serve`; catalog derives from `Cli::command()` with a CI parity test — no hand-written catalog | ✓ VERIFIED | contract_mcp.rs 11/11 green over the real spawned binary: `initialize_echoes_the_pinned_revision_with_both_id_shapes`, `initialize_older_version_negotiates_to_the_server_revision`, `tools_list_carries_the_catalog_with_the_pinned_shape` (test-side independent clap-tree walk), `catalog_sample_round_trips_through_clap_parse`, `tools_call_returns_the_frozen_envelope_verbatim`. Catalog builder `walk_catalog(&Cli::command(), …)` at mcp.rs:116 — zero hand-written entries. Live agent clause: Claude Code completed the full lifecycle user-observed (14-06 ledger item 1, VERIFIED) |
| 2 | Write tools require explicit `confirm`; omitting it returns the refusal envelope as the tool result; frozen envelope verbatim as tool-result content (14-06 amendment: refusal PROSE is transport-aware, dated exception #2 in README) | ✓ VERIFIED | mcp.rs `confirm` gate — `--yes` reachable ONLY from the confirm field (mcp.rs:565-581); hostile `yes` property probe refuses -32602; IGNITION_YES env-proof pinned. `confirm_omitted_refuses_with_mcp_native_prose_and_cli_prose_survives_directly` GREEN: proves transport-awareness BOTH ways (direct CLI keeps frozen `--yes` prose; MCP JSON carries `confirm: true` prose; shape-twins differing in exactly message/hint). `mcp_confirm_refusal_envelope` (mcp.rs ~648) rewrites only the two prose fields on the SAME locked struct. README "The MCP refusal-prose exception (2026-09-16)" present (lines 53-65) |
| 3 | ignition-nvim receives completions (3 families), hover, diagnostics via `ign lsp` from TTL-cached truth — no blocking network inside an LSP request; Python statics ownership retained | ✓ VERIFIED | contract_lsp.rs 3/3 green: `data_plane_completions_diagnostics_and_hover_from_a_wiremock_fed_cache` (real binary, wiremock-fed cache, age-stamp helper), `dead_gateway_requests_all_answer_budget_from_cache_only` (dead port, per-request latency pins, empty-not-guessed diagnostics). Structurally: `session` exists ONLY in GatewaySource (refresher thread, own runtime, block_on legal there); handlers are pure `cache.snapshot()` reads; crossbeam `select!` loop (lsp.rs:108). Narrow caps unit-pinned (lsp.rs:1090-1103: definition/codeAction/workspaceSymbol asserted `is_none()`); statics composition proven from client side by headless e2e (statics all-5-caps true, ignition_live caps false-by-design) |
| 4 | `ign lsp` registered in ignition-nvim's detection order (one-line sibling patch) and verified end-to-end | ✓ VERIFIED (branch-committed, unmerged) | Sibling repo /Users/pmannion/whiskeyhouse/ignition-nvim: commit `0d6bd55` "feat(lsp): register ign lsp as a second (live-truth) client beside ignition-lsp" present on branch `claude/ign-lsp-live-client` (currently the checked-out HEAD). lsp.lua contains `vim.lsp.config('ignition_live', { cmd = { 'ign', 'lsp' } …})` behind `vim.fn.executable('ign') == 1`, registered BEFORE the statics early-return; statics (ignition_lsp) chain untouched. Headless e2e evidence in 14-05-SUMMARY: ATTACHED_COUNT=2, per-client capability assertions, guard-negative run, committed-state re-run (0 diff vs HEAD). **Note: branch is unmerged to that repo's main — merge is user-owned sequencing** |
| 5 | Both protocol modes own stdout completely — byte-scan tests over real spawned binaries fail on any stray byte; `ping` never starves | ✓ VERIFIED | MCP: ScriptedClient reader thread panics on any non-JSON line (contract_mcp.rs:113-119), every test drives the real `CARGO_BIN_EXE_ign`. LSP: `reader_thread` panics on any byte outside Content-Length framing and asserts exact-N payload reads (contract_lsp.rs:253-301). Starvation: `ping_never_starves_behind_an_in_flight_dead_gateway_call` GREEN (bound-but-never-accepting listener, unfiltered first-message ordering). Server-side concurrency: `tools/call` spawned off the reader path, single writer task (mcp.rs:267-280) |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/ignition-cli/src/mcp.rs` | JSON-RPC framing, clap-walk catalog, GUARDED_OPS reader, concurrent serve loop, dispatch bridge (≥250 lines) | ✓ VERIFIED | 1107 lines; all subsystems present and test-exercised |
| `crates/ignition-cli/src/lsp.rs` | lsp-server sync loop, narrow caps, GatewayCache + refresher, 3-family handlers (≥200 lines) | ✓ VERIFIED | 1757 lines; caps narrow by construction + unit-pinned |
| `crates/ignition-cli/src/cli.rs` | Hidden `Commands::Mcp(McpArgs)` + `Commands::Lsp` | ✓ VERIFIED | cli.rs:224, 231, 249 |
| `crates/ignition-cli/src/main.rs` | Commands::Mcp/Lsp seams before chassis + pub(crate) GUARDED_OPS + drift test | ✓ VERIFIED | Seams at 454/470 (pre-chassis, chassis arms unreachable); GUARDED_OPS at 3066; bidirectional drift test (marker set ↔ registry paths) at 3651-3692 |
| `crates/ignition-tui/src/routes.rs` | OutOfBand rows for mcp + lsp with written justifications | ✓ VERIFIED | Rows at 107-110 (`mcp`) and 124-127 (`lsp`), each with dated justification comments |
| `crates/ignition-cli/tests/tui_coverage.rs` | Pinned OutOfBand set [api call, completions, edit, mcp, lsp] | ✓ VERIFIED | `out_of_band_rows_are_pinned` GREEN in this run |
| `crates/ignition-cli/tests/contract_mcp.rs` | ScriptedClient byte-scan harness + full contract suite (≥200 lines) | ✓ VERIFIED | 1250 lines; 11 tests green |
| `crates/ignition-cli/tests/contract_lsp.rs` | Content-Length scripted client + lifecycle/data/dead-gateway suite (≥200 lines) | ✓ VERIFIED | 762 lines; 3 tests green |
| `crates/ignition-cli/tests/mcp_oracle.py` | Real mcp-SDK client oracle, green-skip without uv | ✓ VERIFIED | 111 lines; real ClientSession lifecycle; #[ignore]-gated in contract_mcp.rs |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| mcp.rs catalog | `Cli::command()` | runtime clap walk | ✓ WIRED | walk_catalog(&Cli::command()) — catalog cannot drift; parity test walks the tree independently test-side |
| mcp.rs dispatch bridge | crate::dispatch | confirm → `--yes` argv, spawn per call | ✓ WIRED | Bridge awaits dispatch inside spawned tasks; `--yes` only from confirm field; IGNITION_YES bypass proven behaviorally |
| main.rs seams | mcp::serve / lsp::serve | pre-chassis early return | ✓ WIRED | Chassis arms `unreachable!()` — protocol modes cannot leak into the standard render path |
| lsp.rs handlers | GatewayCache::snapshot() | Arc snapshot read per request | ✓ WIRED | Zero Session/runtime on request path (grep: `session` confined to GatewaySource); dead-gateway test is the behavioral pin |
| lsp.rs main loop | refresher version channel | crossbeam select! over receiver + versions | ✓ WIRED | lsp.rs:108; diagnostics publish on each refresh cycle |
| contract tests | real spawned binaries | `CARGO_BIN_EXE_ign` / cargo_bin | ✓ WIRED | Both harnesses spawn the actual binary, byte-scan its stdout |
| ignition-nvim lsp.lua | `ign lsp` | cmd = {'ign','lsp'} + executable guard | ✓ WIRED | Verified in sibling repo at 0d6bd55 |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| ----------- | ------ | -------------- |
| EXT-04 | ✓ SATISFIED | None — all four clauses (stdio hand-rolled JSON-RPC, clap-derived catalog + parity test, confirm gate with refusal-as-tool-result, envelope verbatim) verified by green tests over the real binary; live-agent clause user-observed in 14-06 |
| IDE-02 | ✓ SATISFIED | None — three-family completions/TTL-stamped hover/diagnostics from cache proven at binary level; no-network-in-request proven behaviorally with a dead gateway; statics composition intact (narrow caps + e2e caps split) |
| IDE-03 | ✓ SATISFIED | None — one-file sibling patch registered in detection order, headless e2e verified; committed at 0d6bd55 (branch-unmerged; merge is user sequencing, not a code gap) |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| (none) | — | No TODO/FIXME/placeholder/unimplemented!/todo! in any phase file | — | — |

Scan covered mcp.rs, lsp.rs, main.rs, cli.rs, contract_mcp.rs, contract_lsp.rs. The single `unimplemented!` grep hit in cli.rs:983 is a doc comment describing the guarantee's absence of stubs.

### Test Execution (run by verifier, 2026-09-16)

```
cargo test -p ignition-cli --test contract_mcp --test contract_lsp --test tui_coverage
→ contract_lsp:   3 passed, 0 failed (2.76s)
→ contract_mcp:  11 passed, 1 ignored (30.04s)  [oracle green-skip per contract]
→ tui_coverage:   5 passed, 0 failed (0.01s)
```

Commits verified in history: `b3b6208`, `096a068` (this repo); `0d6bd55` (sibling repo, branch `claude/ign-lsp-live-client`).

### Human Verification Required

Nothing outstanding as a gate — the two human-facing items were dispositioned at the 14-06 checkpoint with the user in the loop and are recorded, not silent:

1. **Live MCP smoke with a real client** — VERIFIED user-observed (Claude Code lifecycle + refusal probe, 14-06 ledger items 1-2). No action needed.
2. **nvim visual composition** — closed on committed headless e2e evidence + explicit user waiver ("assume pass on nvim", 14-06 ledger item 4). Carry-forward, user-owned: merge `claude/ign-lsp-live-client` into ignition-nvim main when sequencing allows; a real-session `:LspInfo` glance at merge time is the natural belt-and-braces.
3. **protocolVersion manual log inspection** — waived; closed on machine evidence (handshake capture echo + SDK oracle + contract pin).

### Gaps Summary

No gaps. All five success criteria verified at three levels (exists / substantive / wired):

- **SC-1** — protocol lifecycle + clap-derived catalog: machine-proven by 11 green contract tests over the real binary, live-agent clause user-observed.
- **SC-2** — confirm gate + envelope verbatim: green pins including the IGNITION_YES bypass and the hostile-yes probe; the 14-06 transport-aware-prose amendment is honored in code (`mcp_confirm_refusal_envelope` on the same locked struct), documented as the dated README exception (2026-09-16), and pinned both directions in contract_mcp.
- **SC-3** — nvim consumption via TTL cache: binary-level data-plane + dead-gateway proofs green; narrow caps unit-pinned; composition holds at the client boundary (e2e caps split).
- **SC-4** — sibling registration + e2e: committed at 0d6bd55 with committed-state re-verification; unmerged-by-design, user sequencing.
- **SC-5** — stdout ownership + starvation: both byte-scan harnesses panic on any stray byte over the real binaries; ping-starvation structurally pinned.

---

_Verified: 2026-09-16T08:55:00Z_
_Verifier: Claude (gsd-verifier)_
