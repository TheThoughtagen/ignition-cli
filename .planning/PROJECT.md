# ignition-cli

## What This Is

A Rust CLI and ratatui TUI cockpit for Ignition by Inductive Automation gateways — a unified, agentic-friendly developer tool that replaces the gateway webpage and minimizes Designer usage. It wraps gateway REST + WebDev APIs for health checks, project operations, and tag operations, plus Docker test-rig lifecycle control, cross-gateway diff/sync, gwbk backups, EAM lifecycle writes, opt-in script execution, and declared gateway config deploy (`ign deploy render/plan/pull/apply/verify/rollback/projects`) to a redundant 8.3 pair over system OpenSSH. The TUI is the full cockpit for humans; every command is also scriptable with JSON output for AI agents, which can additionally drive `ign` over MCP (`ign mcp serve`) or consume live gateway truth in nvim through `ign lsp`. Round-trip bridges to the ecosystem (nvim editing via `ign edit`/workspace checkout and the Flint codec, `ign lint` delegation, offline git-module export browsing) close the loop with the author's other tooling.

## Core Value

One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.

## Current State

**v1.6 Declared Gateway Deploy shipped 2026-10-29** (Phases 20-25, 92 plans, 220 tasks over 20 days; 524 commits; ~247,000 lines Rust, ~3,056 test fns). Audit: `tech_debt`, 47/47 requirements, verified closeout.

Shipped in v1.6:
- Library config API client (`DeployApi`): config resources with signature-checked PUT, scan-lock, scan completion, deployment modes; every endpoint capture-backed on 8.3.6
- `ign deploy render`: offline, deterministic, byte-faithful render of `gateways/<name>/` for one environment, fail-closed on secrets, lockout types and Windows-unsafe paths
- `ign deploy plan`, `pull` and master `verify`: read-only over hardened system OpenSSH/sftp (Linux and Windows hosts) or the local filesystem; frozen verdict envelope (exit 6)
- Guarded `ign deploy apply` and `rollback`: scan-lock with a measured 176 s hold, snapshot plus `.gwbk`, one hash-checked on-box commit script per host, auto-rollback, journal recovery, run store
- Redundant pair: backup verified by API and file hashes, never written; `deploy_backup_unsynced`; `verify --node both`
- Guarded `ign deploy projects` plus a reusable `deploy-projects.yml` workflow usable from any organization

Carry-forwards (tracked, not blockers; full list in `milestones/v1.6-MILESTONE-AUDIT.md`):
- Phase 25 review fixes WR-01/02/05 await human verification; WR-04 release attestation is live-testable only on the next release
- No SECURITY.md for phases 23-24; Nyquist VALIDATION.md still draft for phases 20-24
- Render residuals: secret key-name rule matches string values only; a mis-cased `--env` on a case-insensitive filesystem changes name-based uuids
- Two timing-sensitive tests tracked as backlog Phase 999.1
- Branch `phase-25-gaps-rc3` is 48+ commits ahead of `origin/main` with no PR; the `v1.6` milestone tag is local only (release.yml fires on any `v*` tag push)

<details>
<summary>v1.1 state (shipped 2026-09-16)</summary>

**v1.1 Agent Surface & IDE Integration shipped 2026-09-16** (Phases 8-14, 47 plans, 113 tasks over 12 days; 236 commits; ~107,954 lines Rust, ~1,255 test fns).

Shipped in v1.1 (all 25 requirements validated):
- `ign api call` raw REST passthrough + curated daily diagnostics (license/redundancy/gan status, diagnostics bundle) — live-verified on 8.3.3 + 8.3.6
- Full guarded EAM write lifecycle (suspend/resume/cancel/force/modify/delete) behind a blast-radius preview gate — live-proven on a disposable WHK controller rig
- Byte-faithful XML/CSV tag bulk transfer with loss-report gates (WebDev route bundle 1.1.0 → 1.3.0); historian tag-binding closed via spike verdict CLOSURE
- `ign workspace checkout/status/push` (injective hostile-name-safe path mapping, manifest three-way compare) + `ign edit` fetch→decode→$EDITOR→encode→push round-trip
- `ign mcp serve` — hand-rolled JSON-RPC 2.0 stdio, 83-tool clap-derived catalog, confirm gate — proven live by a real Claude Code client
- `ign lsp` — TTL-cached gateway truth feeding ignition-nvim completions/hover/diagnostics (headless e2e'd; sibling patch on branch `claude/ign-lsp-live-client` pending user merge)
- TUI theming: four named themes × four capability tiers, WCAG-AAA dark body contrast, CI tokenization gate, per-profile polling cadence

Carry-forwards (tracked, not blockers):
- ignition-nvim patch merge to that repo's main is user-owned sequencing (commit 0d6bd55)
- EAM live-gate §2 vanish-poll deadline needs a capture-driven re-size (10-LIVE-GATE.md §4 D1/§6)
- `trial_reset` defensive-tail error message names a URL instead of a profile (minor UX polish)

</details>

## Next Milestone Goals

Not yet defined; `/gsd-new-milestone` sets them. Candidates already on record:
- Unpark v1.3 Phases 17 (git-module commissioning: GITM-01..04) and 18 (declared rig discovery and v1.2 ledger backfill: RDISC-01..02, LEDG-01)
- v1.6 deferred requirements: `ign drift` (roadmap Plan 6), `ign redundancy restart --rolling` (Plan 5), guarded `apply --strip-core` (SF-5), MCP exposure of `apply`/`rollback`/`projects`
- Backlog 999.1 and the v1.6 tech debt above

## Requirements

### Validated

**v1.1 (shipped 2026-09-16) — 25 requirements:**

- ✓ Session execution core, one-migration config schema, executable contract rituals (Three-Place slug rule, OutOfBand taxonomy, stdout purity) — v1.1 (CORE-09/10/11)
- ✓ `ign api call` raw passthrough + curated diagnostics, both rigs — v1.1 (EXT-01/02)
- ✓ MCP transport with clap-derived catalog + confirm gate, real-client proven — v1.1 (EXT-04)
- ✓ Full guarded EAM write lifecycle with blast-radius preview, live-proven — v1.1 (EAMW-01..07)
- ✓ Byte-faithful XML/CSV tag transfer with loss gates + historian binding closure — v1.1 (TAGS-10..14)
- ✓ TUI themes × capability tiers + per-profile polling cadence — v1.1 (TUIX-03/04/05)
- ✓ Workspace checkout/status/push + `ign edit` round-trip + `ign lsp` feeding nvim — v1.1 (IDE-01..04)

**v1.0 (shipped 2026-08-30) — 44 requirements:**

- ✓ Gateway health operations (status, info, modules, logs, restart, sessions, connections, metrics, doctor, wait) — v1.0
- ✓ Project operations (CRUD, import/export, surgical resource edit, cross-gateway diff/sync) — v1.0
- ✓ Tag operations (providers, browse, read/write, configs, UDTs, alarms, history, bulk JSON transfer) via own WebDev backend — v1.0
- ✓ Test-rig lifecycle (docker up/down/status/reset, trial resets, snapshot/restore) — v1.0
- ✓ JSON output mode on all subcommands (frozen envelope + exit-code taxonomy) — v1.0
- ✓ Ratatui TUI cockpit exposing all CLI capabilities (CI-enforced parity) — v1.0
- ✓ Gateway profile/config management (multiple gateways, env/keyring secret chain) — v1.0
- ✓ Backups/EAM (gwbk download/restore, guarded EAM tasks) and opt-in script exec — v1.0
- ✓ Ecosystem interop (decode/encode scripts, lint delegation, offline export browsing) — v1.0

**v1.6 (shipped 2026-10-29) — 47 requirements:**

- ✓ Config API client (library-only): config resources with signature-checked PUT, scan-lock, scan completion, deployment modes; every endpoint capture-backed — Phase 20 (DAPI-01..07; `ign redundancy status` CLI wiring carried to Phase 24)
- ✓ Offline declared-tree render: `ign deploy render --env` (strict YAML, `modes/<env>` overlay, tags remap, policy refusals, Gson-compatible writer, deterministic uuid, fail-closed secret/Windows/CR lints, guarded `--out` with `render.json`); corpus byte-identical on Linux/macOS/Windows (CI run 38483193605) — Phase 21 (DRND-01..08, DSURF-01)
- ✓ Read-only deploy loop: transport (system OpenSSH and local filesystem, write half built but uncalled), `ign deploy plan`, `pull` and master `verify` with the six-check verdict envelope (exit 6 on red); a pulled tree verifies green against its own gateway (symmetric null rule) — Phase 22 (DPLN-01..09, DSURF-04)
- ✓ Guarded single-gateway apply and rollback: `ign deploy apply` (plan-hash handshake, destructive/drift gates, all-at-once preflights, snapshot plus `.gwbk`, scan-lock with measured hold, auto-rollback through the same engine, on-box journal, private run store) and fresh-process `ign deploy rollback --run <id>`; live on Windows and Linux ssh hosts — Phase 23 (DAPL-01..10, DSURF-02, DSURF-03)
- ✓ Redundant pair: backup verified through its own API and file hashes and never written, `deploy_backup_unsynced` never rolls back the master, `verify --node both` checks referenced secrets on each node, two-node 8.3.6 live gate; `ign redundancy status` wired to `active_node` — Phase 24 (DPAIR-01..04, DAPI-07 CLI half)
- ✓ Guarded project deploy: `ign deploy projects` (artifact pinned by its reviewed canonical hash, parent read-only, export before overwrite and hash-verified read-back, create when absent, `--check` with zero writes) plus CI by composite actions or the reusable workflow from any organization; live-proven cross-org on a self-hosted Windows runner — Phase 25 (DPRJ-01..05)

### Active

Open requirements are the parked v1.3 set in `.planning/REQUIREMENTS.md` (GITM-01..04, RDISC-01..02, LEDG-01). The next milestone adds its own.

### Out of Scope

- Ignition 8.1.x support — git-module v2 policy precedent; 8.3+ only keeps API surface tight (held through v1.1; zero 8.1 pressure)
- Designer-side integration — that's ignition-git-module's job (the CLI bridges to its exports and now its own workspace tree, doesn't replace the Designer)
- Linting engine — ignition-lint owns it; the CLI delegates only (`ign lint` shipped in v1.0)
- Full MCP-style serving beyond the tools-only shim — v1.1 shipped `ign mcp serve` as a thin stdio transport over the frozen JSON contract; resources/prompts/sampling deferred to EXT-06 with documented rmcp escalation
- Live file-watcher auto-push editing — the no-daemon constraint held through v1.1: explicit `ign edit` round-trip and workspace diff/sync only (watch-mode = IDE-05, deferred)
- CLI-side XML/CSV re-serialization of tag exports — proven wrong-shaped in v1.1: passthrough won (fidelity oracle); CSV is CLI-generated from the JSON interchange only
- Hand-written MCP tool catalog — catalog derives from `Cli::command()` + CI parity test (v1.1 shipped the mechanism)
- Breaking the frozen JSON envelope (new shapes, renumbered exit codes) — contract is the product; v1.1 grew additively only (two dated transport-aware prose exceptions in README: api-call, MCP refusal)
- Built-in editor UI in TUI — nvim + LSP owns editing; TUI stays a cockpit
- OpenAPI discovery / codegen from gateway — no stable OpenAPI surface on real 8.3 gateways
- Tag values in the workspace tree — values are runtime state, not source; keeps workspace diffable (v1.1 shipped the exclusion pin)
- Deploy safety bypasses (`--force`, `--skip-verify`, `--no-rollback`), auto-reverting drift, and any write to the backup node — the v1.6 design depends on each safety (held through v1.6)
- An SSH library dependency (russh, ssh2, openssh) — v1.6 proved system OpenSSH on Linux and Windows hosts; MSRV and Windows client gaps rule the libraries out
- Deleting projects absent from the deploy manifest — unmanaged projects are reported, never removed (DPRJ-04)

## Context

**Shipped v1.6 on 2026-10-29** — 6 phases, 92 plans, 220 tasks over 20 days; ~247,000 lines of Rust; ~3,056 Rust test fns. Live evidence on 8.3.6 rigs, a two-node pair, an Azure Windows Server 2022 host (Win32-OpenSSH, PowerShell 5.1) and a Linux ssh host; CI green on ubuntu, macOS and Windows.

**Shipped v1.1 on 2026-09-16** — 7 phases, 47 plans, 113 tasks over 12 days. ~107,954 lines of Rust (3 crates) + ~1,022 lines of Python WebDev routes embedded in the binary; ~1,255 Rust test fns; WebDev route bundle versioned 1.3.0; live evidence on both 8.3.3 and 8.3.6 rigs for every capture-first phase (9, 10, 11, 13) plus a real Claude Code MCP smoke (14-06).

- Ecosystem this completes (all by the same author / WhiskeyHouse):
  - `ignition-mcp` (~/whiskeyhouse/ignition-mcp) — **v1.1 fully replaces it**: `ign mcp serve` is the agent transport now (83 tools derived from the clap tree).
  - `ignition-git-module` (~/whiskeyhouse/ignition-git-module) — rig lifecycle pattern source; tag exports browsable offline.
  - `ignition-nvim` (~/whiskeyhouse/ignition-nvim) — **v1.1 composition is live**: `ign lsp` (ignition_live client) + `ign edit`/workspace checkout; Python ignition-lsp retains statics ownership.
  - `ignition-lint` (~/whiskeyhouse/ignition-lint) — `ign lint` delegates to it.
  - `83-api` (~/whiskeyhouse/83-api) — REST reference; api-call family grounds the curated reads.
  - `WHK-Global` (~/data/projects/WHK-Global) — parent project; EAM write lifecycle live-proven against its controller (disposable-rig substitution).
- Live-verified semantics on 8.3.3 and 8.3.6 rigs; point-release variance handled by capture-first planning (wire truth recorded in per-phase *-LIVE-CAPTURES.md docs before code).
- Primary user is the author (developer + their AI agents); secondary: Whiskey House E&T team.
- Known technical debt / honest limitations: EAM live-gate §2 vanish-poll deadline unproven (capture work scoped); UDT-instance parameter overrides silently drop when target type is unresolvable on 8.3.3 (gateway behavior, documented); `trial_reset` error message polish; mixed-parent XML/CSV export corruption pre-exists in the gateway's JSON interchange (advisory-detected, never gated).

## Constraints

- **Tech stack**: Rust + ratatui for TUI; clap for CLI — keep the dependency tree lean (held: v1.1 added quick-xml (no serde), csv, base64, lsp-server, crossbeam-channel, proptest (dev-only) — all justified in-repo)
- **Compatibility**: Ignition 8.3.1+ only (matches ignition-git-module v2 support policy; live-verified 8.3.3/8.3.6)
- **Agentic usage**: every subcommand must be non-interactive by default with `--json` output; TUI is opt-in; transports (MCP/LSP) own stdout completely (frozen contract — changing envelope/exit codes is a breaking change for agents; transport-aware prose exceptions are dated README entries)
- **Simplicity**: "simple but complete" — no daemon required (watch-mode deferred); hand-rolled JSON-RPC over rmcp until a consumer exists
- **Pairing**: config conventions interoperate with WHK-Global and git-module rigs (compose file discovery, env/secrets)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| CLI replaces ignition-mcp (not complementary) | One canonical interface; agents drive CLIs well with JSON output; Rust beats Python for a daily-driver tool | ✓ Good — v1.1 completed the replacement: MCP transport live-proven by a real Claude Code client (83 tools) |
| Ship own WebDev routes | Independence from WHK-Global deployment; versioned with the CLI | ✓ Good — version-locked 1.3.0 after two atomic v1.1 bumps (1.2.0, 1.3.0); stale-deploy refusal live-proven |
| 8.3+ only | API variance in 8.1 not worth carrying; ecosystem precedent | ✓ Good — zero 8.1 pressure across two milestones of live work on 8.3.3/8.3.6 rigs |
| ratatui full cockpit (every CLI action available in TUI) | TUI as primary human interface, not a viewer | ✓ Good — parity CI-enforced incl. v1.1's seven new routes/verbs; themes user-approved after 3-round UAT |
| Frozen agentic output contract (envelope `{ok,profile,data}`, exit taxonomy 0-7, stderr diagnostics) | Agents key on stable shapes; the contract is the product | ✓ Good — additive-only slug growth through v1.1; two dated transport-aware prose exceptions, byte-frozen CLI envelope |
| Resource editing via export→zip-member-surgery→import | Per-resource REST routes don't exist on real 8.3 gateways (triple-verified) | ✓ Good — generalized into the MemberSource engine in v1.1 (zip + tree transports, one implementation) |
| Native OIDC trial reset over Playwright delegation | Playwright needs Node+chromium and broke across the 8.3.3 UI rewrite | ✓ Good — live-proven on both rigs |
| scriptExec structural opt-in (deploy-time secret, fail-closed route) | Security posture: public template can never arm the gate | ✓ Good — redaction proven at action and binary level |
| **v1.1** Session seam concrete-with-deref (Arc<ReqwestGatewayApi>), dyn-widening deferred to transport need | Zero second-source construction provable by grep; no speculative trait churn | ✓ Good — Phase 14's MCP/LSP built directly on the seam without widening it |
| **v1.1** Capture-first planning: live rig captures recorded before any model/code task | Plan hypotheses about wire shapes are guesses; captures falsify them cheaply | ✓ Good — falsified 405 (→404-empty), PUT-rename (→404), signature-mismatch (→500), UDT folder-vs-provider-root; wire truth docs (09/10/11/13-LIVE-CAPTURES.md) are the planning currency |
| **v1.1** Byte-faithful tag passthrough over CLI re-serialization; CSV is CLI-generated from the JSON interchange | Gateway bytes are the truth; re-serialization corrupts fidelity (the documented rabbit hole) | ✓ Good — sha256 fidelity oracle held live on both rigs; loss scans advisory, gateway owns refusals |
| **v1.1** Hand-rolled JSON-RPC 2.0 MCP shim (not rmcp); catalog derives from clap tree | ~500 lines over a stable surface beats a framework dependency; derivation kills catalog drift | ✓ Good — 83-tool catalog with CI parity; transport added zero envelope changes (prose exceptions only) |
| **v1.1** One guarded-write gate site (`preview_then_confirm`); refusal message IS the deterministic preview | Per-verb gates drift; agents read the blast radius from stderr alone | ✓ Good — all six EAM verbs + edit + workspace push compose the identical refusal shape |
| **v1.1** Fail-closed injective path mapping (percent-escaping, proptest-proven) + manifest-as-workspace-identity | Hostile names and case-folded filesystems are the clobber class; the manifest is the fs truth | ✓ Good — sabotage-proven tests; staleness/conflict gates NOT --yes-able |
| **v1.1** Evidence-ledger discipline for human-verify checkpoints (VERIFIED / MACHINE-EVIDENCE / WAIVED) | Checkpoints must close honestly, never silently merged | ✓ Good — adopted at 14-06; carry-forward labels recorded per item |
| **v1.6** Separate `DeployApi` trait with one stateful `FakeDeploy`, checked tag-for-tag against a live 8.3.6 run | Avoids editing 15 `GatewayApi` doubles; a single fake that later phases share must not encode guesses | ✓ Good — Phase 20 live trace equals fake trace (14 steps) after review fixes |
| **v1.6** 8.3.6 wire truth overrides research and OpenAPI | Captures falsified 408-on-held-lock (→409 HTML), `filter=` syntax (→`field[op]=v`), a release route (none), and declared 400s (→500 NPE) | ✓ Good — D1-D12 in 20-LIVE-CAPTURES.md are the planning currency for Phases 21-25 |
| **v1.6** Deploy refusals are typed per helper, never in generic `classify` | Keeps the v1.1 EAM exit contract frozen while deploy gets `scan_lock_held`, `resource_signature_rejected`, `resource_exists`, `mode_exists` (exit 6) | ✓ Good — `eam_contract` and `api_classify_contract` unchanged |
| **v1.6** Render is fully offline and fail-closed: secrets refused by path (never echoed), strict `Referenced` shape, policy deny-list and `config-mode.json` refusal | A render that ships a plaintext secret or locks out the gateway cannot be undone by a later phase | ✓ Good — 44/44 threats closed at ASVS L1 (21-SECURITY.md); WR-03 `--out` refresh crash-safety open before Phase 22 reads `render.json` |
| **v1.6** `verify` read_back treats JSON null as absent on BOTH the file and the REST side | 8.3.6 REST echoes unset members as explicit null; a one-sided strip made every pulled `ignition/cobranding` differ (G-22-2) | ✓ Good — UAT re-test 2026-10-15: unmodified pulled tree exit 0, Bad probe still exit 6 |
| **v1.6** Decision A: post-write red outcomes keep the frozen envelope (`ok:true`, `data.ok=false`, verdict slug, exit 6); pre-write refusals stay `CoreError` envelopes | Agents need the run record even when the deploy is red; the envelope stays frozen | ✓ Good — documented as a dated README contract exception (DSURF-04) |
| **v1.6** Decision B(i): home-relative staging outside `data/config/`, committed by rename; ledger and journal under `ign-deploy/<gateway>/<mode>/` | Staging inside the mode collection would be scanned mid-write | ✓ Good — live on Windows and Linux hosts; spec §9 amendment PR #63 |
| **v1.6** Decision C: CI and agents must pass `--plan-sha256`; optional interactively | The reviewed plan is the only plan that may be applied | ✓ Good — enforced before any contact; `plan_stale` refusal |
| **v1.6** Decision D: `apply`, `rollback`, `projects` not MCP tools; `render`, `plan`, `pull`, `verify` are | Write verbs stay human- or CI-gated until the handshake has been used | — Pending — revisit MCP exposure in a later milestone |
| **v1.6** One hash-checked on-box commit script per host (POSIX `sh`, PowerShell 5.1) instead of per-file remote calls | Per-file renames took 234-348 s for 152 files on Windows; batched took 6.92 s and fits the lock | ✓ Good — measured hold 176 s inside the 300 s ceiling |
| **v1.6** Rollback and auto-rollback reuse the apply engine's `restore_snapshot` | One restore path is tested once | ✓ Good — fresh-process rollback, kill tests and lapsed-lock live gates passed |
| **v1.6** Backup node is verified, never written; a backup failure never rolls back a good master | Redundancy sync owns the backup | ✓ Good — `deploy_backup_unsynced` exit 6, two-node live gate |

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
*Last updated: 2026-10-29 after v1.6 milestone*
