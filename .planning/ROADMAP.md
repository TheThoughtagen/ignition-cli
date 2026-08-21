# Roadmap: ignition-cli

## Overview

Build `ign` — a single Rust binary + ratatui cockpit that replaces both the Ignition gateway webpage and the author's ignition-mcp server as the canonical human/agent interface to Ignition 8.3+ gateways. The journey climbs the dependency graph: agentic contracts and profiles first, then read-only inspection against any gateway, then mutating project operations, then the Docker test rig that serves as the self-managed fixture for the WebDev backend and full tag runtime operations (the ignition-mcp replacement bar), and finally the TUI cockpit and ecosystem interop that ride the finished action surface.

**Mode:** mvp — every phase delivers an end-to-end user capability (vertical slice), not a horizontal layer.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Foundation & Agentic Contracts** - The `ign` binary with profiles, auth, JSON output, error envelopes, exit codes — the contract every later command inherits
- [ ] **Phase 2: Gateway Health & Inspection** - Read-only gateway operations against any 8.3 gateway: status, modules, logs, connections, metrics, sessions, restart, doctor, wait
- [ ] **Phase 3: Project Operations** - Full project CRUD, export/import, and surgical resource editing — the gateway webpage's project pages replaced
- [ ] **Phase 4: Rig Lifecycle & Trial State** - Docker test rig up/down/status/reset with logs, trial status/reset, and snapshot/restore
- [ ] **Phase 5: WebDev Backend & Tag Operations** - Ship the CLI's own versioned WebDev routes, deploy them, and operate the full tag lifecycle: values, configs, UDTs, alarms, history — the ignition-mcp replacement bar
- [ ] **Phase 6: TUI Cockpit** - Ratatui cockpit exposing every CLI action: dashboard, log tail, tag browser with live watch, alarm panel, project browser, profile switcher
- [ ] **Phase 7: Ecosystem Interop & Advanced Ops** - Cross-gateway diff/sync, gwbk backups, EAM tasks, opt-in script exec, and delegation bridges to ignition-lint / git-module / nvim workflows

## Phase Details

### Phase 1: Foundation & Agentic Contracts
**Goal:** The `ign` binary is installable and configurable — a user can set up multiple gateway profiles with secure auth, and every command honors the machine-readable output contract (JSON, errors, exit codes) that agents and all later phases depend on.
**Mode:** mvp
**Depends on**: Nothing (first phase)
**Requirements**: CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-06, CORE-07, CORE-08
**Success Criteria** (what must be TRUE):
  1. User can add, list, and switch between multiple gateway profiles (`--profile` / `IGNITION_PROFILE`), and the active profile name is visible in every command's output
  2. User can authenticate via API token (preferred) or basic auth from env/config/keyring, and secrets never appear in JSON output or logs (redaction verified by test)
  3. Every subcommand supports `--json` with stable field names and `--compact`; errors render as a JSON envelope with code, message, endpoint, and actionable hint; exit codes follow the documented taxonomy (0/2/config/auth/network/target-state) — all enforced by golden-file CI tests
  4. Commands are non-interactive by default; destructive operations refuse without `--yes` or `IGNITION_YES`
  5. `ign version` checks the gateway minimum version and refuses cleanly on <8.3.1; `ign completions` generates bash/zsh/fish
**Plans**: 4 plans (sequential waves 1→4 — all share main.rs/cli.rs/error.rs, so no parallelism is honest for this shared-foundation phase)

*Planner refinement:* skeleton order 01-02/01-03 swapped — output contract now precedes profiles per research's primary recommendation ("build the contract first — every subsequent subcommand test then enforces the contract for free"), so `profile add/list/use` ship envelope-complete with zero render rework. Locked decisions: envelope `{"ok","profile","data"}`, exit-code table 0-7 (config=3, network=4, auth=5, target-state=6), `async_trait` for GatewayApi, `ign version` unreachable→exit 0 + warning, no Windows CI.

Plans:
- [ ] 01-01-PLAN.md — Cargo workspace skeleton + CLI chassis (3 crates, MSRV 1.88, 5 clap globals, single-exit main, tui feature gate, CI check job)
- [ ] 01-02-PLAN.md — Agentic output contract (CoreError 0-7 taxonomy, frozen envelope, --compact, snapbox golden harness, README exit-code table)
- [ ] 01-03-PLAN.md — Profiles, config & secrets (TOML + env overlay, SecretStore env-first, keyring smoke CI job closing the STATE.md blocker, redaction canary, profile add/list/use)
- [ ] 01-04-PLAN.md — Gateway seam & finish (GatewayApi + wiremock, `version` min-check 8.3.1 with locked behavior matrix, completions, `--yes` guard)

### Phase 2: Gateway Health & Inspection
**Goal:** A user can fully inspect and (carefully) restart any Ignition 8.3+ gateway from the terminal with zero gateway-side setup — the first webpage replacement, plus the `doctor` and `wait` primitives everything downstream reuses.
**Mode:** mvp
**Depends on**: Phase 1
**Requirements**: HLTH-01, HLTH-02, HLTH-03, HLTH-04, HLTH-05, HLTH-06, HLTH-07, HLTH-08, HLTH-09, HLTH-10, HLTH-11
**Success Criteria** (what must be TRUE):
  1. User can view gateway info/status (version, platform, revision, running state), module health, database connections, OPC connections, and system metrics (CPU/memory, historic + current, thread execution)
  2. User can view connected sessions (designers, Perspective, Vision) and terminate a session
  3. User can list, fetch, download, and tail (`-f`) gateway logs, and get/set per-logger levels
  4. User can restart the gateway (optionally `--wait` for ready) and run `ign wait` to poll for gateway up, restart complete, or module ready
  5. `ign doctor` diagnoses connectivity, auth (incl. the three-part token-setup failure causes), read/write permission, WebDev-route presence, and rig detection
**Plans**: 4 plans (TBD)

Plans:
- [ ] 02-01: GatewayClient — header auth (token + Basic fallback), per-class timeouts, HTML error-body sniffing; live-gateway auth verification (flagged gap)
- [ ] 02-02: status/info/modules, db/opc connections, metrics, sessions (+terminate)
- [ ] 02-03: logs (list/fetch/download/tail) + logger levels get/set
- [ ] 02-04: restart `--wait`, `wait` primitives, full `doctor`

### Phase 3: Project Operations
**Goal:** A user can create, move, export, import, and surgically edit Ignition projects entirely from the CLI — the gateway webpage's project management replaced, and the first mutating commands prove the `--yes`/collision-policy conventions.
**Mode:** mvp
**Depends on**: Phase 2
**Requirements**: PROJ-01, PROJ-02, PROJ-03, PROJ-04, PROJ-05
**Success Criteria** (what must be TRUE):
  1. User can list projects with inheritance/parent info, and create, delete, copy, and rename projects (deletes guarded by `--yes`)
  2. User can export a project to file and import from file or stdin, with collision policy (abort/overwrite) honored and long imports surviving via per-operation timeouts (streamed ZIPs to disk)
  3. User can list, get, put, and delete individual resources within a project — the surgical edit loop (change one view/script without re-importing everything)
  4. Export/import JSON metadata states scope explicitly (`includes`/`excludes`) so users know tag providers are not part of a project export
**Plans**: 3 plans (TBD)

Plans:
- [ ] 03-01: project list/new/copy/rename/delete
- [ ] 03-02: export/import — streaming ZIP, timeout policy, collision policies, stdin import
- [ ] 03-03: resource ls/get/put/delete + scope metadata + first e2e harness skeleton

### Phase 4: Rig Lifecycle & Trial State
**Goal:** A user can run a complete Docker test rig from the CLI — up/down/status/reset with compose discovery, logs, trial state management, and snapshot/restore — giving the project (and CI) a self-managed gateway fixture.
**Mode:** mvp
**Depends on**: Phase 2 (client + wait primitives)
**Requirements**: RIG-01, RIG-02, RIG-03, RIG-04
**Success Criteria** (what must be TRUE):
  1. `rig up/down/status/reset` works from compose-file discovery (flag → env/config → cwd → git-module docker dir → WHK-Global conventions), with port-collision pre-flight and wait-for-commissioned after `up`
  2. `rig reset` tears down cleanly (orphans + volumes removed, explicit compose project names) — no stale trial state survives a reset
  3. User can view rig logs (passthrough) and trial status (banners endpoint), and reset trial state via the spike-chosen mechanism verified against ≥2 gateway minor versions
  4. User can snapshot a rig (gwbk download + project/tag exports) and restore it to a repeatable state
**Plans**: 4 plans (TBD)

**⚠ Spike required (at planning):** trial-reset mechanism — delegate to existing Playwright resetter vs native headless HTTP login+CSRF flow in Rust (researcher divergence; see research/SUMMARY.md).

Plans:
- [ ] 04-01: compose shell-out (v2 check, explicit `-p` names, `--remove-orphans`), 5-level rig discovery, up/down/status
- [ ] 04-02: reset (volume teardown, port pre-flight, post-reset scan + wait) + logs passthrough
- [ ] 04-03: trial status (banners) + trial-reset spike & implementation
- [ ] 04-04: snapshot/restore (native gwbk API + project/tag exports)

### Phase 5: WebDev Backend & Tag Operations
**Goal:** A user can deploy the CLI's own versioned WebDev routes to a gateway and then operate the complete tag lifecycle — providers, browse, read/write values, config CRUD, UDTs, alarms, history, bulk transfer — reaching the ignition-mcp replacement bar.
**Mode:** mvp
**Depends on**: Phase 3 (project import machinery), Phase 4 (rig as deploy/e2e fixture)
**Requirements**: WEB-01, WEB-02, TAGS-01, TAGS-02, TAGS-03, TAGS-04, TAGS-05, TAGS-06, TAGS-07, TAGS-08, TAGS-09
**Success Criteria** (what must be TRUE):
  1. `ign webdev deploy` installs the CLI's versioned routes and `ign webdev status` verifies them; CLI refuses WebDev-dependent commands with an actionable error on route/version mismatch
  2. User can list/create/delete tag providers, browse tags as a filtered tree, and read (single/batch) and write tag values — all through the deployed routes
  3. User can get/create/edit/delete tag configs (JSON in/out) and list UDT types / get UDT definitions
  4. User can view active alarms, query alarm history, acknowledge alarms, and query tag history
  5. User can bulk export/import tag providers (json/xml/csv) with collision policy defaulting to abort
**Plans**: 5 plans (TBD)

**⚠ Spikes required (at planning):** (a) WebDev deploy mechanism — per-resource import vs full project-zip import via 8.3 REST; (b) script-exec security posture; (c) tag-history route availability on default rigs. Use `ignition_tools_summary.json` as the authoritative parity checklist (37 vs 42 count divergence).

Plans:
- [ ] 05-01: webdev/ route sources (tags, tagConfig, alarms, scriptExec, tagHistory) with versioned contract + handshake
- [ ] 05-02: `webdev deploy/status` (post-spike) + version negotiation + serde normalization for stringified values
- [ ] 05-03: tag providers, browse, read/write values
- [ ] 05-04: tag config CRUD, UDT defs, bulk provider export/import
- [ ] 05-05: alarms (active/history/ack) + tag history

### Phase 6: TUI Cockpit
**Goal:** A user can open `ign tui` and drive every CLI capability through a k9s/lazygit-style cockpit — the primary human interface, structurally complete because TUI and CLI share the same actions layer.
**Mode:** mvp
**Depends on**: Phases 2–5 (complete action surface)
**Requirements**: TUI-01, TUI-02, TUI-03, TUI-04, TUI-05, TUI-06
**Success Criteria** (what must be TRUE):
  1. `ign tui` opens a cockpit with object-list → detail navigation, and a CI test asserts every CLI action has a TUI mapping (full coverage, not aspirational)
  2. User sees a live status dashboard (modules, sessions, metrics) with periodic refresh, and can switch profiles from within the TUI
  3. User can tail gateway logs with level filtering, without the UI ever blocking on gateway I/O
  4. User can browse tags, live-watch tag values, and view + acknowledge alarms in an alarm panel
  5. User can browse projects/resources and trigger project actions from the TUI
**Plans**: 4 plans (TBD)

Plans:
- [ ] 06-01: event-driven-async loop (AppEvent mpsc, `tokio::select!`, worker-task actions), AppState + TestBackend tests
- [ ] 06-02: status dashboard (modules/sessions/metrics) + profile switcher
- [ ] 06-03: log tail w/ level filter, tag browser + live watch, alarm panel
- [ ] 06-04: project/resource browser + CLI↔TUI coverage-mapping test

### Phase 7: Ecosystem Interop & Advanced Ops
**Goal:** The CLI plugs into the WhiskeyHouse ecosystem and handles the advanced workflows — cross-gateway promotion, backups/EAM, opt-in script execution, and round-trip editing with nvim/ignition-lint/git-module — completing the toolset.
**Mode:** mvp
**Depends on**: Phases 3–5 (export/import, WebDev routes, tag exports)
**Requirements**: SYNC-01, SYNC-02, SCRPT-01, BKUP-01, BKUP-02, INTR-01, INTR-02, INTR-03
**Success Criteria** (what must be TRUE):
  1. User can diff two gateways' projects at resource level and selectively sync resources between them (dev→test→prod), with project-vs-tag-provider scope explicit in command semantics and output metadata
  2. User can download and restore gateway backups (gwbk) via native API, list EAM task history, and create guarded EAM tasks
  3. `ign script run` executes gateway scripts via the scriptExec route — opt-in only (disabled by default)
  4. User can export with `--decode-scripts` / import with `--encode-scripts` for round-trip nvim/VS Code editing, run `ign lint` delegating to ignition-lint on PATH (with install hint if absent), and browse git-module tag exports offline via `--from-export`
**Plans**: 4 plans (TBD)

Plans:
- [ ] 07-01: cross-gateway diff + selective sync (explicit scope semantics)
- [ ] 07-02: gwbk backup download/restore + EAM task history/create
- [ ] 07-03: `script run` (opt-in, guarded) over scriptExec route
- [ ] 07-04: script decode/encode round-trip, `ign lint` delegation, `--from-export` tag browsing

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6 → 7

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation & Agentic Contracts | 0/4 | Not started | - |
| 2. Gateway Health & Inspection | 0/4 | Not started | - |
| 3. Project Operations | 0/3 | Not started | - |
| 4. Rig Lifecycle & Trial State | 0/4 | Not started | - |
| 5. WebDev Backend & Tag Operations | 0/5 | Not started | - |
| 6. TUI Cockpit | 0/4 | Not started | - |
| 7. Ecosystem Interop & Advanced Ops | 0/4 | Not started | - |
