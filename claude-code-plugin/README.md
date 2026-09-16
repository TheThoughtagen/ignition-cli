# ignition-ops — Agent Skills for the `ign` CLI

Skills that teach AI agents how to operate Ignition 8.3+ gateways with [`ign`](https://github.com/TheThoughtagen/ignition-cli) — the single Rust binary that replaces the gateway webpage as the human/agent interface. Pairs with the built-in MCP transport (`ign mcp serve`, 83 tools derived from the CLI's own parser).

## Skills

| Skill | Use when |
|-------|----------|
| `ign-contract` | Scripting `ign` at all — envelope shapes, exit taxonomy, guarded writes, `api call` escape hatch, MCP/LSP |
| `ign-gateway` | Daily health checks, diagnostics bundles, logs, doctor, restart/wait |
| `ign-tags` | Providers, values, config CRUD, UDTs, alarms, history, byte-faithful XML/CSV bulk transfer with loss gates |
| `ign-workspace` | Workspace checkout/status/push, `ign edit` round-trip, live LSP truth |
| `ign-rigs` | Docker compose test rigs, OIDC trial windows, commissioning waits |

## Install

### Claude Code (plugin)

```
/plugin marketplace add TheThoughtagen/ignition-cli
/plugin install ignition-ops@ignition-cli
```

### Any agent harness (`npx skills`)

```bash
npx skills add TheThoughtagen/ignition-cli            # interactive
npx skills add TheThoughtagen/ignition-cli -g -s '*'  # all skills, user-level
```

Works with any harness the [skills CLI](https://github.com/vercel-labs/agent-skills) supports (Claude Code, Cursor, Codex, opencode, …).

## Prerequisites

- `ign` 1.1+ on PATH — `cargo install ignition-cli` (macOS arm64/x64, Linux x64/arm64; **no Windows build**)
- A reachable Ignition 8.3+ gateway and a configured profile (`ign profile add`), or a Docker rig for `ign rig`
- `jq` optional (examples use it for envelope filtering)

## Layout

```
.claude-plugin/marketplace.json      # marketplace: /plugin marketplace add
claude-code-plugin/
  .claude-plugin/plugin.json         # plugin: ignition-ops
  skills/<skill>/SKILL.md            # harness-agnostic Agent Skills spec
```
