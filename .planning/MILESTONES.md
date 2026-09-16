# Milestones

## v1.0 MVP (Shipped: 2026-08-30)

**Phases:** 1-7 · **Plans:** 41 · **Tasks:** 118 · **Timeline:** 9 days (2026-08-20 → 2026-08-29)
**Code:** ~73,900 lines Rust + ~1,022 lines Python (WebDev routes) · **Tests:** 863 green, 26 opt-in live gates
**Git range:** d5476f7 (docs: initialize project) → fe9e074 (test(07): UAT round 2 re-verify)

**Delivered:** `ign` — a single Rust binary + ratatui cockpit that fully operates and inspects an Ignition 8.3+ gateway (health, projects, tags, rigs, backups, EAM, script exec) without opening the gateway webpage or Designer — replacing ignition-mcp as the canonical human/agent interface, with a frozen agentic JSON contract on every subcommand.

**Key accomplishments:**
- Agentic foundation — multi-profile auth (env/keyring secret chain), frozen JSON envelope `{ok,profile,data}`, stable exit-code taxonomy 0-7, snapbox golden CI, `--yes` destructive guards
- Gateway inspection — the webpage's health pages replaced: status/modules/metrics/sessions/connections, `logs -f` NDJSON streaming, logger levels, `doctor` diagnostics, `wait`/`restart` poll primitives
- Project operations — full CRUD, streaming ZIP export/import with collision policies, and surgical resource editing via export→zip-member-surgery→import (live-rig round-trip proven)
- Self-managed Docker rig — up/down/status/reset with 5-level compose discovery, port pre-flight, commissioned-wait; native OIDC trial reset (live-proven on 8.3.3 + 8.3.6); snapshot/restore
- CLI-owned WebDev backend + full tag lifecycle — versioned routes (1.1.0) deployed to the gateway; providers, browse, read/write, configs, UDTs, alarms, history, bulk transfer — **the ignition-mcp replacement bar, all five live e2e gates green on a real 8.3.3 rig**
- TUI cockpit — ratatui dashboard/logs/tag-watch/alarms/projects/rig screens with CI-enforced CLI↔TUI parity (clap-tree coverage walk proves every CLI verb reachable)
- Ecosystem interop — cross-gateway `project diff`/`sync`, gwbk backup download/restore, guarded EAM tasks (live-verified against the WHK controller), opt-in script exec, Flint codec round-trip editing, `ign lint` delegation, offline `--from-export` tag browsing

**Archives:** [v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md) · [v1.0-REQUIREMENTS.md](milestones/v1.0-REQUIREMENTS.md) (all 44 v1 requirements shipped)

---
## v1.1 Agent Surface & IDE Integration (Shipped: 2026-09-16)

**Phases:** 8-14 · **Plans:** 47 · **Tasks:** 113 · **Timeline:** 12 days (2026-09-04 → 2026-09-16)
**Code:** ~107,954 lines Rust (up from ~73,900 at v1.0) · **Tests:** ~1,255 Rust test fns
**Git range:** v1.0 tag → fdf7777 (docs(phase-14): complete phase execution) · 236 commits, 263 files changed (+76,357/−2,351)

**Delivered:** `ign` v1.1 — the agent surface and IDE integration: raw API passthrough + curated diagnostics, full guarded EAM writes with blast-radius preview, byte-faithful XML/CSV tag transfer with loss gates, historian binding closed, workspace checkout/push + `ign edit` round-trip, and MCP/LSP transports that make `ign` drivable by AI agents (live-proven by a real Claude Code client) and by nvim — all 25 v1.1 requirements shipped across 7 phases.

**Key accomplishments:**
- Agent surface — `ign api call` raw REST passthrough (gateway-verbatim `data`, catch-all 4xx→exit-2, auth-header refusal) + curated daily checks (`license status`, `redundancy status`, `gan status`, diagnostics bundle generate/status/wait/download), live-verified on both 8.3.3 and 8.3.6 rigs
- Guarded EAM writes — full suspend/resume/cancel/force/modify/delete lifecycle behind a blast-radius preview gate (CLI refusal prose IS the preview; TUI Confirm modals); capture-first wire truth on both rigs, through-suspend live-proven on a disposable WHK controller rig
- Tag gaps closed — byte-faithful XML transfer (gateway bytes verbatim, sha256 round-trip fidelity oracle both rigs), CLI-generated CSV with documented losses, loss-report advisory gate before imports (route bundle atomically bumped 1.1.0→1.3.0); historian binding spike verdict CLOSURE — `[historyEnabled, historyProvider, sampleMode]` live-proven on both trial rigs, written values proven in history
- IDE integration — `ign workspace checkout/status/push` over the generalized MemberSource engine (injective hostile-name-safe path mapping, proptest-proven; manifest three-way compare; guarded push) and `ign edit` fetch→decode→$EDITOR→encode→push with content-hash no-op detection, fail-closed encode, staleness gate; `ign lsp` feeds ignition-nvim completions/hover/diagnostics from TTL-cached gateway truth (headless e2e'd; nvim patch on branch claude/ign-lsp-live-client pending user merge)
- MCP transport — `ign mcp serve` (hand-rolled JSON-RPC 2.0 stdio, 83-tool clap-derived catalog with CI parity, confirm-gate refusal as tool result) proven live by a real Claude Code client: initialize → tools/list → tools/call, honest evidence ledger in 14-06-SUMMARY.md
- TUI polish — four named themes × four capability tiers with authored degradation (WCAG-AAA dark body contrast; default/mono wire-proven byte-identical), CI tokenization gate, per-profile polling cadence with sub-second clamp; user-approved after 3-round UAT
- Foundation refactor — `ignition-core::Session` single execution seam (zero second client construction, grep-proven), one-migration config schema with lenient degradation, executable contract rituals (Three-Place slug rule CI, OutOfBand taxonomy, stdout-purity byte-scan over the real binary)

**Archives:** [v1.1-ROADMAP.md](milestones/v1.1-ROADMAP.md) · [v1.1-REQUIREMENTS.md](milestones/v1.1-REQUIREMENTS.md) (all 25 v1.1 requirements shipped)

---

