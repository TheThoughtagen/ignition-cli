# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-20)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Phase 2 — Gateway Health & Inspection (Phase 1 complete & verified)

## Current Position

**Phase:** 2 of 7 (Gateway Health & Inspection)
**Current Plan:** 4
**Total Plans in Phase:** 5
**Status:** Ready to execute
**Last Activity:** 2026-08-22

**Progress:** [█████████░] 89%

## Performance Metrics

**Velocity:**
- Total plans completed: 5
- Total execution time: 132min

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 01 P01 | 8min | 3 tasks | 13 files |
| Phase 01 P02 | 53min | 3 tasks | 9 files |
| Phase 01 P03 | 15min | 3 tasks | 15 files |
| Phase 01 P04 | 37min | 3 tasks | 18 files |
| Phase 02 P01 | 19min | 3 tasks | 14 files |

*Updated after each plan completion*
| Phase 02 P01 | 19min | 3 tasks | 14 files |
| Phase 02 P02 | 12min | 3 tasks | 14 files |
| Phase Phase 02 PP03 | 14min | 3 tasks | 15 files |
| Phase 02 P04 | 35min | 3 tasks | 18 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Roadmap: 7-phase order follows research convergence — contracts → inspection → projects → rig → WebDev/tags → TUI → interop (rig before WebDev: rig is the self-managed fixture; TUI last: consumes finished action surface)
- Roadmap: `ign script run` command surfaces in Phase 7 per orchestrator call (scriptExec route ships in Phase 5's webdev/ sources; security posture is a Phase 5 spike)
- [Phase 01]: MSRV locked at 1.88 (keyring 4.1.x floor), correcting STACK.md's 1.85
- [Phase 01]: Workspace shape final from commit one: three crates (ignition-cli bin 'ign' / ignition-core lib / ignition-tui zero-dep stub), tui feature gate default-on, no Windows CI
- [Phase 01]: CLI contract chassis: five global clap args defined once, main() -> ExitCode single exit point, env defaults in exactly one function, stderr-only diagnostics
- [Phase 01]: Edition 2024 let-chains in apply_env_defaults keep clippy -D warnings clean without allows
- [Phase 01]: [Phase 01]: Agentic output contract FROZEN — envelope exactly {ok,profile,data}/{ok,profile,error}, exit taxonomy 1-7 with stable slugs, errors-on-stderr in all modes; changing shape/slugs/codes is a breaking change for agents
- [Phase 01]: [Phase 01]: Exit-code table lives in exactly two places (CoreError::exit_code() + README), enforced by the enumerated unit test and snapbox goldens; --compact implies --json, resolved once in RenderMode::resolve
- [Phase 01]: [Phase 01]: snapbox inline goldens: str! trims leading+trailing newlines, Cow<str> lacks IntoData — use stdout_for_golden (strip println's single trailing newline) and pass &str; isolate IGNITION_CLI_CONFIG per spawn; [..]-elide dynamic values
- [Phase 01]: Secret chain encodes the LOCKED order (env tokens → keyring → USER/PASSWORD) — BasicEnvStore is separate so keyring comes before basic; KeyringStore resolve fails soft, set/delete surface errors
- [Phase 01]: profile add skips pre-resolution (--profile naming a NEW profile must not fail); add's envelope echoes post-add active state; config save re-asserts 0600 on overwrite
- [Phase 01]: URL trailing-slash normalization pinned in goldens (typed url::Url beats storing strings); ActionOutput::render_json is monomorphic per-variant (Serialize not dyn-compatible)
- [Phase 01]: [Phase 01]: GatewayApi locked on async_trait, ONE coarse method (gateway_info) — Phase 2 grows it by capability; auth headers token-XOR-basic enforced by a match, Secret::expose() confined to the single header-construction site
- [Phase 01]: [Phase 01]: version behavior matrix LOCKED — unreachable gateway → exit 0 + warning inside data (never a top-level field); refusal (exit 6) only when the gateway ANSWERED <8.3.1/unparseable; SecretUnavailable degrades to header-less via resolve_secret_opt, never blocks version
- [Phase 01]: [Phase 01]: below_minimum compares against plain semver 8.3.1 (research sketch's 8.3.1.0 constant cannot parse — semver is strict); GatewayInfo carries a serde(skip) endpoint field so action-built GatewayTooOld populates CORE-05
- [Phase 01]: completions print RAW to stdout regardless of --json (the one sanctioned success-path exception, README-documented) and dispatch before config load; require_confirmation guard (exit 2, hint names --yes + IGNITION_YES=1) pinned in main.rs, #[cfg_attr(not(test), expect(dead_code))] until Phase 3's first destructive caller
- [Phase 01 CI bring-up]: #[expect(dead_code)] on test-exercised fns is unfulfilled in test-target clippy compiles → gate with #[cfg_attr(not(test), expect(...))]; keyring store-unavailable is debug (expected headless condition — warn noise preceded JSON envelopes on stderr); stderr-envelope tests parse from first '{' (log-tolerant)
- [Phase 02]: [Phase 02-01]: GatewayInfo serializes under gateway-native camelCase keys (rename=ignitionVersion + alias=version) — passthrough-shaped --json data; state/uptime dropped (not on the real payload; 02-02 sources them from /overview + /StatusPing)
- [Phase 02]: [Phase 02-01]: classify() is the single status→content-type→redirect mapping site running before every .json(); redirect(Policy::none()) pinned so uncommissioned 302s can never masquerade as 200; Basic demoted with a loud per-call warning (dead on 8.3 /data)
- [Phase 02]: [Phase 02-01]: three ADDITIVE exit-6 slugs (gateway_not_commissioned, gateway_restarting, not_found) + status-aware 401/403 auth hints; wiremock gotchas recorded: set_body_string forces text/plain (use set_body_raw), scoped MockGuard drop unmounts fixtures
- [Phase 02]: [Phase 02-01]: gateway-info REQUIRES auth under 8.3 default security (header-less → 401 Jetty HTML, re-verified live on the research rig) — 83-api's auth:none tag does not hold; live suite is skip-by-default green no-op
- [Phase 02]: [Phase 02-02]: Two-layer naming LOCKED — client models stay wire-faithful (gateway-native camelCase renames + flatten passthrough); the status action re-exposes selected fields under unit-explicit keys (uptime_ms, cpu_fraction, trial_remaining_s); overview cpu 0-1 fraction vs gauges percent documented at both fields, never converted
- [Phase 02]: [Phase 02-02]: Inspection commands (status/modules/metrics) REQUIRE a credential — resolve_secret → SecretUnavailable exit 3 (inverse of version's header-less degradation); LOCKED secret chain now built in exactly one place (secret_chain() in main.rs); status = read of a HEALTHY gateway, failed sub-calls exit per taxonomy
- [Phase 02]: [Phase 02-02]: ModuleInfo tolerates the quarantined reduced shape (state/licenseState/vendorName/startupTime Option per openapi — fully-loaded-only; startup_time is a String on the wire); PerformanceCharts parses the nested memoryChartDatapoints wire shape into a flat model; /StatusPing fetched header-less via auth=false (wiremock header-absence proof — the 02-05 wait anchor)
- [Phase Phase 02]: [Phase 02-03]: require_confirmation dead-code gate REMOVED in Phase 2/02-03 (sessions terminate = first destructive caller) — earlier than the logged 'until Phase 3'; the attribute's own reason string mandates removal at the first real caller. Guard fires BEFORE profile/secret/client resolution: refusal = exit 2 with null profile, zero config/network work; usage-class errors lead — Plan key_link + must_have truth #2; clippy -D warnings clean with the gate gone is the proof
- [Phase Phase 02]: [Phase 02-03]: sessions --type rides the SessionsArgs top level and terminate's id is --id <ID> — LOCKED must_have truths overrode the plan's nested-List/positional-id sketch; JSON data always carries ALL family keys (filtered-out = [], endpoints never called) — the stable agent shape all filtered list commands inherit — must_haves are the user contract; agents must never key-hunt
- [Phase Phase 02]: [Phase 02-03]: Perspective path discipline contract-pinned by recorded-request proofs — GET /data/perspective/api/v1/sessions/ EXACT trailing slash (Pitfall 8), DELETE no-trailing-slash with sessionId/message as QUERY params + empty body; connections ride resources/list/ignition/{database,opc}-connection with healthchecks RAW passthrough (LOW-confidence until live capture — live_connections hook + UAT open question) — Wire subtleties asserted on the REQUEST, not just response parsing
- [Phase 02-04]: [Phase 02-04]: CoreError::Network.source → Option<reqwest::Error> — source:None marks a poll deadline expiry (same network_error slug/exit 4, NO new variant per plan); the deadline message rides url (subject + waited + last observation); Some-source Display byte-identical, no golden moved
- [Phase 02-04]: [Phase 02-04]: poll.rs is THE wait engine (HRTB state-threading: for<'a> FnMut(&'a mut S) -> Probe<'a,T>) — ×1.5 backoff clamp [interval,30s], Network/GatewayRestarting retried, Auth never; 02-05's wait/restart --wait reuses it verbatim; tail maps the None-source deadline error to graceful Ok (exit 0)
- [Phase 02-04]: [Phase 02-04]: 'ign logs' shows the NEWEST entries via sortBy=desc(timestamp) + explicit limit (gateway's own openapi asc()/desc() syntax) — 'recent' without inventing a since-window policy; EVERY logs command sends an explicit limit (default 200, loggers included — Pitfall 9); logs -f --json streams NDJSON (one compact entry per line, no envelope) — the SECOND sanctioned stdout exception, README-documented

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 4 spike pending: trial-reset mechanism (Playwright delegation vs native HTTP+CSRF) — resolve at Phase 4 planning
- Phase 5 spike pending: WebDev deploy mechanism (per-resource vs project-zip import); script-exec security posture; tag-history route availability on default rigs
- ~~Phase 2 gap: live-gateway auth verification (token header across /data + /webdev, Basic viability)~~ CLOSED by 02-01: claims verified empirically during research + wiremock-pinned; executable proof path = live_gateway.rs `-- --ignored` (needs IGNITION_LIVE_URL/IGNITION_LIVE_TOKEN per 02-USER-SETUP.md; research rig `ign-research` still up on port 18088 if a fresh token is created). /webdev half re-checks in Phase 5.
- ~~Phase 1: smoke-test keyring 4.1 on headless Linux CI~~ CLOSED & CI-CONFIRMED: keyring-smoke job green on ubuntu headless (run 32517734178, 2026-08-21)

## Session Continuity

**Last session:** 2026-08-22T03:44:19.656Z
**Stopped At:** Completed 02-04-PLAN.md (logs query/tail/download + logger levels + poll engine)
**Resume file:** None
