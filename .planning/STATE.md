---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Rig Modules & Declared Discovery
status: planning
last_updated: "2026-09-20T15:16:39.958Z"
last_activity: 2026-09-20
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-09-16)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Planning next milestone (v1.1 Agent Surface & IDE Integration shipped 2026-09-16; start with `/gsd-new-milestone`)

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-09-20 — Milestone v1.3 started

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

## Session Continuity

**Last session:** 2026-09-20T01:15:00Z
**Stopped At:** Completed quick/260919-tg4 (3 commits: 063f389, a1863ea, 7859247); next: `/gsd-new-milestone`
**Resume file:** None
