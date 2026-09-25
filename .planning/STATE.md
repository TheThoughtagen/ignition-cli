---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Rig Modules & Declared Discovery
current_phase: 15
current_phase_name: 2/2 plans
status: planning
stopped_at: "Completed quick/260919-tg4 (3 commits: 063f389, a1863ea, 7859247); next: `/gsd-new-milestone`"
last_updated: "2026-09-22T12:29:45.761Z"
last_activity: 2026-09-21
last_activity_desc: "Completed quick task 260921-96c: workspace push confirmable over MCP (GUARDED_OPS); carried 260920-iti credential precedence fix onto this branch"
progress:
  total_phases: 4
  completed_phases: 1
  total_plans: 2
  completed_plans: 2
  percent: 25
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-16)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Planning next milestone (v1.1 Agent Surface & IDE Integration shipped 2026-09-16; start with `/gsd-new-milestone`)

## Current Position

Phase: 16-compose-override-module-injection — IN PROGRESS (2/3 plans)
Plan: 16-01 and 16-02 complete; next 16-03 (live gate, needs Docker, human checkpoint on evidence)
Status: mechanism and CLI proven Docker-free. SC-2 (a module actually loading) still unproven, and three details are stable-but-unverified: the comma-joined ACCEPT_MODULE_* separator, the gateway_service heuristic, and SC-3's mount-survives-recreate half
Last activity: 2026-09-21 - Completed quick task 260921-96c: workspace push confirmable over MCP (GUARDED_OPS); carried 260920-iti credential precedence fix onto this branch

## Performance Metrics

**v1.0 baseline:** 41 plans, 118 tasks, 9 days (2026-08-20 → 2026-08-29); ~73,900 LOC; 863 tests at ship.

**v1.1 final:** 7 phases (8-14), 47 plans, 113 tasks, 12 days (2026-09-04 → 2026-09-16); ~107,954 LOC Rust (+34k); ~1,255 test fns; 236 commits, 263 files changed (+76,357/−2,351).

Phase velocity highlights: capture-first phases (10, 13) ran slowest (~127 min/plan avg — live-rig windows dominate); polish phases ran fastest (12: 22 min/plan avg). Full per-plan timings preserved in the git history and phase summaries.

## Accumulated Context

Full v1.1 decision log preserved in: phase `*-SUMMARY.md` files (authoritative per-plan detail), `milestones/v1.1-ROADMAP.md` (phase goals + planner locks), and PROJECT.md Key Decisions (milestone-level outcomes).

### Carry-forwards (open, kept for next milestone)

- [User-owned sequencing] Merge ignition-nvim branch `claude/ign-lsp-live-client` (commit 0d6bd55, ignition_live client registration) to that repo's main — headless e2e evidence committed on the branch; nvim visual check waived at the 14-06 checkpoint
- [SC-5 residual] EAM live-gate §2 vanish-poll: grace row persisted >90s ×2 on the disposable rig, contradicting the fresh-rig <48s upper bound — dedicated capture of scheduled/false post-suspend vanish behavior across fresh + long-lived rigs → re-size the 90s deadline (10-LIVE-GATE.md §4 D1/§6)
- [Polish, minor] `trial_reset`'s defensive tail (crates/ignition-core/src/actions/rig.rs:589) stuffs the rig URL into `CoreError::SecretUnavailable`'s `profile` slot — profile NAME and missing-credential path (IGNITION_USER/IGNITION_PASSWORD) should be named instead; ride along with any later error-message/UX pass
- [Environmental rule] After any transport-affecting change, `cargo install` refresh is REQUIRED — PATH resolves the installed `~/.cargo/bin/ign`, not the repo build (14-06 lesson)
- [Environmental rule, NEW tg4] Git worktrees under `.claude/worktrees/` share the global `CARGO_TARGET_DIR` with the main checkout and cargo gives both the SAME artifact hash — they overwrite each other's `libignition_core.rlib` and dep-info, producing phantom "module not found"/"test vanished" failures that look like code defects. Give each worktree its own `CARGO_TARGET_DIR`. Also: `rm` is shadowed by a wrapper rejecting `-rf`; artifact surgery needs `/bin/rm`

### Blockers

(None — clean milestone boundary.)

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260919-p0g | Add ign testing run verb wrapping testing_discover/testing_run client functions | 2026-09-19 | d5938dd | [260919-p0g-add-ign-testing-run-verb-wrapping-testin](./quick/260919-p0g-add-ign-testing-run-verb-wrapping-testin/) |
| 260919-tg4 | `ign session login` + `ign e2e doctor`/`init` — the IdP session as a Playwright storageState, the six-row E2E diagnosis, and the embedded scaffold | 2026-09-20 | 123a3db | [260919-tg4-add-ign-session-login-plus-ign-e2e-docto](./quick/260919-tg4-add-ign-session-login-plus-ign-e2e-docto/) |
| 260920-iti | Fix credential precedence: profile auth (keyring / named token_env / basic) wins over bare IGNITION_TOKEN and IGNITION_USER/PASSWORD | 2026-09-20 | 3df005a | [260920-iti-fix-credential-precedence-a-profile-with](./quick/260920-iti-fix-credential-precedence-a-profile-with/) |
| 260921-96c | Register workspace push in GUARDED_OPS so it is confirmable over MCP (confirm: true → --yes; rig down stays unguarded by design) | 2026-09-21 | 7800ba6 | [260921-96c-register-workspace-push-in-guarded-ops-s](./quick/260921-96c-register-workspace-push-in-guarded-ops-s/) |
| 5 | fix(tui): watch/alarm workers race in-flight polls against shutdown (Windows CI flake root cause) | 2026-09-22 | d64280c | — |

## Session Continuity

**Last session:** 2026-09-20T01:15:00Z
**Stopped At:** Completed quick/260919-tg4 (3 commits: 063f389, a1863ea, 7859247); next: `/gsd-new-milestone`
**Resume file:** None
