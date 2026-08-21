# ignition-cli

## What This Is

A Rust CLI and ratatui TUI cockpit for Ignition by Inductive Automation gateways — a unified, agentic-friendly developer tool that replaces the gateway webpage and minimizes Designer usage. It wraps gateway REST + WebDev APIs for health checks, project operations, and tag operations, plus Docker test-rig lifecycle control. The TUI is the full cockpit for humans; every command is also scriptable with JSON output for AI agents.

## Core Value

One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Gateway health operations (status, info, modules, logs, restart)
- [ ] Project operations (list/import/export/sync projects and resources, cross-gateway)
- [ ] Tag operations (browse, read, write, UDT definitions, alarms) via own WebDev backend
- [ ] Test-rig lifecycle (docker up/down/reset, trial resets, snapshot/restore)
- [ ] JSON output mode on all subcommands for agentic/scripted usage
- [ ] Ratatui TUI cockpit exposing all CLI capabilities
- [ ] Gateway profile/config management (multiple gateways: dev, test, prod)

### Out of Scope

- Ignition 8.1.x support — git-module v2 policy precedent; 8.3+ only keeps API surface tight
- Designer-side integration — that's ignition-git-module's job
- LSP/editing features — ignition-nvim / ignition-lsp already cover this
- Linting — ignition-lint already covers this
- Replacing ignition-mcp's MCP transport in v1 — CLI + JSON output is the agent interface; MCP-style serving is a later decision

## Context

- Ecosystem this completes (all by the same author / WhiskeyHouse):
  - `ignition-mcp` (~/whiskeyhouse/ignition-mcp) — Python FastMCP server, 37 tools over native REST + WebDev. **This CLI replaces it** as the canonical gateway interface; its tool catalog is the reference for API coverage.
  - `ignition-git-module` (~/whiskeyhouse/ignition-git-module) — Ignition module embedding Git in the Designer; its `docker/` directory (compose files, gw-build, gw-init, gw-secrets, test-rig) is the pattern source for rig lifecycle commands.
  - `ignition-nvim` (~/whiskeyhouse/ignition-nvim) — LSP/completions for editing Ignition resources outside the Designer.
  - `ignition-lint` (~/whiskeyhouse/ignition-lint) — linting toolkit for Ignition projects.
  - `83-api` (~/whiskeyhouse/83-api) — Bruno/Postman collections of the Ignition 8.3 REST API; reference for endpoint coverage.
  - `WHK-Global` (~/data/projects/WHK-Global) — parent Ignition project with Playwright e2e (`e2e/`), trial reset scripts, and WebDev module deployed; the CLI should drive these workflows and reuse rig conventions.
  - `ignition-trial-resetter` (~/whiskeyhouse/ignition-trial-resetter) — trial reset tooling to fold into rig lifecycle.
- Tag writes, alarms, and script execution require a WebDev backend on the gateway. This project ships **its own WebDev routes** (scaffolding included) rather than depending on WHK-Global's.
- Ignition 8.3 uses the REST + WebDev architecture; auth via gateway user with appropriate roles.
- Primary user is the author (developer + their AI agents); secondary: Whiskey House E&T team.

## Constraints

- **Tech stack**: Rust + ratatui for TUI; clap for CLI — keep the dependency tree lean
- **Compatibility**: Ignition 8.3.1+ only (matches ignition-git-module v2 support policy)
- **Agentic usage**: every subcommand must be non-interactive by default with `--json` output; TUI is opt-in via subcommand/flag
- **Simplicity**: "simple but complete" — avoid framework creep; single binary, no daemon required
- **Pairing**: config conventions should interoperate with WHK-Global and git-module rigs (compose file discovery, env/secrets)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| CLI replaces ignition-mcp (not complementary) | One canonical interface; agents drive CLIs well with JSON output; Rust beats Python for a daily-driver tool | — Pending |
| Ship own WebDev routes | Independence from WHK-Global deployment; versioned with the CLI | — Pending |
| 8.3+ only | API variance in 8.1 not worth carrying; ecosystem precedent | — Pending |
| ratatui full cockpit (every CLI action available in TUI) | TUI as primary human interface, not a viewer | — Pending |

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
*Last updated: 2026-08-20 after initialization*
