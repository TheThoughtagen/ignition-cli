# Roadmap: ignition-cli

## Overview

Build `ign` — a single Rust binary + ratatui cockpit that replaces both the Ignition gateway webpage and the author's ignition-mcp server as the canonical human/agent interface to Ignition 8.3+ gateways.

**Mode:** mvp — every phase delivers an end-to-end user capability (vertical slice), not a horizontal layer.

## Milestones

- ✅ **v1.0 MVP** — Phases 1-7 (shipped 2026-08-30) — [archive](milestones/v1.0-ROADMAP.md)
- 🚧 **v1.1 Agent Surface & IDE Integration** — Phases 8-14 (started 2026-09-04)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-7) — SHIPPED 2026-08-30</summary>

- [x] Phase 1: Foundation & Agentic Contracts (4/4 plans) — completed 2026-08-21
- [x] Phase 2: Gateway Health & Inspection (5/5 plans) — completed 2026-08-22
- [x] Phase 3: Project Operations (3/3 plans) — completed 2026-08-22
- [x] Phase 4: Rig Lifecycle & Trial State (4/4 plans) — completed 2026-08-23
- [x] Phase 5: WebDev Backend & Tag Operations (8/8 plans) — completed 2026-08-26
- [x] Phase 6: TUI Cockpit (11/11 plans) — completed 2026-08-28
- [x] Phase 7: Ecosystem Interop & Advanced Ops (6/6 plans) — completed 2026-08-29

Full phase details, goals, requirements mapping, and planner decisions: [milestones/v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md)

</details>

### 🚧 v1.1 Agent Surface & IDE Integration (Phases 8-14, In Progress)

**Milestone Goal:** Deepen the agent-facing surface (raw API passthrough, curated diagnostics, MCP transport), close the remaining tag gaps (xml/csv transfer, historian binding), expand EAM writes, polish the TUI (theming, polling cadence), and make `ign` a first-class editing frontend for ignition-nvim (live LSP, edit round-trip, workspace checkout).

**Structure rationale** (from research consensus): contract/config foundations first (one config migration, contract rituals codified before the second command needs them); independent command families next (disjoint surfaces, parallelizable among themselves); composite engine work after (workspace/historian/edit share the MemberSource seam); protocol transports last (pure lenses over a stable command surface — MCP proves the pattern, LSP reuses it). Phases 9-12 are order-independent of each other; all depend only on Phase 8.

- [x] **Phase 8: 08-foundations-session-core-config-contract** — Shared execution core, one-shot config schema migration, codified contract discipline *(complete 2026-09-06)*
- [x] **Phase 9: 09-agent-surface-api-diagnostics** — Raw REST passthrough escape hatch + curated daily-check diagnostics *(complete 2026-09-07)*
- [ ] **Phase 10: 10-eam-write-operations** — Full guarded EAM agent/task write lifecycle with blast-radius preview
- [ ] **Phase 11: 11-tag-bulk-transfer-xml-csv** — Server-byte-faithful XML/CSV bulk transfer with loss warnings
- [ ] **Phase 12: 12-tui-theming-degradation** — Named UX themes + graceful terminal-capability degradation
- [ ] **Phase 13: 13-composite-engine-workspace-historian-edit** — Workspace checkout, historian binding closure (spike-first), `ign edit` round-trip
- [ ] **Phase 14: 14-transports-mcp-lsp** — MCP stdio shim (proves the protocol pattern), then LSP for ignition-nvim

## Phase Details

### Phase 8: 08-foundations-session-core-config-contract
**Goal**: Every downstream feature builds on one shared execution seam (`ignition-core::Session`), one config migration (`[ui].theme` + per-profile `poll_interval_secs` land together — one goldens migration, never two), and executable contract rituals (Three-Place slug rule, OutOfBand taxonomy, stdout-purity harness) — so nothing later mutates the frozen contract by accident.
**Depends on**: Nothing (first phase of v1.1)
**Requirements**: CORE-09, CORE-10, CORE-11, TUIX-05
**Success Criteria** (what must be TRUE):
  1. A config carrying top-level `[ui].theme` and per-profile `poll_interval_secs` loads cleanly; an invalid config degrades to defaults and the TUI still starts (a config typo never kills TUI startup)
  2. Setting per-profile `poll_interval_secs` visibly changes TUI polling cadence; sub-second values are refused by config validation with a clear error (hard minimum clamp)
  3. Contract discipline is executable, not prose: Three-Place slug rule is test-enforced (exit-code enum test + README table + prose agree), OutOfBand registry is extended for `mcp`/`lsp`/`edit` with the pinned test deliberately updated with justification, and a stdout-purity byte-scan harness over the real spawned binary fails on a single stray byte
  4. All command dispatch — CLI arms and in-process callers — resolves auth/gateway clients through `ignition-core::Session` with no second client construction anywhere in the tree
**Research/Planning flags**: None — pure refactor of source-verified code (Session extraction, config keys, worker spawn-site parameterization); HIGH confidence, no unknowns. Standard patterns; skip research-phase.
**Plans**: 8 plans (6 executed + 2 gap-closure from UAT)
Plans:
- [ ] 08-01-PLAN.md — Config schema migration: [ui].theme + per-profile poll_interval_secs, lenient degradation, poll_interval_too_small clamp slug, load_for_tui
- [ ] 08-02-PLAN.md — ignition-core::Session type (resolve / resolve_degraded / for_url) with behavior-parity tests
- [ ] 08-03-PLAN.md — CLI sweep: all 8 main.rs construction sites onto Session, duplicated choreography deleted, grep-clean proof
- [ ] 08-04-PLAN.md — TUI onto Session + config-degradation wiring (load_for_tui), TUI duplicate chain deleted
- [ ] 08-05-PLAN.md — Per-profile polling cadence (context→AppState→spawn_refresh, switch adoption) + human-verify checkpoint
- [ ] 08-06-PLAN.md — Executable contract rituals: README-table agreement test, OutOfBand reserved slugs, stdout-purity harness

### Phase 9: 09-agent-surface-api-diagnostics
**Goal**: The daily gateway check needs no hand-crafted curl: users get `ign api call` as the escape hatch for anything uncurated, plus curated one-command reads (`license status`, `redundancy status`, `gan status`, diagnostics bundle) for the morning check — live-verified on both rigs.
**Depends on**: Phase 8 (contract discipline must exist before the second command family lands)
**Requirements**: EXT-01, EXT-02
**Success Criteria** (what must be TRUE):
  1. User calls any gateway REST endpoint via `ign api call --method --path` with body/header control and receives envelope-wrapped output with gateway-verbatim `data` (the documented contract exception, written into contract docs during this phase)
  2. Unclassified 4xx from `api call` lands in the exit-2 class with the verbatim body — never an "internal error" exit-1 storm; user-supplied auth-pattern headers are refused
  3. `license status`, `redundancy status`, and `gan status` each return live gateway truth with one command
  4. Diagnostics bundle generate/download/wait works as curated commands, live-verified on BOTH rigs (8.3.3 + 8.3.6)
**Research/Planning flags**: Diagnostics slice needs per-endpoint wire-shape verification against both live rigs — 8.3.x point-release variance is the documented failure mode; use version-tolerant parsing (deny_unknown_fields OFF, optional fields explicit). EXT-01: catch-all classifier is the FIRST task before the happy path; live gate designed read-only so passthrough can't nuke the rig. No research-phase for EXT-01 itself (83-api collection is ground truth).
**Plans**: 8 plans (6 executed + 2 gap-closure from UAT)
Plans:
- [x] 09-01-PLAN.md — GatewayClientError exit-2 slug (Three-Place rule in one task) + api-call-scoped classify arm + exit-partition contract tests
- [x] 09-02-PLAN.md — Live captures on BOTH rigs (8.3.3 + 8.3.6): bundle state vocabulary, license nesting, redundancy/gan units → 09-LIVE-CAPTURES.md (before any model task)
- [x] 09-03-PLAN.md — `ign api call`: raw passthrough core (RawValue verbatim data, auth-header refusal, path/query contract) + CLI + OutOfBand TUI row + binary contract tests + README contract exception
- [x] 09-04-PLAN.md — Curated reads: license/redundancy/gan status — capture-backed version-tolerant models + actions + CLI + Dashboard TUI rows + contract tests
- [x] 09-05-PLAN.md — Diagnostics bundle generate/status/wait/download: capture-encoded state vocabulary, poll-based wait, streaming download with 300s timeout override + CLI + TUI rows + contract tests
- [x] 09-06-PLAN.md — Live gates (e2e_api_diagnostics.rs): env-gated read-only matrix + mutations-gated bundle round-trip, run on BOTH rigs with recorded evidence
- [x] 09-07-PLAN.md — Gap closure (UAT test 8): bundle wait Invalid = terminal steady state, immediate exit-6 bundle_not_available, honest deadline message
- [x] 09-08-PLAN.md — Gap closure (UAT test 10): Dashboard actions menu for the seven Phase 9 verbs + routes↔menu parity CI contract

### Phase 10: 10-eam-write-operations
**Goal**: Users manage the full EAM agent/task lifecycle from the CLI without the gateway webpage — every write behind a confirmation guard, with blast-radius visibility protecting the production controller.
**Depends on**: Phase 8 (guard ladder + Session core)
**Requirements**: EAMW-01, EAMW-02, EAMW-03, EAMW-04, EAMW-05, EAMW-06, EAMW-07
**Success Criteria** (what must be TRUE):
  1. User can suspend and resume an EAM agent — refused without explicit confirmation, executes with it
  2. User can cancel or force-execute a pending EAM task behind the same confirmation guard
  3. User can rename/modify EAM agent/task configuration and delete agents/tasks behind the confirmation guard
  4. Before any guarded EAM write executes, user sees a blast-radius preview naming the target agent/task and the controller impact
  5. At least one guarded write is live-verified end-to-end against the real WHK controller rig (env-gated live gate recorded during the phase, not bolted on after)
**Research/Planning flags**: Established v1.0 guard-ladder patterns extended; no research-phase needed. EAM is endpoint-sensitive — both-rig guidance applies where endpoint shapes are involved (v1.0 `debug/eam-create-422.md` is required reading for wire-shape honesty). Gate-first, not gate-last.
**Plans:** 5 plans
Plans:
- [x] 10-01-PLAN.md — Live wire captures on both rigs (controller-mode provisioning + 12-probe list) → 10-LIVE-CAPTURES.md *(complete 2026-09-09)*
- [x] 10-02-PLAN.md — Client surface: runtime verb paths + trait methods + wiremock REQUEST pins (capture-locked) *(complete 2026-09-09)*
- [x] 10-03-PLAN.md — Action layer: suspend/resume/cancel/modify/delete + blast-radius preview composer + authoritative re-checks *(complete 2026-09-09)*
- [x] 10-04-PLAN.md — CLI + two-tier guard dispatch + force preview composition + TUI routes/parity + README reconciliation *(complete 2026-09-10, recovered from executor outage)*
- [ ] 10-05-PLAN.md — Env-gated WHK controller live gate: scratch-task lifecycle, recorded in-phase (SC-5)

### Phase 11: 11-tag-bulk-transfer-xml-csv
**Goal**: Tags move in the formats the ecosystem already speaks — byte-faithful passthrough of what the gateway produces (never CLI-side re-serialization) — with honest warnings about what lossy formats would drop before import commits.
**Depends on**: Phase 8; must land before Phase 14 (possible route-bundle bump should precede the MCP catalog freeze)
**Requirements**: TAGS-10, TAGS-11, TAGS-12
**Success Criteria** (what must be TRUE):
  1. User can bulk-download and bulk-upload tags in XML; gateway bytes pass through verbatim — round-trip proven byte-faithful on a real multi-level UDT export from a live gateway, not just wiremock fixtures
  2. User can bulk-download and bulk-upload tags in CSV with server-byte-faithful passthrough and documented lossy-field behavior
  3. Before importing XML/CSV that would drop or coerce tag fields, user sees a loss-report warning and can abort
**Research/Planning flags**: RESEARCH REQUIRED for this phase — pull a REAL multi-level UDT export and decide quick-xml serde-derive vs hand-rolled Event-loop before writing code (wiremock fixtures cannot provide this); validate the loss-report design against real exports. If routes change: one atomic WebDev bundle bump, both-direction version-drift tests.
**Plans**: TBD

### Phase 12: 12-tui-theming-degradation
**Goal**: The cockpit looks right and stays readable on any terminal — named UX themes selected by config, graceful degradation across color capabilities, with tokenization discipline making the style layer maintainable.
**Depends on**: Phase 8 (config schema provides `[ui].theme`; theme key plumbing already migrated)
**Requirements**: TUIX-03, TUIX-04
**Success Criteria** (what must be TRUE):
  1. User selects a named UX theme (monochrome + color palettes) via top-level `[ui].theme` config; the choice applies consistently across TUI screens
  2. TUI remains readable as terminal capabilities step down truecolor → 256 → 16 → mono (verified in capability-limited terminals — no broken layout, no unreadable contrast)
  3. All `Color::` literals live in the style-tokens module — CI grep enforces tokenization-first discipline; palettes compose from tokens, never scattered literals
**Research/Planning flags**: Standard patterns (k9s/btop-convergent named palette slots ~15-25 keys; ratatui theme crates rejected by stack research). No research-phase needed.
**Plans**: TBD

### Phase 13: 13-composite-engine-workspace-historian-edit
**Goal**: The generalized MemberSource diff engine turns local directories into a first-class authoring surface (workspace checkout), the historian gap closes honestly (spike-first: Designer-diff oracle or documented-limitation-with-evidence), and `ign edit` delivers the kubectl-edit loop — hardening decode/encode at workspace scale before edit rides the same codec leg.
**Depends on**: Phase 8 (Session core); benefits from Phase 11 codec/format work landing first
**Requirements**: IDE-04, TAGS-13, TAGS-14, IDE-01
**Success Criteria** (what must be TRUE):
  1. User checks out a project's resources into a local directory tree (`ign workspace checkout`), sees drift against the gateway (`status`), and pushes back guarded by `--yes`
  2. Workspace path mapping is injective and hostile-name-safe (property tests pass); `--decode-scripts` checkout produces nvim-editable script files that encode back cleanly; tag values never appear in the workspace tree
  3. User sees tag↔historian data-flow bindings in `tags config` output on a licensed gateway
  4. Historian binding resolved honestly per the spike outcome: either binding create/update works via the tag write path on a licensed rig, OR the phase ships "documented limitation, now with Designer-diff evidence" — both are legitimate done states; spike outcome recorded either way
  5. User edits a gateway resource via `ign edit`: fetch → decode → `$EDITOR` → encode → push, with unchanged saves detected (content-hash no-op), invalid encodes refused (fail-closed validation), and stale pushes blocked by a staleness check
**Research/Planning flags**: MANDATORY SPIKE for the historian slice — confirm licensed-Historian rig access BEFORE phase planning begins; plan 01 is the time-boxed Designer-diff (re-read 05-06 artifacts; both-rig diff). Roadmap deliberately holds the done-definition loose pending the spike. Workspace and edit slices: adversarial-$EDITOR and bijection/manifest pitfalls get plan-level verifications; no research-phase beyond the spike.
**Plans**: TBD

### Phase 14: 14-transports-mcp-lsp
**Goal**: AI agents and ignition-nvim drive the now-stable command surface over protocols: `ign mcp serve` proves the protocol-mode pattern once (hand-rolled JSON-RPC 2.0 stdio, stdout purity, clap-derived catalog), and `ign lsp` reuses it verbatim to feed live gateway truth to nvim — completing the ignition-mcp replacement.
**Depends on**: Phases 9, 10, 11, 13 (transports are lenses — the command surface they mirror must be stable first; MCP catalog derives from the clap tree and structurally cannot drift)
**Requirements**: EXT-04, IDE-02, IDE-03
**Success Criteria** (what must be TRUE):
  1. An AI agent completes initialize → tools/list → tools/call over `ign mcp serve` (stdio, hand-rolled JSON-RPC 2.0); the tool catalog derives from `Cli::command()` with a CI parity test — no hand-written catalog
  2. Write tools require an explicit `confirm` tool-call field (the `--yes` translation) — omitting it returns the refusal envelope as the tool result; the frozen JSON envelope is returned verbatim as tool-result content
  3. ignition-nvim receives completions (tag-path, named-query, provider), hover, and diagnostics through `ign lsp` — served from TTL-cached gateway truth with no blocking network call inside an LSP request; Python ignition-lsp retains statics ownership (composition, never replacement)
  4. `ign lsp` is registered in ignition-nvim's detection order (one-line sibling-repo patch) and verified end-to-end in that repo
  5. Both protocol modes own stdout completely — byte-scan tests over the real spawned binaries fail on any stray byte; `ping` never starves behind a gateway call
**Research/Planning flags**: LSP slice needs verification of the sync dispatch loop + in-process tokio `block_on` pattern under the existing tracing setup, plus scripted-client harness design. MCP slice: transport decision settled by stack research (hand-rolled, not rmcp); protocolVersion pin "2025-06-18" is an implementation-time live smoke test against Claude Code/Claude Desktop, not research-phase.
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 8 → 9 → 10 → 11 → 12 → 13 → 14
Phases 9-12 are order-independent of each other (all depend only on Phase 8). Phases 13 and 14 must follow as planned.

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation & Agentic Contracts | v1.0 | 4/4 | Complete | 2026-08-21 |
| 2. Gateway Health & Inspection | v1.0 | 5/5 | Complete | 2026-08-22 |
| 3. Project Operations | v1.0 | 3/3 | Complete | 2026-08-22 |
| 4. Rig Lifecycle & Trial State | v1.0 | 4/4 | Complete | 2026-08-23 |
| 5. WebDev Backend & Tag Operations | v1.0 | 8/8 | Complete | 2026-08-26 |
| 6. TUI Cockpit | v1.0 | 11/11 | Complete | 2026-08-28 |
| 7. Ecosystem Interop & Advanced Ops | v1.0 | 6/6 | Complete | 2026-08-29 |
| 8. 08-foundations-session-core-config-contract | v1.1 | 6/6 | Complete | 2026-09-06 |
| 9. 09-agent-surface-api-diagnostics | v1.1 | 8/8 | Complete | 2026-09-07 |
| 10. 10-eam-write-operations | v1.1 | 4/5 | In progress | 2026-09-09 |
| 11. 11-tag-bulk-transfer-xml-csv | v1.1 | 0/TBD | Not started | - |
| 12. 12-tui-theming-degradation | v1.1 | 0/TBD | Not started | - |
| 13. 13-composite-engine-workspace-historian-edit | v1.1 | 0/TBD | Not started | - |
| 14. 14-transports-mcp-lsp | v1.1 | 0/TBD | Not started | - |

---
*Roadmap created: 2026-09-04 — milestone v1.1 (25 v1.1 requirements mapped across 7 phases)*
