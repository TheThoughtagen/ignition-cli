# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-04)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Milestone v1.1 Agent Surface & IDE Integration — Phase 9: 09-agent-surface-api-diagnostics

## Current Position

**Phase:** 9 of 14 (09-agent-surface-api-diagnostics)
**Current Plan:** 3
**Total Plans in Phase:** 6
**Status:** Ready to execute
**Last Activity:** 2026-09-07

**Progress:** [█████████░] 92%

## Performance Metrics

**v1.0 baseline (for comparison):** 41 plans, 118 tasks, 9 days (2026-08-20 → 2026-08-29); avg ~38 min/plan; slowest plans were live-gate/WebDev phases (P03-P04 of Phase 5 at ~400+ min).

**v1.1 velocity:** Phase 8 complete (6/6 plans); Phase 9 P02 done (62 min) — P01 pending, then P03-P06.

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 8 | 1/6 | 200 min | 200 min |
| 9 | 0/TBD | - | - |
| 10 | 0/TBD | - | - |
| 11 | 0/TBD | - | - |
| 12 | 0/TBD | - | - |
| 13 | 0/TBD | - | - |
| 14 | 0/TBD | - | - |

*Updated after each plan completion*
| Phase 08 P02 | 200 min | 2 tasks | 3 files |
| Phase 08 P01 | 4h 58min | 3 tasks | 9 files |
| Phase 08 P03 | 1h 40min | 2 tasks | 2 files |
| Phase 08 P04 | 128 min | 2 tasks | 3 files |
| Phase 08 P06 | 189 min | 3 tasks | 5 files |
| Phase 08 P05 | 194 min | 3 tasks | 6 files |
| Phase 09 P02 | 62 min | 2 tasks | 2 files |
| Phase 09 P01 | 210min | 2 tasks | 5 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap]: Phase numbering continues v1.0 (8-14); research layer-2 split into four phases (9-12) — 15 reqs in one phase exceeded standard-depth manageability
- [Roadmap]: Transports (Phase 14) deliberately last — MCP catalog derives from the clap tree, so every command family must land first
- [Roadmap]: Historian binding (Phase 13) is spike-gated — licensed-Historian rig access must be confirmed BEFORE Phase 13 planning
- [Roadmap]: TUIX-05 fully delivered in Phase 8 (config plumbing + worker parameterization + clamp); TUIX-03/04 (rendering) in Phase 12
- [Phase 08]: Session seam is concrete-with-deref (Arc<ReqwestGatewayApi>, not Arc<dyn>) — dyn-widening deferred to Phase 14 where MCP needs dyn — TUI workers/ClientHandle are concretely typed; Phase 8 goal is construction-site unification, not handle-type churn
- [Phase 08]: Session::resolve takes the EFFECTIVE profile flag — IGNITION_PROFILE folding stays in the bin's apply_env_defaults (one env-to-flag home) — Seam must mirror main.rs resolve_profile_context verbatim; re-reading env in core would fork the precedence rule
- [Phase 08]: poll_interval_too_small is its own enum variant riding the config exit class (exit 3) — Phase-7 additive-slug mechanism; serde accepts 0, load-time validation refuses, load_for_tui degrades — keeps code() total, slug first-class in the envelope for agents, and the strict/degrading split is exactly what 08-04's TUI wiring needs
- [Phase 08]: New config keys follow the lenient-degradation pattern: deserialize_with warn+default on wrong types, warn-list entry, skip_serializing_if at defaults — legacy configs round-trip byte-identically — a typo in a NEW key must never fail the load; legacy on-disk shape and contract goldens stay frozen
- [Phase 08]: Plan 08-03: Session gained profile_url/credential_present accessors + resolve_side constructor — doctor's raw-URL/presence contract and the diff/sync no-re-overlay golden contract cannot hold through resolve/resolve_degraded alone — Byte-identical mandate; goldens pinned sides as overlay-immune
- [Phase 08]: Plan 08-03: resolve_profile_context survives reduced to selection-only (profile list view + two-client envelope echo) — envelope must not demand the active profile's secret and list must tolerate fresh installs — Plan's 'otherwise' clause; non-construction consumers keep main.rs selection-only
- [Phase 08]: TUI load pattern: config::load_for_tui then Session::resolve_loaded — caller owns the load policy, seam owns overlay/selection/LOCKED chain; Session::resolve_loaded added to core (returns session + selected POST-OVERLAY profile) — Session::resolve loads strict config internally, physically incompatible with the TUI degradation contract; constructor extension beats keeping a duplicated selection choreography in the TUI
- [Phase 08]: TUI rig helpers return Option<Arc<ReqwestGatewayApi>> via Session::for_url — Session hands out Arc handles (client not Clone); call sites deref with &*api / as_deref — Zero second-source construction in the TUI requires going through the seam; Arc is also the shape 08-05's ResolvedContext wants
- [Phase 08]: Three-Place slug rule now executable: readme_exit_table_agreement parses README exit table via include_str! (section-scoped) and cross-checks against literal (exit,slug) table both directions — nothing later mutates the frozen contract by accident
- [Phase 08]: mcp/lsp/edit pre-declared as reserved OutOfBand slugs (taxonomy + justification only, zero rows); rows land TOGETHER with their clap commands in P13/14 — orphan registry rows fail the clap walk by design; pinned test is the pre-declaration
- [Phase 08]: stdout purity harness is assert-based byte-exact over the real binary (NOT snapbox goldens) so SNAPSHOTS=overwrite cannot sanitize a leaked byte; ambient IGNITION_* env knobs stripped for determinism — single stray stdout byte under max diagnostics must fail CI, never be rewritten
- [Phase 08]: Plan 08-05: ResolvedContext struct replaces the positional (String, String, Arc) triple from resolve/rebuild — poll_interval rides a typed field so the profile-switch chain cannot silently drop it (the update.rs:545-587 trap)
- [Phase 08]: Plan 08-05: 5s poll default has ONE source (workers::refresh::REFRESH_PERIOD, imported by context.rs); only the dashboard refresh worker is parameterized — WATCH/ALARMS/TAIL periods + TICK parked for Phase 12; CI pins plumbing assertions (Duration values per hop), wall-clock cadence is checkpoint-only
- [Phase 09]: Bundle states captured PascalCase: Generating->Valid; BUNDLE_GENERATING_STATES=["Generating"]; unobserved states must passthrough — Live capture on both rigs; lowercase guesses would have shipped wrong (Pitfall 2)
- [Phase 09]: Units locked from captures: uptime=ms-since-gateway-start (wall-clock proven twice); lastSyncTimestamp=-1 never-synced sentinel (unit not capture-proven, model Option, ms flagged inference); fileSize=bytes, key absent until Valid — Magnitude cross-checks against wall clock; -1 sentinel rules out epoch units on fresh rigs
- [Phase 09]: Live 4xx partition: 404+HTML=unknown path, 404+EMPTY=wrong method on real path (NOT 405), 401+HTML=bad auth — DELETE /gateway-info answered 404-empty on both rigs - the plan's 405 hypothesis falsified by capture; evidence for 09-06 gates
- [Phase 09]: 8.3 headless commissioning rides the commissioner wire API (bootstrap -> eula-accept -> start-gateway); 8.3 image entrypoint ignores ACCEPT_EULA/GATEWAY_ADMIN_PASSWORD — Env vars are 8.1-era; wire recipe extracted from commissioner.js and replayed with curl on both rigs
- [Phase 09]: GatewayClientError rides exit 2 with slug gateway_client_error carrying the verbatim 4 KiB-capped gateway body — additive-slug on the frozen taxonomy, Three-Place rule landed atomically (README row + both CI agreement tests)
- [Phase 09]: The api-call catch-all is parameter-scoped (api_call: bool on classify, set only by pub send_and_classify_for_api) — curated pipeline 4xx/exit-1 semantics provably unchanged via pinned non-leak regression

### Pending Todos

None.

### Blockers/Concerns

- [Phase 13 prerequisite]: Confirm licensed-Historian rig access before starting Phase 13 planning — spike cannot proceed without it (documented-limitation fallback is legitimate, but access confirmation must happen first)
- [Phase 11 prerequisite]: Real multi-level UDT export needed for the derive-vs-Event-loop decision — requires a live rig during Phase 11 planning

## Session Continuity

**Last session:** 2026-09-07T05:17:30.281Z
**Resume file:** None
