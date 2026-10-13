---
gsd_state_version: 1.0
milestone: v1.6
milestone_name: Declared Gateway Deploy
current_phase: 21
current_phase_name: Deploy Render
status: executing
stopped_at: Completed 20-06-PLAN.md
last_updated: "2026-10-13T03:06:41.338Z"
last_activity: 2026-10-12
last_activity_desc: Phase 20 complete, transitioned to Phase 21
progress:
  total_phases: 6
  completed_phases: 1
  total_plans: 15
  completed_plans: 6
  percent: 17
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-10)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Phase 20 — config-api-client

## Current Position

Phase: 21 — Deploy Render
Plan: Not started
Status: Ready to execute
Last activity: 2026-10-12 — Phase 20 complete, transitioned to Phase 21

Progress: [██████████] 100%

## Performance Metrics

**v1.0 baseline:** 41 plans, 118 tasks, 9 days (2026-08-20 → 2026-08-29); ~73,900 LOC; 863 tests at ship.

**v1.1 final:** 7 phases (8-14), 47 plans, 113 tasks, 12 days (2026-09-04 → 2026-09-16); ~107,954 LOC Rust (+34k); ~1,255 test fns; 236 commits, 263 files changed (+76,357/−2,351).

Phase velocity highlights: capture-first phases (10, 13) ran slowest (~127 min/plan avg — live-rig windows dominate); polish phases ran fastest (12: 22 min/plan avg). Full per-plan timings preserved in the git history and phase summaries.
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 20 P01 | 35min | 2 tasks | 11 files |
| Phase 20 P02 | 30min | 3 tasks | 7 files |
| Phase 20 P03 | 50min | 2 tasks | 5 files |
| Phase 20 P04 | 55min | 2 tasks | 5 files |
| Phase 20 P05 | 70min | 2 tasks | 7 files |
| Phase 20 P06 | 35min | 2 tasks | 3 files |

## Accumulated Context

Full v1.1 decision log preserved in: phase `*-SUMMARY.md` files (authoritative per-plan detail), `milestones/v1.1-ROADMAP.md` (phase goals + planner locks), and PROJECT.md Key Decisions (milestone-level outcomes).

### Carry-forwards (open, kept for next milestone)

- [User-owned sequencing] Merge ignition-nvim branch `claude/ign-lsp-live-client` (commit 0d6bd55, ignition_live client registration) to that repo's main — headless e2e evidence committed on the branch; nvim visual check waived at the 14-06 checkpoint
- [SC-5 residual] EAM live-gate §2 vanish-poll: grace row persisted >90s ×2 on the disposable rig, contradicting the fresh-rig <48s upper bound — dedicated capture of scheduled/false post-suspend vanish behavior across fresh + long-lived rigs → re-size the 90s deadline (10-LIVE-GATE.md §4 D1/§6)
- [Polish, minor] `trial_reset`'s defensive tail (crates/ignition-core/src/actions/rig.rs:589) stuffs the rig URL into `CoreError::SecretUnavailable`'s `profile` slot — profile NAME and missing-credential path (IGNITION_USER/IGNITION_PASSWORD) should be named instead; ride along with any later error-message/UX pass
- [Environmental rule] After any transport-affecting change, `cargo install` refresh is REQUIRED — PATH resolves the installed `~/.cargo/bin/ign`, not the repo build (14-06 lesson)
- [Environmental rule, NEW tg4] Git worktrees under `.claude/worktrees/` share the global `CARGO_TARGET_DIR` with the main checkout and cargo gives both the SAME artifact hash — they overwrite each other's `libignition_core.rlib` and dep-info, producing phantom "module not found"/"test vanished" failures that look like code defects. Give each worktree its own `CARGO_TARGET_DIR`. Also: `rm` is shadowed by a wrapper rejecting `-rf`; artifact surgery needs `/bin/rm`

### Blockers

- [Before Phase 23 planning] Decision B (staging location / svc-ignition-deploy write scope) is OPEN. Resolve in Phase 22 from capture C6 and record it; Phase 23 also needs the OQ5 measured hold budget.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260919-p0g | Add ign testing run verb wrapping testing_discover/testing_run client functions | 2026-09-19 | d5938dd | [260919-p0g-add-ign-testing-run-verb-wrapping-testin](./quick/260919-p0g-add-ign-testing-run-verb-wrapping-testin/) |
| 260919-tg4 | `ign session login` + `ign e2e doctor`/`init` — the IdP session as a Playwright storageState, the six-row E2E diagnosis, and the embedded scaffold | 2026-09-20 | 123a3db | [260919-tg4-add-ign-session-login-plus-ign-e2e-docto](./quick/260919-tg4-add-ign-session-login-plus-ign-e2e-docto/) |
| 260920-iti | Fix credential precedence: profile auth (keyring / named token_env / basic) wins over bare IGNITION_TOKEN and IGNITION_USER/PASSWORD | 2026-09-20 | 3df005a | [260920-iti-fix-credential-precedence-a-profile-with](./quick/260920-iti-fix-credential-precedence-a-profile-with/) |
| 260921-96c | Register workspace push in GUARDED_OPS so it is confirmable over MCP (confirm: true → --yes; rig down stays unguarded by design) | 2026-09-21 | 7800ba6 | [260921-96c-register-workspace-push-in-guarded-ops-s](./quick/260921-96c-register-workspace-push-in-guarded-ops-s/) |
| 5 | fix(tui): watch/alarm workers race in-flight polls against shutdown (Windows CI flake root cause) | 2026-09-22 | d64280c | — |

## Session Continuity

**Last session:** 2026-10-12T16:37:10.632Z
**Stopped At:** Completed 20-06-PLAN.md
**Resume file:** None

## Decisions

- [Phase ?]: [20-01] Competing scan-lock acquire is an immediate 409 with an HTML body naming the actor; 408 never observed (D1)
- [Phase ?]: [20-01] Stale signature is a 500 marked 'signature mismatch'; omitted and empty signatures are one identical 422 (D3)
- [Phase ?]: [20-01] list is effective-view only and filters with object-style field[op]=value; list-by-collection is names then find?collection= (D7)
- [Phase ?]: [20-01] Any actor's scan releases the scan-lock at trigger time; a no-op scan advances lastScanTimestamp (D4, D12)
- [Phase ?]: 20-02: active_node decides by activityLevel only; role is configuration (C-S3 failover keeps Master fixed while activity moves)
- [Phase ?]: 20-02: ResourceSignatureRejected carries only name, reason and endpoint; never a signature or gateway problem text
- [Phase ?]: 20-03: D1 makes 409 the primary scan-lock refusal pin (408 mapped, enrichment-pinned); D4 added no scan-route classify arm; ScanStatus decisive fields required
- [Phase ?]: 20-04: resource_list sees the effective view only (D7); named collections are listed by names then find?collection= (resource_list_in_collection); filters go out object-style field[op]=v
- [Phase ?]: 20-04: stale signature (500 + 'signature mismatch') and missing signature (422 fieldName signature) are typed only in the deploy-scoped write guard, never in classify.rs
- [Phase ?]: [20-05] deployment_mode() returns Option<String>; null, empty, absent, blank all read as no mode (D5); GatewayInfo untouched via private GatewayModeProbe
- [Phase ?]: [20-05] mode_create is confirmed by a follow-up modes() read; duplicate (409) and illegal name (400) are internal via classify fallback; FakeDeploy reuses lock_verdict/scan_complete/normalize_deployment_mode/validate_segment
- [Phase ?]: 20-06: live gate booted with plain compose.yml; no wire drift, fake trace equals live trace
