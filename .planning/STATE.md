---
gsd_state_version: 1.0
milestone: v1.6
milestone_name: Declared Gateway Deploy
status: "v1.6 shipped — PR #29 open"
stopped_at: Completed 25-27-PLAN.md
last_updated: "2026-10-29T18:00:41.839Z"
last_activity: 2026-10-29
progress:
  total_phases: 6
  completed_phases: 6
  total_plans: 92
  completed_plans: 92
  percent: 100
current_phase: 25
current_phase_name: guarded-project-deploy
last_activity_desc: Milestone v1.6 completed and archived
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-29)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Planning next milestone (v1.6 shipped 2026-10-29; v1.3 Phases 17-18 still parked)

## Current Position

Phase: Milestone v1.6 complete
Plan: —
Status: v1.6 shipped — PR #29 open
Last activity: 2026-10-29

## Performance Metrics

**v1.0 baseline:** 41 plans, 118 tasks, 9 days (2026-08-20 → 2026-08-29); ~73,900 LOC; 863 tests at ship.

**v1.1 final:** 7 phases (8-14), 47 plans, 113 tasks, 12 days (2026-09-04 → 2026-09-16); ~107,954 LOC Rust (+34k); ~1,255 test fns; 236 commits, 263 files changed (+76,357/−2,351).

**v1.6 final:** 6 phases (20-25), 92 plans, 220 tasks, 20 days (2026-10-10 → 2026-10-29); ~247,000 LOC Rust (+139k); ~3,056 test fns; 524 commits, 692 files changed (+230,498/−1,655). Audit `tech_debt`, 47/47 requirements, verified closeout.

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
| Phase 21 P01 | ~15min | 2 tasks | 200 files |
| Phase 21 P02 | 25min | 2 tasks | 14 files |
| Phase 21 P03 | 45min | 3 tasks | 10 files |
| Phase 21 P04 | 60min | 2 tasks | 22 files |
| Phase 21 P05 | 40min | 2 tasks | 6 files |
| Phase 21 P06 | 45min | 2 tasks | 7 files |
| Phase 21 P07 | 75min | 2 tasks | 7 files |
| Phase 21 P08 | 55min | 2 tasks | 8 files |
| Phase 21 P09 | 75min | 2 tasks | 9 files |
| Phase 22 P01 | 30min | 2 tasks | 12 files |
| Phase 22 P02 | 25min | 2 tasks | 10 files |
| Phase 22 P03 | 75min | 3 tasks | 13 files |
| Phase 22 P04 | 70min | 2 tasks | 11 files |
| Phase 22 P05 | 55min | 2 tasks | 9 files |
| Phase 22 P06 | 150min | 2 tasks | 6 files |
| Phase 22 P07 | 80min | 2 tasks | 4 files |
| Phase 22 P08 | 190min | 2 tasks | 12 files |
| Phase 22 P09 | 95min | 2 tasks | 3 files |
| Phase 22 P10 | 70min | 3 tasks | 7 files |
| Phase 22 P11 | 60min | 2 tasks | 10 files |
| Phase 22 P12 | 75min | 3 tasks | 3 files |
| Phase 22 P13 | 3h | 2 tasks | 14 files |
| Phase 22 P14 | 15h | 3 tasks | 7 files |
| Phase 22 P15 | 2h25m | 3 tasks | 6 files |
| Phase 23 P01 | 27min | 3 tasks | 3 files |
| Phase 23 P02 | 70min | 3 tasks | 6 files |
| Phase 23 P03 | 55min | 2 tasks | 6 files |
| Phase 23 P18 | 25min | 2 tasks | 6 files |
| Phase 23 P04 | ~2h | 2 tasks | 9 files |
| Phase 23 P19 | 1h45m | 3 tasks | 5 files |
| Phase 23 P05 | 2h50m | 2 tasks | 6 files |
| Phase 23 P06 | 5h | 3 tasks | 8 files |
| Phase 23 P17 | 1h | 3 tasks | 4 files |
| Phase 23 P07 | 2h | 3 tasks | 8 files |
| Phase 23 P09 | 2h | 3 tasks | 10 files |
| Phase 23 P11 | ~40min | 2 tasks | 4 files |
| Phase 23 P12 | 2h | 2 tasks | 7 files |
| Phase 23 P08 | 2h40m | 2 tasks | 12 files |
| Phase 23 P20 | 1h45m | 2 tasks | 11 files |
| Phase 23 P10 | 1h25m | 3 tasks | 4 files |
| Phase 23 P13 | ~3h | 2 tasks | 9 files |
| Phase 23 P21 | 100min | 2 tasks | 3 files |
| Phase 23 P14 | ~2h | 2 tasks | 12 files |
| Phase 23 P15 | ~1h30m | 2 tasks | 5 files |
| Phase 23 P16 | ~6h | 3 tasks | 7 files |
| Phase 23 P22 | 2h50m | 3 tasks | 5 files |
| Phase 23 P23 | 2h45m | 2 tasks | 5 files |
| Phase 23 P24 | 1h50m | 2 tasks | 3 files |
| Phase 24 P01 | 2h | 3 tasks | 4 files |
| Phase 24 P02 | n/a | 3 tasks | 4 files |
| Phase 24 P03 | n/a | 3 tasks | 8 files |
| Phase 24 P04 | not timed | 2 tasks | 7 files |
| Phase 24 P05 | not timed | 3 tasks | 11 files |
| Phase 24 P06 | not timed | 3 tasks | 7 files |
| Phase 24 P07 | not timed | 2 tasks | 5 files |
| Phase 24 P08 | ~1h (tracer + docs) | 2 tasks | 9 files |
| Phase 24 P09 | not timed | 3 tasks | 12 files |
| Phase 24 P10 | live gates 34 min; suite wait 2 h | 3 tasks | 3 files |
| Phase 25 P01 | 27min | 3 tasks | 2 files |
| Phase 25 P02 | 3h | 3 tasks | 3 files |
| Phase 25 P03 | 2h | 3 tasks | 2 files |
| Phase 25 P04 | ~4h (machine under load) | 2 tasks | 12 files |
| Phase 25 P05 | 1h | 2 tasks | 4 files |
| Phase 25 P06 | about 1h30 | 2 tasks | 6 files |
| Phase 25 P07 | 80min | 2 tasks | 12 files |
| Phase 25 P08 | 1h55m | 3 tasks | 8 files |
| Phase 25 P09 | about 40 min continuation | 2 tasks | 10 files |
| Phase 25 P10 | 25min | 2 tasks | 9 files |
| Phase 25 P11 | ~95min | 2 tasks | 14 files |
| Phase 25 P12 | 10min | 2 tasks | 6 files |
| Phase 25 P13 | ~25min (continuation) | 2 tasks | 7 files |
| Phase 25 P14 | ~1h | 2 tasks | 9 files |
| Phase 25 P15 | 95min | 2 tasks | 7 files |
| Phase 25 P16 | 40min | 2 tasks | 6 files |
| Phase 25 P17 | 2h15m | 3 tasks | 5 files |
| Phase 25 P18 | about 3h40m incl. gate wait | 2 tasks | 9 files |
| Phase 25 P19 | about 35 min (continuation; tracer in an earlier session) | 3 tasks | 4 files |
| Phase 25 P20 | 2h30m | 3 tasks | 7 files |
| Phase 25 P21 | not timed (live run 04:13Z to 04:28Z) | 4 tasks | 8 files |
| Phase 25 P22 | 10min | 2 tasks | 4 files |
| Phase 25 P23 | 25min | 2 tasks | 4 files |
| Phase 25 P24 | 35min | 3 tasks | 10 files |
| Phase 25 P25 | 40min | 2 tasks | 5 files |
| Phase 25 P26 | 25min | 2 tasks | 2 files |

## Accumulated Context

Full v1.1 decision log preserved in: phase `*-SUMMARY.md` files (authoritative per-plan detail), `milestones/v1.1-ROADMAP.md` (phase goals + planner locks), and PROJECT.md Key Decisions (milestone-level outcomes).

### Carry-forwards (open, kept for next milestone)

- [User-owned sequencing] Merge ignition-nvim branch `claude/ign-lsp-live-client` (commit 0d6bd55, ignition_live client registration) to that repo's main — headless e2e evidence committed on the branch; nvim visual check waived at the 14-06 checkpoint
- [SC-5 residual] EAM live-gate §2 vanish-poll: grace row persisted >90s ×2 on the disposable rig, contradicting the fresh-rig <48s upper bound — dedicated capture of scheduled/false post-suspend vanish behavior across fresh + long-lived rigs → re-size the 90s deadline (10-LIVE-GATE.md §4 D1/§6)
- [Polish, minor] `trial_reset`'s defensive tail (crates/ignition-core/src/actions/rig.rs:589) stuffs the rig URL into `CoreError::SecretUnavailable`'s `profile` slot — profile NAME and missing-credential path (IGNITION_USER/IGNITION_PASSWORD) should be named instead; ride along with any later error-message/UX pass
- [Environmental rule] After any transport-affecting change, `cargo install` refresh is REQUIRED — PATH resolves the installed `~/.cargo/bin/ign`, not the repo build (14-06 lesson)
- [Environmental rule, NEW tg4] Git worktrees under `.claude/worktrees/` share the global `CARGO_TARGET_DIR` with the main checkout and cargo gives both the SAME artifact hash — they overwrite each other's `libignition_core.rlib` and dep-info, producing phantom "module not found"/"test vanished" failures that look like code defects. Give each worktree its own `CARGO_TARGET_DIR`. Also: `rm` is shadowed by a wrapper rejecting `-rf`; artifact surgery needs `/bin/rm`
- [Security, accepted AR-01 in 25-SECURITY.md] `deploy-plan.yml`'s ungated `plan` job maps the write-capable `IGN_SSH_KEY` (pre-existing since quick task 261021). Add `IGN_PLAN_SSH_KEY` for a read-restricted key, keep `IGN_SSH_KEY` in the apply environment only, update the README secrets text and add a harness check like 3d. Also open, low (T-25-F2): cap and sanitize the gateway `problem` text that `import_denied` echoes

### Blockers

- [Phase 21 residual, dev-only] WR-08 / R-01: `capture.sh` `assert_local` accepts `http://evil.example#@localhost` — one-line fix `authority=${rest%%[/?#]*}`.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260919-p0g | Add ign testing run verb wrapping testing_discover/testing_run client functions | 2026-09-19 | d5938dd | [260919-p0g-add-ign-testing-run-verb-wrapping-testin](./quick/260919-p0g-add-ign-testing-run-verb-wrapping-testin/) |
| 260919-tg4 | `ign session login` + `ign e2e doctor`/`init` — the IdP session as a Playwright storageState, the six-row E2E diagnosis, and the embedded scaffold | 2026-09-20 | 123a3db | [260919-tg4-add-ign-session-login-plus-ign-e2e-docto](./quick/260919-tg4-add-ign-session-login-plus-ign-e2e-docto/) |
| 260920-iti | Fix credential precedence: profile auth (keyring / named token_env / basic) wins over bare IGNITION_TOKEN and IGNITION_USER/PASSWORD | 2026-09-20 | 3df005a | [260920-iti-fix-credential-precedence-a-profile-with](./quick/260920-iti-fix-credential-precedence-a-profile-with/) |
| 260921-96c | Register workspace push in GUARDED_OPS so it is confirmable over MCP (confirm: true → --yes; rig down stays unguarded by design) | 2026-09-21 | 7800ba6 | [260921-96c-register-workspace-push-in-guarded-ops-s](./quick/260921-96c-register-workspace-push-in-guarded-ops-s/) |
| 5 | fix(tui): watch/alarm workers race in-flight polls against shutdown (Windows CI flake root cause) | 2026-09-22 | d64280c | — |
| 261013-neg | Fix 21-REVIEW WR-02: strict Referenced secret shape, no key-name exemption | 2026-10-13 | ae0338a | [261013-neg-fix-wr-02-referenced-wrapper-secret-fail](./quick/261013-neg-fix-wr-02-referenced-wrapper-secret-fail/) |
| 261018-lqg | Live rig test: apply with a queued config save during the scan-lock hold | 2026-10-18 | 12754c9 | [261018-lqg-live-rig-test-apply-with-a-queued-config](./quick/261018-lqg-live-rig-test-apply-with-a-queued-config/) |
| 261018-n1b | Fix headless build: IsTerminal import gated on tui feature | 2026-10-18 | d55502d | [261018-n1b-fix-headless-build-isterminal-import-gat](./quick/261018-n1b-fix-headless-build-isterminal-import-gat/) |
| 261018-nmf | Make the deploy apply README heading check line-ending agnostic (windows-latest CRLF) | 2026-10-18 | 17b1730 | [261018-nmf-make-the-deploy-apply-readme-heading-che](./quick/261018-nmf-make-the-deploy-apply-readme-heading-che/) |
| 261018-nr2 | Live ssh gate for the Linux ApplyHost (commit.sh) against ign-lnx-836 (superseded by 23-22: the sftp listing gap was closed there; five-gate live ssh run 5 of 5 on ign-lnx-836) | 2026-10-18 | 1372a55 | [261018-nr2-live-ssh-gate-for-the-linux-applyhost-co](./quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/) |
| 261021-m5y | Reusable CI/CD workflows for ign deploy plan and apply | 2026-10-21 | fb70f5e | [261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-](./quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/) |
| 261021-tir | Bump workspace version to 1.6.0-rc.1 | 2026-10-21 | ede85e2 | [261021-tir-bump-workspace-version-to-1-6-0-rc-1](./quick/261021-tir-bump-workspace-version-to-1-6-0-rc-1/) |
| 261019-ftu | Bring the G1 record in 23-LIVE-CAPTURES.md up to the `ls -lan` POSIX listing (docs only) | 2026-10-19 | 3a201b0 | [261019-ftu-g1-live-captures-lan](./quick/261019-ftu-g1-live-captures-lan/) |
| 261029-df7 | WR-04 opt-in release-binary attestation (verify-attestation input, tag-bound attest job) | 2026-10-29 | e92d430 | [261029-df7-wr-04-optional-release-binary-attestatio](./quick/261029-df7-wr-04-optional-release-binary-attestatio/) |
| 261029-fyc | Close stale v1.6 pre-close audit items: 4 debug sessions resolved, nr2 superseded by 23-22, flaky tests to backlog 999.1 | 2026-10-29 | 82fb870 | [261029-fyc-close-stale-v1-6-pre-close-audit-items](./quick/261029-fyc-close-stale-v1-6-pre-close-audit-items/) |
| 14 | Fix clippy 1.99 useless_borrows_in_formatting in live_deploy_projects.rs (b988daa) | 2026-10-27 | b988daa | — |
| 15 | Windows clippy: cfg_attr allow unused_mut in deploy_projects.rs test | 2026-10-27 | b2b01e4 | — |
| 16 | Close G-25-6: win32_code_in reads the real PS 5.1 record (261026-vc6; live rerun passed) | 2026-10-27 | 1510def | — |
| 17 | README: deploy projects supports ssh master nodes | 2026-10-27 | 0a2ecf8 | — |
| 18 | Bump workspace version to 1.6.0-rc.2 | 2026-10-27 | edba4a4 | [261027-7gq-bump-workspace-version-to-1-6-0-rc-2](./quick/261027-7gq-bump-workspace-version-to-1-6-0-rc-2/) |

## Session Continuity

**Last session:** 2026-10-29T03:32:44.412Z
**Stopped At:** Completed 25-27-PLAN.md
**Resume file:** None

## Decisions

v1.6 per-plan decisions were cleared at milestone close. They are preserved in the phase SUMMARY files under `milestones/v1.6-phases/`, the decisions A-D in `milestones/v1.6-ROADMAP.md`, and the outcome table in PROJECT.md Key Decisions.

## Operator Next Steps

- Start the next milestone with /gsd-new-milestone
