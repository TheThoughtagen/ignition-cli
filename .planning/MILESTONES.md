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
## v1.6 Declared Gateway Deploy (Shipped: 2026-10-29)

**Phases:** 20-25 · **Plans:** 92 · **Tasks:** 220 · **Timeline:** 20 days (2026-10-10 → 2026-10-29)
**Code:** ~247,000 lines Rust (up from ~108,000 at v1.1) · **Tests:** ~3,056 Rust test fns
**Git range:** 41315d8 (docs: start milestone v1.6) → milestone archive commit · 524 commits, 692 files changed (+230,498/−1,655; crates: +128,280/−234)
**Closeout:** verified (all 6 phases `passed`, pre-close artifact audit clear after quick 261029-fyc); audit status `tech_debt`, 47/47 requirements satisfied

**Delivered:** `ign deploy` — a declared gateway config tree (`gateways/<name>/`) deployed to a redundant Ignition 8.3 pair through the `production` deployment-mode collection: render, plan, pull, apply, verify and roll back, plus guarded project deploy, with no dependency on the git module. All 47 v1.6 requirements shipped across 6 phases.

**Key accomplishments:**

- Config API client — `DeployApi` over 8.3.6 config resources, scan-lock, scan and deployment modes, with success predicates taken from live captures (13 capture-backed endpoint shapes), `scan_lock_held` naming the holder, signature-checked PUT, and `active_node` by activity level
- Offline render — `ign deploy render` produces the exact on-disk bytes for one environment: strict YAML, mode overlays, type allowlist, Gson-compatible order-preserving writer (183-file corpus byte-identical on Linux, macOS and Windows), name-based v8 uuids, and refusal of embedded secrets, lockout types and Windows-unsafe paths
- Read-only pipeline — hardened system-OpenSSH/sftp and local transports (Linux and Windows hosts), `deploy plan` with exact delete set, core relation and `--base` drift, `deploy pull` that bootstraps a tree whose plan is empty, and master `deploy verify` with tag probes and the frozen verdict envelope (exit 6)
- Guarded apply and rollback — `ign deploy apply` under a measured scan-lock hold (176 s), all-at-once preflights, host and run-dir snapshots plus `.gwbk`, one hash-checked on-box commit script per host (POSIX `sh` and PowerShell 5.1), auto-rollback through the same engine, on-box journal recovery, and fresh-process `deploy rollback --run <id>`; live-gated on Windows and Linux hosts
- Redundant pair — the backup is verified through its own API and file hashes, never written; `deploy_backup_unsynced` never rolls back a good master; `deploy verify --node both` checks referenced secrets on each node; two-node 8.3.6 live gate
- Guarded project deploy — `ign deploy projects` with sha256-pinned artifacts, parent-first import, export before overwrite, canonical-hash read-back, `--check` with zero writes, never deleting unmanaged projects, refusal of git-managed ones, and a reusable `deploy-projects.yml` workflow proven cross-org on a self-hosted Windows runner

**Known tech debt:** 14 items, listed in [v1.6-MILESTONE-AUDIT.md](milestones/v1.6-MILESTONE-AUDIT.md) (secret key-name rule string-only, case-insensitive-FS uuid divergence, no SECURITY.md for phases 23-24, Nyquist validation still draft for phases 20-24, Phase 25 review fixes WR-01/02/04/05 awaiting human verification); flaky tests tracked as backlog Phase 999.1

**Archives:** [v1.6-ROADMAP.md](milestones/v1.6-ROADMAP.md) · [v1.6-REQUIREMENTS.md](milestones/v1.6-REQUIREMENTS.md) · [v1.6-MILESTONE-AUDIT.md](milestones/v1.6-MILESTONE-AUDIT.md) · phases in [v1.6-phases/](milestones/v1.6-phases/)

---
