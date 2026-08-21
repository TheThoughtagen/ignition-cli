# Project Research Summary

**Project:** ignition-cli — Rust CLI + ratatui TUI cockpit for Ignition 8.3+ gateways
**Domain:** Developer/ops tooling wrapping Ignition Gateway REST + WebDev APIs with Docker test-rig control (greenfield, ecosystem-completing)
**Researched:** 2026-08-20
**Confidence:** HIGH (stack versions verified against crates.io API + official changelogs 2026-08-21; domain pitfalls verified against official IA docs, IA forum engineer guidance, and the author's own working code)

## Executive Summary

This is a greenfield Rust CLI (`ign`) that replaces the author's own ignition-mcp (37-tool Python MCP server) as the canonical agent + human interface to Ignition 8.3+ gateways. The external Ignition-CLI landscape is thin — only `igw-cli` (Go) overlaps meaningfully, and it has no TUI, no rig control, no WebDev runtime ops — so "table stakes" is defined by the ignition-mcp catalog being replaced plus standard DevOps-CLI conventions (kubectl/gh patterns: `--json`, exit codes, profiles, `wait`, `doctor`, `--yes` mutation guards). The critical domain fact shaping everything: native 8.3 REST covers config/CRUD/health/backups but **cannot** read/write runtime tag values, query/ack alarms, query history, or execute scripts — those require WebDev routes this repo ships, versions, and deploys itself (`ign webdev deploy`). That deploy capability gates the entire tag-runtime feature block and must be built with it, not after it.

The recommended approach is a deliberately boring, all-batteries-included stack — clap 4.6 + tokio 1.53 + reqwest 0.13 + ratatui 0.30 in a three-crate workspace (`ignition-cli` bin / `ignition-core` lib / `ignition-tui` lib + `webdev/` route sources), TOML profiles with keyring/env secrets, thiserror-driven JSON error envelopes, and `docker compose` driven by subprocess (bollard rejected: no compose support exists, and reimplementing Compose semantics is a verified trap). The architectural invariant that makes the "TUI = full cockpit" constraint structural rather than aspirational: **CLI handlers and TUI both call a shared actions layer; neither talks to the client directly.**

Key risks, all with concrete preventions from prior art: API-token auth is a three-part setup whose failures surface as bare 403s (needs `doctor` from day one); project import/export are *synchronous* with no job IDs (needs per-operation-class timeouts, not async-job machinery); the TUI event loop must never block on gateway I/O (command-layer separation from the skeleton phase); rig resets orphan volumes and collide on ports if compose semantics aren't respected (`down --remove-orphans -v`, explicit project names, port pre-flight); and trial reset has no REST endpoint — it's an auth+CSRF web flow where the three researchers diverge on delegate-vs-reimplement (flagged spike).

## Key Findings

### Recommended Stack (from STACK.md)

One sentence: clap 4 + tokio 1 + reqwest 0.13 + ratatui 0.30 in a three-crate workspace, plain TOML config with keyring-stored secrets, thiserror-driven JSON error reporting, and `docker compose` via subprocess — no frameworks, lean tree. All versions verified live against crates.io/GitHub changelogs on 2026-08-21.

- **Cargo workspace (3 crates + `webdev/` dir), Edition 2024, MSRV 1.85** — `ignition-core` is the seam both front-ends share; feature-gated TUI keeps headless/agent builds lean; WebDev route sources versioned with the CLI (Key Decision: own routes).
- **clap 4.6 (derive) + clap_complete 4.6** — 1:1 subcommand mapping, global `--json`/`--profile` args, usage-error exit code 2 for free; the Rust CLI standard.
- **tokio 1.53** (rt-multi-thread, process, signal) — one runtime for HTTP, `docker compose` subprocess streaming, and the TUI `select!` loop.
- **reqwest 0.13** (rustls default, `json` feature) — ⚠ 0.13 breaking changes verified: rustls is now default TLS, `query`/`form` feature-gated; per-profile `ssl_verify = false` via `danger_accept_invalid_certs` for dev rigs only. **Auth correction: header-based (`X-Ignition-API-Token` + Basic fallback), NOT session/cookie as PROJECT.md phrased it — keep the `cookies` feature OUT until a live gateway proves otherwise.**
- **ratatui 0.30** (umbrella crate → ratatui-crossterm 0.1.2 → crossterm 0.29) — 0.30 is the modularized "biggest release"; official event-driven-async template is the TUI pattern; access crossterm only via ratatui's re-export.
- **Shell out to `docker compose` (v2 plugin), no bollard** — `bollard-compose` verified non-existent; rigs are compose files (git-module pattern source) and `docker compose ps --format json` gives structured status. bollard deferred to a possible v2 for daemon-level introspection.
- **Hand-rolled profiles: `toml` 1.1 + `serde` + `directories` 6.0** — profiles are ~100 lines of typed TOML + env overlay; `config`/`figment` rejected as heavy/stale.
- **`keyring` 4.1 (default `v1` Entry API) + env fallback** — v4.1 (Jun 2026) restored the classic API; secrets never in TOML. MEDIUM confidence — smoke-test on Linux CI in Phase 1.
- **`thiserror` 2.0 + `std::process::ExitCode`** — typed errors feed the JSON error envelope; stable exit-code contract: 0 ok / 1 runtime / 2 usage / 3 connection / 4 auth / 5 gateway-state / 6 docker-rig.
- **Testing: `wiremock` 0.6.5 + `assert_cmd` + `predicates` + `tempfile`** — mock gateway `/data` + `/system/webdev` endpoints incl. HTML error bodies; golden-file JSON-shape tests per subcommand; rig integration tests `#[ignore]`-gated behind Docker.

Rejected-with-reasons highlights: `tuirealm` (framework creep — explicit project constraint), `anyhow`-as-primary (fights the stable JSON error-slug contract), `exitcode` crate (dead since 2017), `miette`/`color-eyre` (conflicts with JSON envelope), `chrono` (prefer `jiff` if ever needed — don't add in v1).

### Expected Features (from FEATURES.md)

**Table stakes — the ignition-mcp 37-tool replacement bar (must be in v1):**
- **Gateway health/inspection (A1–A11):** status/info, modules, logs (list/fetch/tail/download), logger levels, DB + OPC connections, system metrics (exceed mcp via 8.3 system-performance/thread-diagnostics), connected sessions (designers + Perspective + Vision, incl. terminate), restart + restart-task status, `doctor` (connectivity/auth/write-permission/WebDev-presence probe — igw-cli's best idea), and `wait` primitives (gateway up, restart complete).
- **Projects (B1–B5):** list (+inheritance), CRUD, export/import (foundation for everything downstream), resource-level get/put/delete ("edit one view without re-importing everything"), collision policies (abort/overwrite/merge — match git-module convention, default Abort).
- **Tags (C1–C9):** providers, browse (native REST) — then read/write values, tag-config CRUD, UDT defs, alarms (active/history/ack), history queries (all **WebDev**, gated on F6 deploy); bulk tag provider export/import (native, collision-policy Abort).
- **Profiles/config (D1–D4):** multi-gateway profiles with visible profile name in every output (prod-vs-dev misfire is THE classic accident), token+basic auth from env/keyring, `IGNITION_*` env overrides for everything.
- **Agentic conventions (E1–E7) — day one, cross-cutting:** `--json` on EVERY subcommand with stable field names, documented exit-code taxonomy, JSON error envelope with code/message/endpoint/hint (e.g., "WebDev route missing → run ign webdev deploy"), non-interactive by default + `--yes` for destructive ops, shell completions, version check refusing <8.3.1 cleanly, human tables + `--compact`.

**Differentiators (the reason this becomes the daily driver):**
- **F1 Rig lifecycle** — `rig up/down/status/reset/logs` over git-module compose conventions (env names `GATEWAY_ADMIN_USERNAME/PASSWORD`, ports 9088/9043), replacing a terminal of compose incantations. Highest-value differentiator.
- **F6 Shipped + versioned WebDev backend** — `ign webdev deploy/status` installs the CLI's own routes; route-version negotiation; removes ignition-mcp's #1 setup failure. Gates C3–C8, F7, F10.
- **F4 TUI cockpit** — k9s-for-Ignition (nothing like it exists in the ecosystem): live dashboard, log tail, tag browser + live watch (F10), alarm panel, project browser, profile switcher; object-list→detail navigation, not menu-tree.
- **F3 Snapshot/restore** — gwbk via native `gateway-backups` API + project/tag exports; repeatable e2e fixture states (WHK-Global consumer).
- **F8 Ecosystem interop** — read/delegate/drive, never re-implement: script decode/encode on export/import (nvim loop), `ign lint` delegation wrapper, tag-export browsing, e2e driver mode for WHK-Global Playwright specs.

**Defer to v2+:** F5 cross-gateway sync/diff (build on B3/B4 after scope semantics proven), F7 script exec ships opt-in/flag-day, F9 EAM writes (reads maybe), MCP server transport (JSON contract stays stable enough to wrap later), F2 trial reset *mechanism* decision can lag `rig trial status`.

**Anti-features (explicit):** LSP/completions (ignition-nvim), lint engine (ignition-lint — delegate only), Designer git (ignition-git-module), 8.1.x support, MCP serving in v1, view editing, OpenAPI-discovery subsystem (thin `ign api call` escape hatch instead), daemon/background service, web UI.

### Architecture Approach (from ARCHITECTURE.md)

Single `ign` binary over a three-crate workspace; strict layering: **UI/CLI → shared actions → client/rig → gateway**, results flow back as typed serde models, no component reaches around the actions layer.

1. **`ignition-core::client` (GatewayClient)** — all HTTP: auth (token header preferred, Basic fallback), base URL per profile, per-operation-class retries/timeouts, error mapping; native REST (`/data/api/v1/*`) + WebDev (`/system/webdev/{project}/{route}` → `Global/IgnitionCLI/*`); coarse `trait GatewayApi` for mock injection (the same seam the TUI uses).
2. **`ignition-core::actions`** — the verb layer (`health`, `export_project`, `read_tags`, `rig_up`, `webdev_deploy`, …); **invariant: CLI and TUI both call actions, never the client directly** — this is what makes "every CLI action in TUI" cheap.
3. **`ignition-core::rig`** — compose shell-out with 5-level rig discovery (flag → env/config → cwd compose → git-module docker dir → WHK-Global); trial reset as a *delegation boundary* (subprocess, not embedded browser).
4. **`ignition-tui`** — official ratatui event-driven-async template: one `AppEvent` enum (Tick/Key/Resize/`ActionCompleted(Result)`), mpsc channel, `tokio::select!` loop, pure `AppState` + render functions, `TestBackend`-testable.
5. **`webdev/`** — importable Ignition project fragment (`resources/IgnitionCLI/{tags,tagConfig,alarms,scriptExec,tagHistory}/doPost.py`); every route response carries `cliVersion`/contract version for handshake; deploy via REST project-import (mechanism = spike).
6. **Output/render layer** — presentation never inside actions; `--json` serializes action models zero-copy; exit codes as agent contract.

Testing: 3 layers — unit (mock trait), wiremock HTTP contract (real URL shapes, HTML error bodies, auth headers), and rig-based integration that dogfoods the binary itself (`ign rig up → webdev deploy → tags read --json → rig down -v`), `#[ignore]`-gated for CI.

### Critical Pitfalls (from PITFALLS.md)

1. **API-token auth is a three-part setup** (write-level security level defined + assigned to Gateway Write Permissions + token carries the level; plus "Require secure connections for API Keys" rejecting plain-HTTP tokens on localhost) — failures return bare 401/403 with no explanation. **Prevention:** `doctor`/`auth check` from day one translating 401/403 into the three concrete causes; document setup in `profile add` output. *(Phase 1–2.)*
2. **Project import/export are SYNCHRONOUS** — no job IDs exist; the trap is the inverse of the presumed one: long imports block the HTTP connection and a default timeout kills them mid-flight with unknown state. **Prevention:** per-operation-class timeouts (fast reads 10s / import-export minutes-or-disabled / client slightly longer than gateway exec timeout), stream export ZIPs to disk, treat timeout as "verify with `project list`". *(Phase: projects; timeout policy in skeleton.)*
3. **Blocking the TUI event loop with synchronous gateway calls** freezes the UI and looks like a hang. **Prevention:** actions-on-worker-tasks + mpsc from day one; the command-layer separation enabling this must exist in the skeleton phase, not be retrofitted. *(Phase: TUI, enabled by Phase 1 architecture.)*
4. **Orphaned volumes/containers and port collisions on rigs** — `down && up` without `-v` leaves stale trial state ("reset didn't work"); implicit directory-name compose projects collide; two rigs fight over 8088/9043 and commands silently target the wrong gateway. **Prevention:** `down --remove-orphans -v` on reset, explicit `-p` project names stored in rig profile, port pre-flight on `up` ("port 8088 in use by container X (rig Y)"), profile-URL-vs-rig-mapping cross-check. *(Phase: rig.)*
5. **Projects ≠ gateways** — tag providers/config live outside project export ZIPs; a naive "sync projects" delivers a gateway that looks synced but has no tags. **Prevention:** name scope explicitly in command semantics and `--json` metadata (`includes`/`excludes`); separate `sync`/`snapshot` concept composing project + tag-provider config. *(Phase: projects design decision.)*
6. **WebDev routes are a bespoke contract you own** — prior art shows stringified values, drifting shapes, errors-as-200, Jython 2.7 quirks. **Prevention:** versioned route contract (`contractVersion` handshake, CLI refuses mismatched), type normalization in serde, non-2xx for real errors, `webdev deploy` as first-class bootstrap. *(Phase: WebDev.)*
7. **Trial reset is an auth+CSRF web flow, not an endpoint** — and browser automation broke across 8.3.3's UI rewrite. **Prevention:** banners endpoint (`GET /data/api/v1/trial` state via `/data/api/v1/overview/banners`) as free `rig trial status`; version-slew-test any reset path. *(Phase: rig — mechanism is a flagged spike.)*

Cross-cutting agentic discipline (pitfalls 4.1–4.5): one JSON envelope everywhere, prompts gated on TTY with `--yes`, exit-code taxonomy CI-tested, secret redaction in logs and rig JSON (allowlist, never raw `docker inspect`), progress-to-stderr with parseable stage markers + graceful SIGINT ("compose up completed, gateway still starting").

## Implications for Roadmap

### Convergence across all four researchers

- **Skeleton/profiles/auth/contracts first** — FEATURES ("every later feature sits on them"), ARCHITECTURE (build order 1), PITFALLS (phase-mapping row 1 piles 4.1–4.4, 1.1, 1.2, 1.10, 6.1 here).
- **Rig before WebDev** — ARCHITECTURE is explicit ("the rig is the test fixture for everything downstream, and it dogfoods `rig up`/`reset` in CI"); PITFALLS 6.2 requires rig + webdev-deploy-in-suite-setup for e2e; FEATURES agrees the tracks are independent but WebDev needs a test gateway.
- **TUI last** — all three: it consumes the completed action surface; building early forces rework (FEATURES), full coverage lands last (ARCHITECTURE), and the Elm/TestBackend pattern must be in the phase plan (PITFALLS 2.3).
- **WebDev deploy (F6) is fused with tag runtime ops** — F6 gates C3–C8/F7/F10; ship routes + deploy + first tag commands together (FEATURES dependency graph + PITFALLS 1.8 chicken-and-egg note).

### Divergence to resolve

- **Project-ops placement:** FEATURES MVP puts projects 3rd (daily value, native REST, webpage replacement); ARCHITECTURE puts them 5th (after WebDev). **Opinionated call: follow FEATURES — projects at Phase 3.** They need only the matured client from Phase 2, carry no gateway-side setup risk, exercise mutations + the JSON contract before the riskier rig/WebDev work, and deliver the first webpage-replacement value. Both orderings satisfy the hard constraint (rig before WebDev).
- **Trial-reset mechanism:** ARCHITECTURE/FEATURES lean delegate-to-existing-Playwright-script (cheapest credible, "do not embed a browser engine in Rust"); PITFALLS 1.9 leans native headless HTTP flow (login → session+CSRF from `/data/app/session` → `POST /data/api/v1/trial` → verify via banners) with Playwright as fallback. **Flag as a Phase 4 spike; ship `rig trial status` (banners, free) regardless; reset can follow.**
- **`ignition-mcp` tool count:** PROJECT.md/FEATURES say 37 tools; ARCHITECTURE cites the `ignition_tools_summary.json` artifact at 42. Treat the JSON artifact as the authoritative parity checklist when the phase lands.

### Suggested phases

**Phase 1: Workspace skeleton, profiles, auth & agentic contracts**
- **Rationale:** Everything depends on it; agentic discipline (envelope, exit codes, TTY rules) must exist before the third command, enforced by CI from then on (PITFALLS 4.1–4.4).
- **Delivers:** 3-crate workspace, clap global flags (`--json`, `--profile`, `--verbose`), config.toml + profiles + keyring/env secret resolution (0600, redaction), JSON envelope + error taxonomy + exit-code contract (golden-file tested), tracing (stderr CLI / file TUI), `GatewayApi` trait seam + wiremock harness, `version` + completions.
- **Addresses:** D1–D4, E1–E7. **Avoids:** 1.1/1.2 foundations (doctor prep), 3.6, 4.1–4.4, 1.10, 6.1.

**Phase 2: Gateway client & read-only inspection**
- **Rationale:** Immediately useful against any 8.3 gateway with zero gateway-side setup; validates auth/error mapping/JSON contract against reality; produces the `wait_until_ready` primitive everything reuses.
- **Delivers:** GatewayClient (header auth, per-class timeouts, content-type sniffing for HTML error bodies), `status/info/modules/logs(+tail,level)/db/opc/metrics/sessions`, `restart --wait` (multi-stage RUNNING poll, `pendingRestart` in status), full `doctor` (three-part-auth diagnosis + WebDev probe + rig detection), `wait` primitives, `api call` escape hatch.
- **Addresses:** A1–A11. **Avoids:** 1.1, 1.2, 1.7, 4.4, 4.5.

**Phase 3: Project operations**
- **Rationale:** Native REST only, no rig/WebDev dependency — the webpage-replacement milestone; first mutating commands prove `--yes`/collision-policy conventions.
- **Delivers:** `project list/new/cp/mv/rm`, export/import (streaming ZIP, timeout override, idempotent-retry guidance, `overwrite` semantics), resource `ls/get/put/rm`, collision policies, scope metadata (`includes`/`excludes` — no tag providers), first e2e harness skeleton.
- **Addresses:** B1–B5 (+F8 round-trip hooks later). **Avoids:** 1.3, 1.5, 5.1/5.2 boundaries, 6.2 harness rules.

**Phase 4: Rig lifecycle & trial state**
- **Rationale:** Before WebDev because the rig is the self-managed test fixture WebDev e2e needs; dogfoods `rig up/reset` in CI from here on.
- **Delivers:** compose shell-out (v2 version check, explicit `-p` project names, `--remove-orphans`), 5-level rig discovery, `up/down/status/reset/logs` (port pre-flight, volume-teardown explicitness, scan-config/projects + `wait_until_ready` after any file-level op), snapshot/restore (F3, native gwbk API — stretch, may slip to Phase 7), `trial status` (banners) + trial-reset via the spike-chosen mechanism.
- **Addresses:** F1, F3 (stretch), F2-partial (status now, reset after spike). **Avoids:** 3.1–3.6, 1.9, 1.6, 4.5.

**Phase 5: WebDev backend, deploy & tag runtime ops — the ignition-mcp replacement bar**
- **Rationale:** F6 gates this whole block; with rig (Phase 4) as fixture, routes can be deployed by the CLI under test, killing the "manually deployed once" dependency.
- **Delivers:** `webdev/` routes (tags, tagConfig, alarms, scriptExec, tagHistory) with versioned contract + `cliVersion` handshake, `webdev deploy/status` (post-spike), serde normalization for stringified values, `tag read/write/browse/config/udt/history`, `alarm active/history/ack`, `script run` (opt-in, guarded), signature-aware config resource read-modify-write.
- **Addresses:** C3–C8, F6, F7. **Avoids:** 1.8, 1.4, 6.2 (webdev-deploy-in-suite-setup).

**Phase 6: TUI cockpit**
- **Rationale:** Consumes the now-complete action surface; command-layer separation (Phase 1) makes full coverage structural — a CI test can assert every CLI subcommand has a TUI action mapping.
- **Delivers:** event-driven-async loop (AppEvent mpsc, `ratatui::init()`/restore panic hook), health dashboard, log tail with level filter, tag browser + live watch (F10, poll-based), alarm panel, project/resource browser, profile switcher; Elm split + TestBackend snapshot tests.
- **Addresses:** F4, F10. **Avoids:** 2.1–2.4, 6.3.

**Phase 7: Ecosystem interop & polish**
- **Rationale:** Differentiators that ride the finished core; each is delegation/convention reuse, low architectural risk.
- **Delivers:** `lint` delegation, script decode/encode on export/import (nvim loop), tag-export browsing (`--from-export`), F3 if slipped, F5 cross-gateway diff (explicit project-vs-tag-provider scope), curated backup/EAM reads (F9-read), e2e driver conveniences for WHK-Global.
- **Addresses:** F8, F5, F9-read. **Avoids:** 5.1–5.5 scope traps (explicit delegation boundaries).

### Phase Ordering Rationale

- Dependency graph from FEATURES: profiles+JSON core → everything; WebDev deploy → all WebDev ops; tag browse → read → watch; export → resource ops → diff; rig up → trial reset; native gwbk → snapshot.
- Architecture layering: client (2) must precede actions that use it; rig (4) must precede WebDev e2e (5); TUI (6) last to avoid rework against a moving action surface.
- Pitfall phase-mapping table aligns almost 1:1 with this ordering (its rows: skeleton / health / projects / tag-ops+WebDev / rig+trial / TUI — only the projects↔rig swap differs, resolved above toward earlier value).

### Research Flags

**Needs `/gsd-research-phase` or an explicit spike during planning:**
- **Phase 4 (trial reset):** genuine researcher divergence — delegate to Node/Playwright resetter (ARCHITECTURE/FEATURES: cheapest, proven) vs native headless HTTP login+CSRF flow in Rust (PITFALLS: robust against UI rewrites, no Node dep). Verify against ≥2 gateway minor versions.
- **Phase 5 (WebDev deploy mechanism):** per-resource import vs full project-zip import via 8.3 REST — endpoints exist in 83-api but minimal-payload form unverified (ARCHITECTURE open question 1). Also: script-exec security posture (dedicated role vs admin-only) and tag-history route availability on default rigs (historian enabled?).
- **Phase 2 (auth verification):** live-gateway check that token-header auth works on all `/data` + `/webdev` endpoints and whether Basic fallback is viable on 8.3.1 — STACK corrected PROJECT.md's "session/cookie" phrasing based on the reference impl; needs empirical confirmation (three-part write setup per PITFALLS 1.1).

**Standard patterns, skip research-phase:**
- **Phase 1 (workspace/clap/config):** exhaustively documented Rust-CLI territory; versions verified.
- **Phase 6 (TUI):** ratatui official templates verified (event-driven-async + component + TestBackend); the pattern is prescribed, not open.
- **Phase 3 (projects):** endpoint shapes verified in 83-api (675 requests); Bruno `.bru` files double as wiremock fixtures.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Every version verified via crates.io API + official changelogs 2026-08-21; sub-items MEDIUM: keyring 4.1 default-store (smoke-test Phase 1), indicatif (deferred), tracing-appender TUI mode. One evidence-backed correction (header auth, no cookies). |
| Features | HIGH | Bar defined by the author's own ignition-mcp catalog + local ecosystem sources (all primary). External landscape (igw-cli HIGH — README fetched; kindling/igniscope/agent-tools MEDIUM) is low-stakes: landscape is thin, competitors don't change the bar. |
| Architecture | HIGH | Patterns verified against ratatui official templates + local source (ignition-mcp client, WHK-Global WebDev layout, git-module docker conventions). Open questions are enumerated and spike-scoped, not unresolved design risk. |
| Pitfalls | HIGH | Majority verified against primary sources: official 8.3 docs, IA forum engineer walkthrough, local code that already hit them (reset-trial.mjs breakage comments, gateway.mjs TLS lesson). MEDIUM only: restart timing numbers, TUI-scope judgment. |

**Overall confidence: HIGH** — unusually strong for greenfield research because the domain's hardest lessons are already encoded in the author's own prior art, and the external Ignition CLI space is thin enough that this tool defines its own table stakes.

### Gaps to Address

- **WebDev deploy mechanism** — per-resource vs zip import; spike at Phase 5 planning (ARCHITECTURE OQ-1).
- **Trial-reset path** — delegate vs native HTTP; spike at Phase 4 planning (researcher divergence).
- **Live-gateway auth verification** — token header across all endpoints, Basic viability, three-part write setup on 8.3.1; resolve empirically Phase 2 (STACK gap 1, PITFALLS 1.1).
- **keyring 4.1 on headless Linux CI** — smoke-test in Phase 1; keep keyring paths out of default CI (STACK gap 3).
- **Import/export transport detail** — multipart vs plain body (enable reqwest feature when known); settle in Phase 3 (STACK gap 2).
- **Parity checklist count** — 37 (PROJECT/FEATURES) vs 42 (`ignition_tools_summary.json`); use the JSON artifact as authoritative in Phase 5.
- **TUI refresh cadence for prod profiles** — poll-on-tick vs on-demand; decide in Phase 6 design (ARCHITECTURE OQ-5).

## Sources

### Primary (HIGH confidence)
- **Local author ecosystem (read directly):** `.planning/PROJECT.md`; `ignition-mcp` (readme, `ignition_client.py`, `config.py`, `docs/webdev-setup.md`, tool-summary JSON — auth shape, WebDev URL shape, 37/42-tool catalog, env matrix); `ignition-git-module` (readme, `docker/` compose + `.env` + gw-build/gw-init/gw-secrets, test-rig — rig conventions, ports 9088/9043, commissioning, `tags_importOnStartup`); `WHK-Global` (`e2e/reset_trial.mjs`, `e2e/lib/gateway.mjs`, `com.inductiveautomation.webdev/resources/` — route file layout, scoped-TLS lesson, defensive parsing); `ignition-trial-resetter` (readme, `reset-trial.mjs`, `instances/` — no-REST-for-trial-reset, CSRF/session flow, 8.3.3 UI-rewrite breakage); `ignition-nvim` + `ignition-lint` readmes (interop boundaries); `83-api` Bruno collection (~100 endpoint families, 675 requests — native surface: gateway-backups, eam-tasks, restart-tasks, perspective-sessions terminate, running-scripts diagnostics-only).
- **Official docs/changelogs (fetched 2026-08-20/21):** crates.io API (all crate versions); reqwest 0.13 CHANGELOG (rustls default, feature gates); ratatui repo CHANGELOG + ratatui/templates repo (0.30 modularization, event-driven-async template, TestBackend); toml-rs CHANGELOG (1.0/1.1, MSRV 1.85); keyring-rs GitHub releases (v4.1 Entry-API restoration); Docker docs (compose ps `--format json`, Compose v1-EOL/Spec history); docs.inductiveautomation.com 8.3 API page (token header, signature DELETE, audit logging); IA Forum "Ignition 8.3 API Usage Guide" (three-part auth setup, CSRF session flow, scan routes, HTML error bodies).
- **Context7:** `/websites/rs_clap` (derive/global-args/exit codes), ratatui 0.30 docs (init/restore panic hook, blocking `event::read`, TestBackend), bollard docs (Engine-API-only — informed the avoidance decision).

### Secondary (MEDIUM confidence)
- `igw-cli` README (fetched raw — command set, doctor, wait, profiles, JSON/exit-code discipline, `--yes` guard; the one meaningful competitor). kindling / igniscope / ignition-agent-tools repo metadata (README fetches 404'd or description-only — landscape context only).

### Tertiary (LOW confidence)
- Landscape negatives ("nobody combines these") rest on absence-of-evidence in a thin ecosystem — recheck igw-cli's trajectory before v1 ships. keyring 4.1 default-store behavior on Linux (release-notes-verified, not compiled against).

---
*Research completed: 2026-08-20 (external verification 2026-08-21)*
*Ready for roadmap: yes*
