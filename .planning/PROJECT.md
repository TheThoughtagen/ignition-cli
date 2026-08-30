# ignition-cli

## What This Is

A Rust CLI and ratatui TUI cockpit for Ignition by Inductive Automation gateways — a unified, agentic-friendly developer tool that replaces the gateway webpage and minimizes Designer usage. It wraps gateway REST + WebDev APIs for health checks, project operations, and tag operations, plus Docker test-rig lifecycle control, cross-gateway diff/sync, gwbk backups, EAM tasks, and opt-in script execution. The TUI is the full cockpit for humans; every command is also scriptable with JSON output for AI agents. Round-trip bridges to the ecosystem (nvim editing via the Flint codec, `ign lint` delegation, offline git-module export browsing) close the loop with the author's other tooling.

## Core Value

One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

## Requirements

### Validated

All shipped and live-verified in v1.0 (2026-08-30) — 44 atomic requirements, 7 phases, 41 plans:

- ✓ Gateway health operations (status, info, modules, logs, restart, sessions, connections, metrics, doctor, wait) — v1.0
- ✓ Project operations (CRUD, import/export, surgical resource edit, cross-gateway diff/sync) — v1.0
- ✓ Tag operations (providers, browse, read/write, configs, UDTs, alarms, history, bulk JSON transfer) via own WebDev backend — v1.0
- ✓ Test-rig lifecycle (docker up/down/status/reset, trial resets, snapshot/restore) — v1.0
- ✓ JSON output mode on all subcommands (frozen envelope + exit-code taxonomy) — v1.0
- ✓ Ratatui TUI cockpit exposing all CLI capabilities (CI-enforced parity) — v1.0
- ✓ Gateway profile/config management (multiple gateways, env/keyring secret chain) — v1.0
- ✓ Backups/EAM (gwbk download/restore, guarded EAM tasks) and opt-in script exec — v1.0
- ✓ Ecosystem interop (decode/encode scripts, lint delegation, offline export browsing) — v1.0

### Active

Candidates for the next milestone — actual scope defined by `/gsd-new-milestone`:

- [ ] Tag-provider bulk transfer in xml/csv (deferred from TAGS-09; JSON shipped as the native interchange)
- [ ] Tag↔historian data-flow binding (05-06 documented limitation; Designer-diff follow-up is the resolution path)
- [ ] TUI theming (monochrome/color UX themes — backlog-owned per 06-UAT triage)
- [ ] EXT-01: `ign api call` raw passthrough escape hatch
- [ ] EXT-02: license status, diagnostics bundle, redundancy, GAN status as curated commands
- [ ] EXT-03: EAM write operations beyond guarded basics
- [ ] EXT-04: MCP-server transport mode (thin shim over the stable JSON contract)
- [ ] TUIX-01: configurable polling cadence per profile

### Out of Scope

- Ignition 8.1.x support — git-module v2 policy precedent; 8.3+ only keeps API surface tight (held through v1.0; zero 8.1 pressure)
- Designer-side integration — that's ignition-git-module's job (the CLI bridges to its exports, doesn't replace it)
- LSP/editing features — ignition-nvim / ignition-lsp already cover this (the CLI's decode/encode-scripts codec bridges to them)
- Linting engine — ignition-lint owns it; the CLI delegates only (`ign lint` shipped in v1.0)
- MCP transport — CLI + JSON output is the agent interface for now; MCP-style serving stays a later decision (EXT-04 candidate above)

## Context

**Shipped v1.0 on 2026-08-30** — 7 phases, 41 plans, 118 tasks over 9 days. ~73,900 lines of Rust (3 crates) + ~1,022 lines of Python WebDev routes embedded in the binary; 863 tests green (26 opt-in live gates); all five Phase-5 live e2e gates green on a real 8.3.3 rig; UAT rounds passed for phases 5-7 including gap-closure re-verification.

- Ecosystem this completes (all by the same author / WhiskeyHouse):
  - `ignition-mcp` (~/whiskeyhouse/ignition-mcp) — Python FastMCP server, 37 tools over native REST + WebDev. **v1.0 replaces it** as the canonical gateway interface; its tool catalog was the reference for API coverage.
  - `ignition-git-module` (~/whiskeyhouse/ignition-git-module) — Ignition module embedding Git in the Designer; its `docker/` directory was the pattern source for rig lifecycle commands; its tag exports are browsable offline via `tags browse --from-export`.
  - `ignition-nvim` (~/whiskeyhouse/ignition-nvim) — LSP/completions for editing Ignition resources outside the Designer; `--decode-scripts`/`--encode-scripts` round-trips resources into its editing model.
  - `ignition-lint` (~/whiskeyhouse/ignition-lint) — linting toolkit; `ign lint` delegates to it on PATH.
  - `83-api` (~/whiskeyhouse/83-api) — Bruno/Postman collections of the Ignition 8.3 REST API; reference for endpoint coverage.
  - `WHK-Global` (~/data/projects/WHK-Global) — parent Ignition project; rig conventions reused, EAM task family live-verified against its controller.
  - `ignition-trial-resetter` (~/whiskeyhouse/ignition-trial-resetter) — superseded: native OIDC trial reset shipped in the CLI.
- Tag writes, alarms, and script execution ride the CLI's **own versioned WebDev routes** (bundle 1.1.0, stale-deploy refusal live-proven) rather than depending on WHK-Global's deployment.
- Ignition 8.3 uses the REST + WebDev architecture; auth via gateway user with appropriate roles. Live-verified against 8.3.1-era semantics on 8.3.3 and 8.3.6 rigs.
- Primary user is the author (developer + their AI agents); secondary: Whiskey House E&T team.
- Known technical debt / honest limitations: tag↔historian data-flow binding unresolved (documented in 05-06); route sources self-contained with deliberate duplication (~25-line core per route); no per-resource REST exists on real 8.3 gateways — resource editing rides zip-member surgery.

## Constraints

- **Tech stack**: Rust + ratatui for TUI; clap for CLI — keep the dependency tree lean (held: zero heavy deps added beyond plan)
- **Compatibility**: Ignition 8.3.1+ only (matches ignition-git-module v2 support policy)
- **Agentic usage**: every subcommand must be non-interactive by default with `--json` output; TUI is opt-in via subcommand/flag (frozen contract — changing envelope/exit codes is a breaking change for agents)
- **Simplicity**: "simple but complete" — avoid framework creep; single binary, no daemon required
- **Pairing**: config conventions should interoperate with WHK-Global and git-module rigs (compose file discovery, env/secrets)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| CLI replaces ignition-mcp (not complementary) | One canonical interface; agents drive CLIs well with JSON output; Rust beats Python for a daily-driver tool | ✓ Good — v1.0 shipped with the full tag lifecycle + all five live e2e gates green (the replacement bar) |
| Ship own WebDev routes | Independence from WHK-Global deployment; versioned with the CLI | ✓ Good — version-locked 1.1.0; stale-1.0.0 deploys refuse `route_version_mismatch` (live-proven both directions) |
| 8.3+ only | API variance in 8.1 not worth carrying; ecosystem precedent | ✓ Good — zero 8.1 pressure across 9 days of live work on 8.3.3/8.3.6 rigs |
| ratatui full cockpit (every CLI action available in TUI) | TUI as primary human interface, not a viewer | ✓ Good — parity is CI-enforced, not aspirational (tui_coverage clap-tree bidirectional walk) |
| Frozen agentic output contract (envelope `{ok,profile,data}`, exit taxonomy 0-7, stderr diagnostics) | Agents key on stable shapes; the contract is the product | ✓ Good — golden-enforced, additive-only slug growth across 7 phases |
| Resource editing via export→zip-member-surgery→import | Per-resource REST routes don't exist on real 8.3 gateways (triple-verified) | ✓ Good — course-corrected from the roadmap assumption; live round-trip proven |
| Native OIDC trial reset over Playwright delegation | Playwright needs Node+chromium and broke across the 8.3.3 UI rewrite | ✓ Good — live-proven on both 8.3.3 (expired→active) and 8.3.6 |
| scriptExec structural opt-in (deploy-time secret, fail-closed route) | Security posture: public template can never arm the gate | ✓ Good — redaction proven at action and binary level |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-30 after v1.0 milestone*