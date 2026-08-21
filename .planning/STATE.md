# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-20)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Phase 1 — Foundation & Agentic Contracts

## Current Position

**Phase:** 1 of 7 (Foundation & Agentic Contracts)
**Current Plan:** 3
**Total Plans in Phase:** 4
**Status:** Ready to execute
**Last Activity:** 2026-08-21

**Progress:** [█████░░░░░] 50%

## Performance Metrics

**Velocity:**
- Total plans completed: 1
- Total execution time: 8min

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 01 P01 | 8min | 3 tasks | 13 files |

*Updated after each plan completion*
| Phase 01 P02 | 53min | 3 tasks | 9 files |

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

### Pending Todos

None yet.

### Blockers/Concerns

- Phase 4 spike pending: trial-reset mechanism (Playwright delegation vs native HTTP+CSRF) — resolve at Phase 4 planning
- Phase 5 spike pending: WebDev deploy mechanism (per-resource vs project-zip import); script-exec security posture; tag-history route availability on default rigs
- Phase 2 gap: live-gateway auth verification (token header across /data + /webdev, Basic viability) — resolve empirically in Phase 2
- Phase 1: smoke-test keyring 4.1 on headless Linux CI

## Session Continuity

**Last session:** 2026-08-21T16:34:41.700Z
**Last Date:** 2026-08-21T16:34:41.700Z
**Stopped At:** Completed 01-02-PLAN.md (agentic output contract: taxonomy, envelopes, render modes, golden harness)
**Resume file:** None
