---
phase: 14-transports-mcp-lsp
plan: 06
subsystem: transports
tags: [mcp, json-rpc, stdio, lsp, nvim, claude-code, protocol-negotiation, confirmation-gate, envelope]

# Dependency graph
requires:
  - phase: 14-transports-mcp-lsp/02
    provides: MCP contract wall (byte-scan harness, envelope-verbatim pins, mcp-SDK oracle) — the machine-evidence base for the live smoke
  - phase: 14-transports-mcp-lsp/05
    provides: ignition-nvim headless end-to-end evidence (ATTACHED_COUNT=2, per-capability assertions) — the machine-evidence base for the nvim visual check
provides:
  - Real-client verification of SC-1's "an AI agent" clause: Claude Code completed initialize → tools/list → tools/call against `ign mcp serve` live
  - SC-2 closed in production conditions: confirmation_required refusal arrived as a tool result, zero gateway requests, MCP-native JSON (after two user-driven fix loops: b3b6208, 096a068)
  - protocolVersion 2025-06-18 live-negotiation evidence (manual handshake capture + SDK oracle reruns) — closes the roadmap's implementation-time live-smoke flag
  - Honest evidence ledger for all five Phase-14 success criteria (verified / machine-evidenced / waived, labeled per item)
affects: [v1.1 verification (gsd-verify-work 14), ignition-nvim merge sequencing, future transport changes (cargo-install refresh rule)]

# Tech tracking
tech-stack:
  added: [] # no new dependencies — verification and prose changes only
  patterns:
    - "Transport-aware envelope prose: one locked JSON envelope struct, per-transport message/hint fields (CLI keeps --yes prose; MCP speaks MCP-native confirm:true)"
    - "Evidence ledger discipline: human-verify checkpoint closes per-item as VERIFIED (user-observed), MACHINE-EVIDENCE (committed automated proof), or WAIVED (user decision) — never silently merged"

key-files:
  created:
    - .planning/phases/14-transports-mcp-lsp/14-06-SUMMARY.md
  modified:
    - crates/ignition-cli/src/mcp.rs (fix loops b3b6208 + 096a068: MCP-native refusal prose, confirm optional in guarded schemas, single self-sufficient text block)
    - crates/ignition-cli/tests/contract_mcp.rs (transport-awareness pins both ways; schema-required sabotage discipline)
    - README.md (second dated envelope-transport exception beside 09-03's api-call note)

key-decisions:
  - "Stale ~/.cargo/bin/ign caused the round-0 CONNECTION_CLOSED — cargo install refresh is the required step after ANY transport-affecting change (PATH resolves the INSTALLED binary, not the repo build)"
  - "MCP confirmation_required refusal prose is transport-aware (dated envelope-transport exception #2 in README) — CLI envelope byte-frozen, MCP rides the same struct with MCP-native message/hint, one self-sufficient text block"
  - "confirm is OPTIONAL in MCP tool schemas by design — schema-required would invite agents to auto-fill it, defeating the human-in-the-loop gate"
  - "protocolVersion closed on machine evidence (handshake capture + oracle), nvim visual closed on user waiver — each ledger item labeled honestly, nothing silently merged"
  - "ignition-nvim patch stays on unmerged branch claude/ign-lsp-live-client (commit 0d6bd55) — merging that repo's main is the user's sequencing"

patterns-established:
  - "Live-smoke refresh rule: after any change to a spawned-protocol surface, reinstall via cargo install before client testing — PATH launches the installed binary"
  - "Per-item evidence labeling at human checkpoints: verified / machine-evidenced / waived, with the exact waiver recorded"

# Metrics
duration: 2h 50m (incl. 3 user live-smoke rounds + 2 fix loops; closure step ~10 min)
completed: 2026-09-16
---

# Phase 14 Plan 06: Phase-Final Human Verification Summary

**Real-client MCP live smoke closed Phase 14: Claude Code completed initialize → tools/list → tools/call over `ign mcp serve`, the confirm refusal arrived as an MCP-native tool result after two fix loops (b3b6208, 096a068), and protocolVersion 2025-06-18 was closed on machine evidence — with every checkpoint item honestly labeled verified / machine-evidenced / waived.**

## Performance

- **Duration:** 2h 50m (checkpoint open ~11:44Z → closure ~14:35Z; includes 3 user live-smoke rounds and 2 fix loops)
- **Started:** 2026-09-16T11:44:21Z (immediately after 14-05 docs close)
- **Completed:** 2026-09-16T14:35:00Z
- **Tasks:** 1 (checkpoint:human-verify, RESOLVED)
- **Files modified:** 5 code/doc files (fix loops) + 3 planning docs (closure)

## Accomplishments
- SC-1 closed with a REAL AI agent client: the user's live Claude Code session connected to `ign mcp serve`, enumerated all 83 tools (catalog spot-checks present: status, tags_browse, project_delete), and completed a full tools/call round trip
- SC-2 closed in production conditions: the destructive probe WITHOUT confirm returned the confirmation_required envelope as a tool result with zero gateway requests — and after user feedback, the refusal JSON itself now speaks MCP (message "«verb» is destructive; rerun with confirm: true to confirm", hint spelling `{"confirm": true}` and marking --yes/IGNITION_YES CLI-only), riding the same byte-stable envelope struct in ONE self-sufficient text block
- protocolVersion 2025-06-18 verified against the installed binary (manual handshake capture echoed 2025-06-18; Python mcp-SDK oracle re-ran initialize → ORACLE OK during the round-2 loop; contract_mcp pins echo-if-equal negotiation) — the roadmap's implementation-time live-smoke flag closed
- Evidence ledger recorded honestly: two items VERIFIED by user observation, two closed on machine evidence / waiver — nothing silently merged
- Root-caused the round-0 CONNECTION_CLOSED to a stale pre-Phase-14 `~/.cargo/bin/ign` (`unrecognized subcommand 'mcp'` → exit) — NOT a server defect; server was vindicated by the fresh binary's clean handshake

## Task Commits

Each unit of work was committed atomically:

1. **Task 1: Phase-final human verification — MCP live smoke + nvim composition** — `b3b6208` (feat: fix loop 1 — envelope verbatim + confirm:true guidance block), `096a068` (feat: fix loop 2 — MCP-native refusal prose + optional confirm in schema), plus the closure docs commit (docs(14-06): record checkpoint evidence ledger, close plan)

_Note: the two feat commits are the checkpoint's own "fix-and-retest" mandate executing — user-reported issues fixed and re-verified within Task 1._

## Verification Evidence Ledger

**1. Real-client initialize → tools/list → tools/call — VERIFIED (user-observed).**
User's live Claude Code session (real MCP client): server connected, 83 tools enumerated with catalog spot-checks present, read-only status tool called (gateway was down — the honest network-error envelope came back and the agent parsed profile/URL/failure class from it: envelope passthrough validated live), destructive probe attempted without confirm.

**2. Confirm refusal as tool result with MCP-native JSON — VERIFIED (user-confirmed).**
Round 3, user verbatim: "The hint problem is fixed. The JSON message and hint now tell MCP callers to pass confirm: true, and the extra line after the JSON is gone." Zero gateway requests on the refusal. Commits 096a068 + b3b6208; transport-awareness proven both ways in contract_mcp.rs (direct CLI still prints --yes prose; MCP JSON carries confirm:true prose; shape-twins modulo the two prose fields).

**3. protocolVersion 2025-06-18 live — PASS on MACHINE EVIDENCE; manual log inspection WAIVED by user ("idk how to check").**
Machine evidence: orchestrator's manual handshake capture against the installed binary echoed protocolVersion "2025-06-18"; the Python mcp-SDK oracle (a real client implementation) completed initialize → ORACLE OK both in 14-02 and again during the round-2 loop; contract_mcp pins echo-if-equal negotiation.

**4. nvim composition — PASS on COMMITTED HEADLESS E2E EVIDENCE (14-05); visual confirmation WAIVED by user decision ("assume pass on nvim").**
Committed evidence: ATTACHED_COUNT=2; ignition_live completion+hover=true (definition/codeAction/workspaceSymbol=false by design); ignition_lsp statics intact; native merge; executable-guard negative proven. The plan's visual gate (:LspInfo, completion feel, hover stamp in a real session) was waived by the user, not observed.

## Files Created/Modified
- `crates/ignition-cli/src/mcp.rs` - transport-aware refusal prose (MCP-native message/hint), confirm removed from required arrays in guarded schemas, guidance block removed (single self-sufficient text block)
- `crates/ignition-cli/tests/contract_mcp.rs` - both-ways transport pins, schema-required sabotage discipline (re-adding required → red → revert)
- `README.md` - second dated envelope-transport exception (MCP prose) beside 09-03's api-call note
- `.planning/phases/14-transports-mcp-lsp/14-06-SUMMARY.md` - this ledger
- `.planning/STATE.md` / `.planning/ROADMAP.md` - position, decisions, phase closure

## Decisions Made
- **cargo-install refresh rule:** PATH resolution launches the INSTALLED binary, not the repo build — after any transport-affecting change, `cargo install` must run before client testing (the round-0 incident is the recorded proof)
- **Transport-aware envelope prose:** the confirmation_required envelope keeps its byte-frozen struct; message/hint fields vary per transport (exception #2, dated, in README); the MCP text block is fully self-sufficient — no trailing guidance prose after the JSON
- **confirm OPTIONAL in MCP schemas:** schema-required would invite agents to auto-fill the confirmation field, defeating the gate; omission routes to the envelope refusal (no -32602)
- **Honest closure labeling:** protocolVersion closed on machine evidence; nvim visual closed on user waiver — each recorded with its provenance rather than silently upgraded to "verified"

## Deviations from Plan

None — plan executed exactly as written. The two mid-checkpoint fix loops ran inside Task 1's own mandate ("No code changes in this task; **fix-and-retest if the user reports issues**") and are committed as Task 1's feat commits. The round-0 incident was environmental (stale installed binary), not a plan or code deviation.

## Issues Encountered

- **Round 0 — CONNECTION_CLOSED (environmental, not a server defect):** the user's first live attempt died at startup; root cause was a STALE pre-Phase-14 `~/.cargo/bin/ign` exiting on `unrecognized subcommand 'mcp'` — a stray-stdout suspect (research Pitfall 3) was ruled out. Fixed by `cargo install` refresh; the fresh binary completed a full clean handshake. Recorded as the cargo-install refresh rule.
- **Round 1 — refusal prose was CLI-only + guidance block after the JSON:** the user flagged that the confirmation_required envelope's advice told CLI callers to pass `--yes` and that a trailing guidance block followed the JSON. Fix loop 1 (b3b6208) added a confirm:true guidance block; the user rejected that approach round 2 — "the JSON itself must speak MCP." Fix loop 2 (096a068) made the envelope's message/hint MCP-native over MCP, removed the trailing block (ONE self-sufficient text block), and dropped confirm from required arrays.
- **Round 3 — confirm/required residual observation:** the user's client still showed confirm as required; root cause was a CACHED schema on the client side — FIX B in 096a068 had already made confirm optional on the wire (live-probed: single-block refusal, confirm optional).

## User Setup Required

None — no external service configuration required. (Carry-forward sequencing item, user-owned: merge ignition-nvim branch `claude/ign-lsp-live-client` (commit 0d6bd55) to that repo's main.)

## Next Phase Readiness
- **Phase 14 is fully executed (6/6 plans).** All five success criteria closed: SC-1 (real agent client), SC-2 (refusal-as-tool-result, MCP-native), SC-3/SC-4 (nvim composition, machine-evidenced + waived visual), SC-5 (binary-proven in 14-02/14-04; live clients did not abort on stray bytes)
- Test status inherited from 096a068 (no code changes in the closure step): contract_mcp 11+1-ignored green, full `cargo test -p ignition-cli` green, uv mcp-SDK oracle OK, clippy -D warnings + fmt clean
- **Ready for `/gsd-verify-work 14`** (independent verification pass)
- Carry-forward: nvim patch merge is the user's sequencing; the cargo-install refresh rule applies to any future transport-affecting change

---
*Phase: 14-transports-mcp-lsp*
*Completed: 2026-09-16*

## Self-Check: PASSED

- 14-06-SUMMARY.md exists on disk ✅
- Commits b3b6208 and 096a068 verified in git history ✅
- No code changes in this closure step — test status inherited from 096a068: contract_mcp 11 passed + 1 ignored green, full `cargo test -p ignition-cli` green, uv mcp-SDK oracle OK, clippy -D warnings + fmt clean
