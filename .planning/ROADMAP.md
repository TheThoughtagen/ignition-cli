# Roadmap: ignition-cli

## Overview

Build `ign` — a single Rust binary + ratatui cockpit that replaces both the Ignition gateway webpage and the author's ignition-mcp server as the canonical human/agent interface to Ignition 8.3+ gateways. The journey climbs the dependency graph: agentic contracts and profiles first, then read-only inspection against any gateway, then mutating project operations, then the Docker test rig that serves as the self-managed fixture for the WebDev backend and full tag runtime operations (the ignition-mcp replacement bar), and finally the TUI cockpit and ecosystem interop that ride the finished action surface.

**Mode:** mvp — every phase delivers an end-to-end user capability (vertical slice), not a horizontal layer.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Foundation & Agentic Contracts** - The `ign` binary with profiles, auth, JSON output, error envelopes, exit codes — the contract every later command inherits
- [x] **Phase 2: Gateway Health & Inspection** - Read-only gateway operations against any 8.3 gateway: status, modules, logs, connections, metrics, sessions, restart, doctor, wait
- [x] **Phase 3: Project Operations** - Full project CRUD, export/import, and surgical resource editing — the gateway webpage's project pages replaced
- [x] **Phase 4: Rig Lifecycle & Trial State** - Docker test rig up/down/status/reset with logs, trial status/reset, and snapshot/restore (2026-08-23)
- [x] **Phase 5: WebDev Backend & Tag Operations** - Ship the CLI's own versioned WebDev routes, deploy them, and operate the full tag lifecycle: values, configs, UDTs, alarms, history — the ignition-mcp replacement bar (2026-08-25; all five live e2e gates green on a real 8.3.3 rig; UAT gaps closed & re-verified 7/7 on 2026-08-26)
- [x] **Phase 6: TUI Cockpit** (Complete 2026-08-28 — 11/11 plans: 6 original + 5 gap-closure from 06-UAT) - Ratatui cockpit exposing every CLI action: dashboard, log tail, tag browser with live watch, alarm panel, project browser, rig screen; CLI<->TUI coverage CI-enforced (tui_coverage clap-tree walk); gap closure shipped: tags freshness refire, TTY hint, f64 gauges, root-level puts, modal geometry/vim motions/prose menus (06-10), grouped rig status render + README keymap sync (06-11); monochrome/color UX themes backlog-owned per UAT triage
- [x] **Phase 7: Ecosystem Interop & Advanced Ops** - Cross-gateway diff/sync, gwbk backups, EAM tasks, opt-in script exec, and delegation bridges to ignition-lint / git-module / nvim workflows (COMPLETE 2026-08-29 — 4/4 plans)

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
**Plans**: 5 plans (sequential waves 1→5 — every plan grows the single `impl GatewayApi` block in client/mod.rs plus the cli.rs/main.rs/render.rs choke files, so no parallelism is honest for this phase)

*Planner refinement:* the sketch's 02-02 carried six capabilities (status/modules/metrics/db/opc/sessions — HLTH-01/02/05/06/07/08) in one plan, over the 2–3-task context budget; split into status+modules+metrics (02-02) and sessions+connections (02-03). 02-01 absorbs the research-mandated Phase-1 correction (real 8.3 gateways return `ignitionVersion`, not `version`) plus three ADDITIVE exit-6 slugs (`gateway_not_commissioned`, `gateway_restarting`, `not_found`) from the live-verified error matrix. Locked decisions honored throughout: frozen envelope, coarse async_trait GatewayApi growth, single-site Secret::expose(), snapbox golden conventions, and the --yes guard (first destructive caller: `sessions terminate` in 02-03).

Plans:
- [ ] 02-01-PLAN.md — Live-truth client foundation: ignitionVersion fix + status→content-type→redirect classifier + additive taxonomy slugs + ListEnvelope/IgnitionMock harness + live-gateway suite (closes the flagged auth-verification gap)
- [ ] 02-02-PLAN.md — `ign status` / `ign modules` / `ign metrics` (overview + unauth /StatusPing anchor + systemPerformance endpoints; HLTH-01/02/07)
- [ ] 02-03-PLAN.md — `ign sessions` (+ terminate — first --yes caller, dead_code gate removed) + `ign connections` (resources/list database+opc with healthchecks passthrough; HLTH-08/05/06)
- [ ] 02-04-PLAN.md — `ign logs` tree (list / tail -f / download .idb) + logger levels get/set, shared poll helper (HLTH-03/04)
- [ ] 02-05-PLAN.md — `ign restart --wait` + `ign wait gateway|restart|module` + full `ign doctor` (three-part token-setup diagnosis; HLTH-09/10/11)

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
**Plans**: 3 plans (sequential waves 1→3 — every plan grows the single `impl GatewayApi` block in client/mod.rs plus the cli.rs/main.rs/render.rs choke files, so no parallelism is honest — the Phase 1/2 pattern)

*Planner refinement:* research validated the sketch with two adjustments — scope metadata (`includes`/`excludes`) moved into 03-02 (it is export/import output, not a resource concern), and `project set` (PUT modify; `--parent` = the inheritance move) made explicit in 03-01 since the roadmap's "move" maps to native rename/reparent. Locked decisions honored throughout: frozen envelope + additive-only slugs (`project_exists`, `invalid_import_file`, `resource_binary`), guard→resolve→action destructive dispatch (project delete, import-overwrite, and resource delete all fire `require_confirmation` BEFORE resolution), classify()-only error mapping, two-column wire-faithful/unit-explicit naming, serde-only actions, every new trait method stubbed into all inline test doubles, and the wiremock `set_body_raw`/MockGuard footguns.

Plans:
- [x] 03-01-PLAN.md — project CRUD: list (inheritance info) / new / copy / rename / set (reparent) / delete (`--yes` + `confirm=true` at both guard layers) + post_json/put_json pipeline helpers (PROJ-01/02)
- [ ] 03-02-PLAN.md — export (streaming ZIP to disk via reqwest stream + tokio fs, 120 s timeout) / import (file/stdin buffered, 300 s, application/zip) + `--collision-policy abort|overwrite` + scope metadata (PROJ-03/04)
- [ ] 03-03-PLAN.md — resource list/get/put/delete surgical loop (binary refusal, MEDIUM-family wiremock-first + live-capture gate) + assert_cmd #[ignore] e2e harness skeleton (PROJ-05)

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
**Plans**: 4 plans (sequential waves 1→4 — the rig family grows the cli.rs/main.rs choke files + client/mod.rs trait block, the Phase 1–3 pattern; no parallelism is honest)

*Planner refinement:* the flagged trial-reset spike is RESOLVED by 04-RESEARCH.md with live evidence — native Rust HTTP wins (the Playwright resetter needs Node+chromium, broke across 8.3.3's UI rewrite, and verifies via DOM text; the 8.3.6 login flow is a mapped OIDC challenge ladder with pure-JSON endpoints and per-call token rotation). Implementation is a LADDER: tier 0 = token-auth `POST /data/api/v1/trial` (one live call decides), tier 1 = native OIDC+CSRF flow in client/idp.rs, tier 2 = Playwright documented in README only, never shipped. Trial status sources `GET /data/api/v1/trial` (unauthenticated, richer than banners) with banners as cross-check — correcting the sketch's banners-primary assumption. Snapshot composition is honest redundancy: gwbk (streamed, `type=roaming`) + per-project exports + manifest.json (tag-provider bulk export stays Phase 5; gwbk captures tag config). Docker-only rig verbs (up/down/status/reset/logs) carry `profile: null` — the first non-gateway commands.

Plans:
- [x] 04-01-PLAN.md — compose engine (RigPlan resolve-then-act, ComposeRunner seam, LDJSON/array parsers, 5-level discovery, port pre-flight) + `rig up/down/status` with commissioned-wait (RIG-01 core)
- [x] 04-02-PLAN.md — `rig reset` (guarded `down -v --remove-orphans` cycle + volume preview) + `rig logs` passthrough streaming (RIG-01 completion, RIG-02 half)
- [x] 04-03-PLAN.md — `rig trial status|reset` (trial+banners wire, tier-0 spike then tier-1 OIDC ladder, ≥2-minor-version e2e gate) (RIG-02 half, RIG-03)
- [x] 04-04-PLAN.md — `rig snapshot|restore` (streaming gwbk + project exports + manifest; octet-stream restore + restart-wait + token-clobber warning; round-trip e2e) (RIG-04)

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
**Plans**: 6 plans + 2 gap-closure plans (wave 1 ran TWO plans in parallel — the first honest parallelism in the project: route sources touch no Rust choke file while the resource re-point touches no `webdev/` or `lib.rs`); waves 2–5 sequential (every plan grows the `client/mod.rs` trait/impl block + the ~10 inline test doubles + `cli.rs`/`main.rs`/`render.rs` choke files — the Phase 1–4 pattern). Gap closure: 05-07 (wave 1) then 05-08 (wave 2 — sequenced for live-rig exclusivity; no file overlap).

*Planner refinement:* all three flagged spikes are RESOLVED by 05-RESEARCH.md with live evidence (HIGH confidence, disposable 8.3.3 rig): deploy = project-zip import on Phase 3 machinery (routes embedded in the binary — no source-checkout dependency); scriptExec = deploy-time shared secret baked via template substitution (research's `secret.json` sibling-file idea was never runtime-proven readable — template-baking is the guaranteed mechanism, same threat model), fail-closed constant-time gate, deployed ONLY via `--with-script-exec` — planner locked secret-ONLY posture (`require-auth:false`: API tokens 401 on require-auth routes, so the Basic layer would lock the CLI's own calls out); tag history = structurally safe everywhere, InternalHistorian provisionable headlessly (the tag↔historian data-flow binding rides as a bounded ≤30min execution spike in 05-06 with the assert-and-document fallback pre-cleared). The Phase 3 `resource`-family blocker lands as its OWN plan (05-02, `gap_closure: true`): export/import zip surgery per the research's native-first negative verdict, `zip` crate 8.6 verified at planning (research said 6.x — corrected), e2e witnesses re-pointed; execution updates STATE.md to close the blocker. Bulk export/import formats resolved to JSON-native only (xml/csv were delegated format-discretion; research found JSON is the native interchange — round-trip live-proven — xml/csv deferred to backlog). Roadmap sketch renumbered: deploy moved 05-02→05-03 to sit after the resource plan (it reuses the zip dep), providers+browse/read/write kept together per sketch (05-04).

Plans:
- [x] 05-01-PLAN.md — webdev/ route sources (tags, tagConfig, alarms, tagHistory, scriptExec-template) with the versioned handshake + embedded bundle module in ignition-core (wave 1, parallel with 05-02)
- [x] 05-02-PLAN.md — resource family re-point: export→zip-member-surgery→import(overwrite), zip 8.6 dep, guarded put/delete, e2e witnesses re-pointed (closes the Phase 3 cross-phase blocker) (wave 1, parallel with 05-01)
- [x] 05-03-PLAN.md — webdev client seam (route_call + 405/402/401/200-denial probe + deploy zip builder) + `ign webdev deploy/status` + version refusal matrix + scriptExec secret lifecycle + doctor 405 re-pin (WEB-01/02)
- [x] 05-04-PLAN.md — tag providers (native REST, signature-chained delete) + browse filtered tree + single/batch read + write, with the version precondition every webdev command inherits (TAGS-01..04)
- [x] 05-05-PLAN.md — tag config CRUD (stringified-JSON re-parse) + UDT types/def + bulk export/import with abort-default collisions (TAGS-05/06/09)
- [x] 05-06-PLAN.md — alarms active/history(journal-gated)/ack(3-arg) + tag history query + InternalHistorian fixture + the binding spike + alarm lifecycle live gate (TAGS-07/08 — phase closer)
- [x] 05-07-PLAN.md — [GAP CLOSURE] import-denial seam (success:false on 200 → exit 6 `import_denied`, additive slug) + put-new landing via spike-verified surgery shape (parent-folder resource.json descriptors — dir-entry candidate disproven live) + live e2e_projects gate run (UAT gap 1)
- [x] 05-08-PLAN.md — [GAP CLOSURE] alarms view→ack loop: full event_id in the active table + short-prefix expansion in ack + route traceback surfaced in webdev_route_error (UAT gap 2)

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
**Plans**: 11 plans (6 sequential waves + 5 UAT gap-closure plans 07–11)

Plans:
- [x] 06-01-PLAN.md — Foundation: ratatui 0.30.2 + crossterm 0.29 deps, AppEvent select loop, AppState/update (Elm), profile→client context, UI chrome + modal infra, routes scaffold, logs::tail +Send fix, minimal `ign tui` arm
- [x] 06-02-PLAN.md — Dashboard screen (status/modules/metrics/sessions, 5s refresh worker) + global actions menu (version/wait/doctor/restart) + profile switcher with era-based worker re-targeting
- [x] 06-03-PLAN.md — Logs screen (tail worker, 10k ring, level filter, scrollback, loggers actions) + Alarms panel (5s poll, full UUIDs, history, username-required ack modal)
- [x] 06-04-PLAN.md — Tags screen: provider/tree browse, detail + read, live watch table (2s tags_read poll), write modal, providers/config/export/import actions
- [x] 06-05-PLAN.md — Projects screen: project list → detail → resource drill-down + project/resource/webdev action menus with CLI confirm-parity
- [x] 06-06-PLAN.md — Rig screen (status, guarded actions, raw logs pane) + complete routes registry + tui_coverage.rs CI proof (clap tree-walk, bidirectional) + README
- [x] 06-07-PLAN.md — Gap closure: 8.3.3 metrics decode (f64 gauges), designer-prune 409 → exit-6 session_not_prunable, contextual ign tui TTY hint
- [x] 06-08-PLAN.md — Gap closure: root-level resource put (project-root file member zip surgery, structure-pinned + live-rig round-trip)
- [x] 06-09-PLAN.md — Gap closure: Tags freshness — 'r' current-level refresh, write→detail refire trigger, error-pane recovery hints
- [x] 06-10-PLAN.md — Gap closure: modal geometry (fit + frame-clamp), vim motions in all modals, prose menu labels + noun-grouped Projects menu
- [x] 06-11-PLAN.md — Gap closure: rig status summary readability + README TUI keymap sync

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
**Plans**: 4 plans (sequential waves 1→4 — every plan grows the cli.rs/main.rs/routes.rs choke files, the Phase 1–4 pattern; no parallelism is honest)

*Planner refinement:* all seven research Open Questions resolved at planning — lint = doctor posture default (exit 0 when the child ran, findings + child_exit_code as data) + `--strict` literal passthrough; project diff = member-status only (zero new deps, no `similar`); tag-provider scope = `scope: "project"` metadata + README-documented tags-export pipe (no flag, no provider sync); script run input = all three forms (`--code`, `--file`, `--file -`); EAM create guard ladder = `eam_backup` unguarded (OnDemand default), restart/send\*/license types `--yes`, restore/install/upgrade REFUSED (additive `eam_task_type_refused` slug, EXT-03 pointer), any non-OnDemand schedule mode `--yes`; sidecar addressing = counter-named sidecars + JSON-pointer manifest with span-level splicing (byte-equality on unedited members is the acceptance test); EAM controller flip = README-documented manual config-resource PUT, no verb. New additive slugs this phase: `eam_not_controller`, `eam_task_type_refused`, `script_exec_not_configured`, `lint_tool_absent`. Every new verb lands its TUI routes row in the same plan (tui_coverage clap-walk gate).

Plans:
- [x] 07-01-PLAN.md — cross-gateway `project diff` (normalized member compare, B-relative-to-A, two-client resolution shape) + guarded `project sync` (A→B, --delete opt-in) (SYNC-01/02)
- [ ] 07-02-PLAN.md — standalone `ign backup download/restore` (--type param, 8th guarded verb) + EAM family (`eam history/tasks`, guarded `task new`/`task force`, `eam_not_controller` state-gate slug) (BKUP-01/02)
- [ ] 07-03-PLAN.md — `ign script run` over the shipped scriptExec route (structural opt-in, three input forms, `script_exec_not_configured` slug) (SCRPT-01)
- [ ] 07-04-PLAN.md — interop trio: Flint codec `--decode-scripts`/`--encode-scripts` round-trip, `ign lint` delegation (doctor posture + `--strict`), `tags browse --from-export` offline (INTR-01/02/03)

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6 → 7

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation & Agentic Contracts | 4/4 | Complete | 2026-08-21 |
| 2. Gateway Health & Inspection | 5/5 | Complete | 2026-08-22 |
| 3. Project Operations | 3/3 | Complete | 2026-08-22 |
| 4. Rig Lifecycle & Trial State | 4/4 | Complete | 2026-08-23 |
| 5. WebDev Backend & Tag Operations | 8/8 | Complete | 2026-08-26 |
| 6. TUI Cockpit | 11/11 | Complete | 2026-08-28 |
| 7. Ecosystem Interop & Advanced Ops | 4/4 | Complete | 2026-08-29 |
