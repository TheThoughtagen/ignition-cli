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