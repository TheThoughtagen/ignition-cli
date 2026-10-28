---
gsd_state_version: 1.0
milestone: v1.6
milestone_name: Declared Gateway Deploy
current_phase: 25
current_phase_name: guarded-project-deploy
status: executing
stopped_at: Completed 25-22-PLAN.md
last_updated: "2026-10-28T15:51:30.010Z"
last_activity: 2026-10-28
last_activity_desc: Phase 25 execution started
progress:
  total_phases: 6
  completed_phases: 5
  total_plans: 92
  completed_plans: 87
  percent: 83
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-10-15)

**Core value:** One binary that lets a developer (or an AI agent) fully operate and inspect an Ignition 8.3+ gateway — health, projects, tags, rigs — without opening the gateway webpage or Designer.
**Current focus:** Phase 25 — guarded-project-deploy

## Current Position

Phase: 25 (guarded-project-deploy) — EXECUTING
Plan: 22 of 27 executed (next: 25-23)
Status: Ready to execute
Last activity: 2026-10-28 — Phase 25 execution started

Progress: [██████████] 95%

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

## Accumulated Context

Full v1.1 decision log preserved in: phase `*-SUMMARY.md` files (authoritative per-plan detail), `milestones/v1.1-ROADMAP.md` (phase goals + planner locks), and PROJECT.md Key Decisions (milestone-level outcomes).

### Carry-forwards (open, kept for next milestone)

- [User-owned sequencing] Merge ignition-nvim branch `claude/ign-lsp-live-client` (commit 0d6bd55, ignition_live client registration) to that repo's main — headless e2e evidence committed on the branch; nvim visual check waived at the 14-06 checkpoint
- [SC-5 residual] EAM live-gate §2 vanish-poll: grace row persisted >90s ×2 on the disposable rig, contradicting the fresh-rig <48s upper bound — dedicated capture of scheduled/false post-suspend vanish behavior across fresh + long-lived rigs → re-size the 90s deadline (10-LIVE-GATE.md §4 D1/§6)
- [Polish, minor] `trial_reset`'s defensive tail (crates/ignition-core/src/actions/rig.rs:589) stuffs the rig URL into `CoreError::SecretUnavailable`'s `profile` slot — profile NAME and missing-credential path (IGNITION_USER/IGNITION_PASSWORD) should be named instead; ride along with any later error-message/UX pass
- [Environmental rule] After any transport-affecting change, `cargo install` refresh is REQUIRED — PATH resolves the installed `~/.cargo/bin/ign`, not the repo build (14-06 lesson)
- [Environmental rule, NEW tg4] Git worktrees under `.claude/worktrees/` share the global `CARGO_TARGET_DIR` with the main checkout and cargo gives both the SAME artifact hash — they overwrite each other's `libignition_core.rlib` and dep-info, producing phantom "module not found"/"test vanished" failures that look like code defects. Give each worktree its own `CARGO_TARGET_DIR`. Also: `rm` is shadowed by a wrapper rejecting `-rf`; artifact surgery needs `/bin/rm`

### Blockers

- [RESOLVED 23-22, 2026-10-19] `lnxvm_cli_apply_rollback` failed at `ign deploy plan` on ign-lnx-836 (`remote readdir(".../data/"): Permission denied`): the deploy account was traverse-only on `data` and `data/config`. The user granted `r-x` on those two directories (non-recursive, no default ACL); all five Linux gates then passed in one run. PR #63's account-scope text should add the list right. See `23-LIVE-CAPTURES.md` `### Grant of list rights on data and data/config`.
- [RESOLVED 2026-10-17: host rebuilt via az and recorded `ready` in 23-LIVE-CAPTURES.md; execution resumes at 23-08] [was OPEN 23-01, 2026-10-16] Windows capture host `pending` (`ign-win-836` / `rg-ign-cli-win-test` returns ResourceGroupNotFound; no `IGN_WINVM_*` or `IGN_WFLWVM01_*` set; the user said the host will come later). Stop point: the 23-08 Task 1 precondition. Plans that will not run until a host is recorded `ready` in 23-LIVE-CAPTURES.md `## Windows capture host (23-01)`: 23-08, 23-20, 23-10, 23-13, 23-21, 23-14, 23-15, 23-16. Phase 23 is paused, not complete; resume at 23-08 once the user exports the env and the Task 1 verification (`az vm show`, `ssh ... pwd`) passes.
- [RESOLVED 22-14, exit gate] Decision B = B(i) (home-relative staging, recorded 22-12) and the OQ5 hold budget are recorded in 22-LIVE-CAPTURES.md (`## Decisions: OQ5 ...`). The hold budget `ceil(2 x (p95 commit + p95 scan) + 1)` measured at 152 files is 505 s (Defender exclusion off) and 699 s (on), both ABOVE the 300 s ceiling.
- [RESOLVED 23-10, 2026-10-17: D-01's batched commit measured at 152 and 1,520 files on the rebuilt Windows host; `HOLD_S` = 176 s, unclamped, inside the 300 s ceiling, D-03a not triggered; see `23-LIVE-CAPTURES.md` `## Decisions: OQ5b`] [was OPEN] Hold budget above the ceiling: a measured 152-file commit takes 234 to 348 s on the Windows host (per-file sftp renames), so a 300 s hold lapses mid-commit; a 1,520-file tree extrapolates (not measured) to roughly ten times that. Decide: shorten the locked window, raise the ceiling with the user, or budget the transfer separately (OQ5-2). `op_timeout_s` above 3600 would also be needed for such a tree (OQ5-4).
- [RESOLVED 22-04] WR-03 / R-03: `--out` refresh is not crash-safe (old `render.json` can stay valid over mixed content). Phase 22 re-hashes a `--base` render from its real bytes and never trusts the marker's hashes (21-SECURITY.md); the `--out` refresh itself is unchanged.
- [Phase 21 residual, dev-only] WR-08 / R-01: `capture.sh` `assert_local` accepts `http://evil.example#@localhost` — one-line fix `authority=${rest%%[/?#]*}`.
- [RESOLVED 22-13] svc-ignition-deploy has Modify on the Azure VM `production` collection (post-C6 grant of 2026-10-15, verified as the account); used by the 22-13 live proof and the 22-14 timings
- [RESOLVED 22-15] G-22-2: pulled tree verifies green (symmetric N1); UAT re-test passed on a fresh rig 2026-10-15
- [RESOLVED 22-16] G-22-1: CI run 38838954722 on ab8608a green on all four jobs (ubuntu, macos, windows, keyring-smoke); six CI rounds, see 22-16-SUMMARY.md

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
| 261018-nr2 | Live ssh gate for the Linux ApplyHost (commit.sh) against ign-lnx-836 (incomplete: sftp listing gap) | 2026-10-18 | 1372a55 | [261018-nr2-live-ssh-gate-for-the-linux-applyhost-co](./quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/) |
| 261021-m5y | Reusable CI/CD workflows for ign deploy plan and apply | 2026-10-21 | fb70f5e | [261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-](./quick/261021-m5y-reusable-ci-cd-workflows-for-ign-deploy-/) |
| 261021-tir | Bump workspace version to 1.6.0-rc.1 | 2026-10-21 | ede85e2 | [261021-tir-bump-workspace-version-to-1-6-0-rc-1](./quick/261021-tir-bump-workspace-version-to-1-6-0-rc-1/) |
| 261019-ftu | Bring the G1 record in 23-LIVE-CAPTURES.md up to the `ls -lan` POSIX listing (docs only) | 2026-10-19 | 3a201b0 | [261019-ftu-g1-live-captures-lan](./quick/261019-ftu-g1-live-captures-lan/) |
| 14 | Fix clippy 1.99 useless_borrows_in_formatting in live_deploy_projects.rs (b988daa) | 2026-10-27 | b988daa | — |
| 15 | Windows clippy: cfg_attr allow unused_mut in deploy_projects.rs test | 2026-10-27 | b2b01e4 | — |
| 16 | Close G-25-6: win32_code_in reads the real PS 5.1 record (261026-vc6; live rerun passed) | 2026-10-27 | 1510def | — |
| 17 | README: deploy projects supports ssh master nodes | 2026-10-27 | 0a2ecf8 | — |
| 18 | Bump workspace version to 1.6.0-rc.2 | 2026-10-27 | edba4a4 | [261027-7gq-bump-workspace-version-to-1-6-0-rc-2](./quick/261027-7gq-bump-workspace-version-to-1-6-0-rc-2/) |

## Session Continuity

**Last session:** 2026-10-28T15:51:29.983Z
**Stopped At:** Completed 25-22-PLAN.md
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
- [Phase ?]: [21-01] email-profile secret path is settings.settings.password (RESEARCH typo corrected)
- [Phase ?]: [21-01] OpenAPI resource taxonomy is 56 types (39 named, 17 singleton), not 87/49
- [Phase ?]: 21-02 CD1: v8 sha2 uuid loads on 8.3.6 (4516ad9c-d6b0-8d86-89b9-802c96e33a9e for CAP21_V8); D-03 can be frozen
- [Phase ?]: 21-02 CD3/CD5/CD6: new named types are scope A with uuid; tags/P/type maps to ignition/type/P; gateway encodes colon as %3a and refuses other hostile tag names (NTFS deferred to Phase 22)
- [Phase ?]: 21-03: yaml_serde location is 1-based; custom value errors position at mapping start so schema_version error is relocated to its own line
- [Phase ?]: 21-03: no alias guard needed; yaml_serde repetition limit refuses alias bombs. libyaml-rs and yaml_serde have no build.rs
- [Phase ?]: 21-03: YAML error messages scrub echoed values (invalid type/value, unknown variant, enum tag); unknown field keys kept
- [Phase 21]: 21-04: render_sha256 listing is path NUL hex-sha256 LF in UTF-8 byte order; consumed by Phase 22, change means re-goldening
- [Phase 21]: 21-04: deploy_render exposed over MCP by clap derivation, unguarded, no confirm (Decision D)
- [Phase ?]: 21-05: JSON nesting capped at 128 by an own linear scan (serde_json RawValue validation accepted 200-deep); stage 5 collects JSON findings with allowlist findings
- [Phase ?]: 21-06: CD5 mapping ignition/<type>/<P>/<rest> adopted; unexercised tag layouts (root-level tags, nested type-definition folders) stay refused
- [Phase ?]: 21-07: NamedNoUuid types keep an existing valid uuid (CD3: gateway writes one for store-and-forward-engine); never synthesized
- [Phase ?]: 21-07: tag types (CD5) are exempt from TYPE_RULES and stage 6; TYPE_RULES has 50 rows (19 singleton, 29 named, 2 named-no-uuid)
- [Phase 21]: 21-08: tag-origin files get the Embedded scan only; schema-path and key-name secret rules apply to resource config.json (no resource type for tags)
- [Phase 21]: 21-08: empty string at an OpenAPI secret path is refused as plaintext; empty string under a key-name hit passes; DRND-07 residual (other key names in undescribed types) flagged
- [Phase ?]: 21-09: render.json marker schema_version 1 {gateway, env, mode, render_sha256, files[{path,sha256,bytes}]} is read by Phase 22; --out clobber ladder never deletes and refuses before any write
- [Phase ?]: [22-01] unit_key never climbs above module/type; plan_sha256 canonical text pinned by PLAN_GOLDEN a89de504...c473 and an independent in-test recomputation
- [Phase ?]: [22-01] Scope::from_manifest drops tag-type types entries (tag_providers covers them); for_node refuses ssh nodes until 22-06
- [Phase ?]: 22-02: CORE-01 profile header suppressed for deploy plan (no session resolves); DPLN-02/03 stay unchecked until 22-03
- [Phase ?]: 22-03: removed means not rendered with at least one delete-set file; lockout_risk excludes unmanaged entries (needs confirmation); ledger v1 and deploy_ledger_invalid (exit 6) adopted
- [Phase ?]: 22-04: plan --base re-hashes the previous render (render.json is identity and consistency only), resolving WR-03 / R-03; --diff withholds any member with a secret-bearing location on either side
- [Phase ?]: 22-05: stdout capture is bounded (32 MiB, overflow kills the child as code -2) and a timed-out child is killed and reaped before the call returns
- [Phase ?]: 22-05: ssh option paths quote whitespace, double %, refuse quote/$/control characters; host and user refuse leading -, whitespace, control characters and @
- [Phase ?]: 22-06: SftpTransport lists with ls -la and fetches per-file get (never get -R) so symlinks, sizes and every cap are enforced from the listing; DPLN-01 stays pending until Windows and the write half
- [Phase ?]: 22-07: a Staged belongs to its mirror, mode and staging mode; Separate commit consumes the staged files, so idempotence is stage+commit twice
- [Phase ?]: 22-07: mirror writes are ordered writes, deletes, emptied-dir removal; modes core and local are never mirrored
- [Phase ?]: 22-08: deploy pull's --yes guard counts only files in the three subtrees it replaces; DPLN-06 stays pending until live captures (22-09, 22-14)
- [Phase ?]: 22-09 CV1: match 'will not be loaded' client-side (logger filter is not a prefix filter); window = lastScanTimestamp - lastScanDuration - 1000 ms; settle 5 s; line logged once; REST view still lists the resource
- [Phase ?]: 22-09 CV2: REST config equals file config.json except N1 (null members), N2 (security-levels name-keyed subset), N3 (files [] resource); system-collection items and routeless types skipped
- [Phase ?]: 22-09 CV3: a scan does not rewrite plain-file-write resource files; post-scan bytes equal rendered bytes
- [Phase ?]: 22-10: LOG_WINDOW_SLACK_MS = 1000 and LOG_SETTLE_MS = 5000 per CV1; loaded answers warn while a scan runs and states in its detail that a clean log covers the last scan only
- [Phase ?]: 22-10: deploy_verify_failed stays a constant in verify.rs (not a CoreError, not in DEPLOY_SLUGS); an unreachable gateway or transport is a CoreError, a refused log or tag source is the check's unavailable input carrying only the slug
- [Phase ?]: 22-11: deploy verify keeps the CORE-01 profile header; README must not repeat the literal '## Exit codes' before the real section (readme_exit_table_agreement splits on it)
- [Phase ?]: 22-12 Decision B = B(i): home-relative staging; ledger and journal at ign-deploy/<gateway>/<mode>/ in the service account home on ssh nodes (LocalFs keeps data_dir/ign-deploy); spec section 9 amendment still to do; account needs Modify on each mode collection
- [Phase ?]: 22-13: Windows listings are marked with pwd and every Windows read is cross-checked against a PowerShell \\?\ walk; ssh ledgers and staging live in the account home (B(i))
- [Phase ?]: 22-14: hold budget measured at 152 files is 505 s (no Defender exclusion) and 699 s (exclusion), both above the 300 s ceiling: open for Phase 23; op_timeout_s default 120 to 1050; no Defender exclusion recommended on timing grounds
- [Phase ?]: 22-15: N1 null normalisation is symmetric (null equals absent on file and REST side); N3 key lookup stays on raw REST map
- [Phase ?]: [23-01] C2: the scan-lock queues another actor's API write until the hold lapses; a file written with no lock, under a held lock or across a lapse is not loaded without a scan (120 s); C11 provider and tags load in one scan; C-gwbk is a chunked ZIP, completeness = opens as ZIP
- [Phase ?]: [23-01] Windows capture host recorded pending; stop point 23-08 Task 1; C-restart shape {pending:[string]} captured, no-token and no-level token get 401 HTML
- [Phase ?]: 23-02: Phase 23 contract vocabulary confirmed as option-a (all ten items); eight deploy slugs landed, DEPLOY_SLUGS is 12; PreflightFailure uses Box<[T]> for clippy box_collection
- [Phase ?]: 23-03: RunDir::create runs the git-ignore guard itself (NotIgnored and GitUnavailable refuse with nothing written); record.json always writes post_fingerprint and rolled_back_by (null when unset)
- [Phase ?]: 23-18: a journal refusal is deploy_run_invalid (run_id unknown, problem prefixed by the location); read_ledger and read_journal share one private read helper per transport; journal_location and location_of live in ledger.rs
- [Phase ?]: 23-04: commit refuses a staged copy and target that are the same inode (C7a-4 self-rename deletes); restore copies pre-images through a staged restore copy so the snapshot is never moved and a second restore writes nothing
- [Phase ?]: 23-04: DeployJournalPresent raised by a host carries env '<env>'; the engine (23-05) and verbs own the real --env value
- [Phase ?]: 23-19: HangAfterMove stops the blocking commit after the k-th move and the async commit awaits pending(), so an abort is a faithful kill -9 and no blocking thread lingers
- [Phase ?]: 23-19: FaultyHost counts invocations from 1 per call name; fail and hang never reach the inner host; after-hooks run also when the inner call errored
- [Phase 23]: 23-05: a scan runs once (C11: provider and definitions load in one scan); a host Err that wrote nothing is returned typed with the engine filling deploy_journal_present's env; stale and a post-move commit failure are Internal until 23-17; no lock release or discard in failing branches until 23-17
- [Phase 23]: 23-05: hold.rs carries D-02's formula, floor 10 and ceiling 300 only; hold_for is unclamped, hold_clamped separate; no measured constant until 23-10
- [Phase ?]: 23-06: Linux ssh apply is one hash-checked POSIX sh call; operations with no staging directory use a throw-away .ign-ctl-* directory the script removes on exit; the journal hash seen outside the lock rides in the work list (J record)
- [Phase ?]: 23-17: HeldLock remembers which hold is ours (timestamp and actor read once after the grant); another actor's lock reads as NotHeld, so the DAPL-07 lock-taken case ends rollback_failed
- [Phase ?]: 23-17: restore_snapshot reuses the held lock on the gateway's remaining alone; the local clock can only read shorter, so it is not consulted
- [Phase 23]: 23-07: NodeTarget.gateway_name added (optional, validated); wrong_target compares the master only, backup value validated but unused
- [Phase 23]: 23-07: an unanswerable preflight (network_error, transport_failed) aborts after run.json and result.json are written; it is never folded into deploy_preflight_failed
- [Phase 23]: 23-09: .gwbk completeness is the ZIP central directory (gateway sends no Content-Length); a refused .gwbk is deleted; unreadable restart-pending 'before' reports all pending as new plus restart_pending_unavailable
- [Phase 23]: 23-11: manual rollback cause is Option<RollbackCause> (None = manual); restore_snapshot takes a RestoreContext; rollback result carries restart_pending and restart_pending_new; run_rollback re-runs check_rollback
- [Phase 23]: 23-12: the run's own journal selects recovery in check_rollback; D-10 clear only before the first move with the pre-run fingerprint; the snapshot is hashed before a recovery restores; a commit Err over this run's journal is reported as deploy_journal_present (no restore on a guess)
- [Phase ?]: 23-08: Windows stub runs the hashed commit.ps1 text through [ScriptBlock]::Create (-File fails under AllSigned/Restricted); moved files get an ACL reset; a target that cannot be read is refused as held before any move; WINDOWS_SNAPSHOT_PATH_MAX = 259
- [Phase ?]: [23-20] commit.ps1 has every op commit.sh has; the Windows probe reads the collection's access list against the account's token and never writes into it; a held file is the refusal 'held' for fingerprint as for commit; the Windows mirror commit is one script call and the per-file rename path is gone; apply_host_for_node hands a Windows ssh node the sftp host (commit 76107ed); the conformance table passed 47 of 47 on a real Windows host built with cargo xwin, windows-latest is still to run
- [Phase ?]: 23-13: deploy apply is wired in main.rs through deploy_apply_run (core builds the host with HoldModel::MEASURED); progress lines are human-mode stderr only; deploy apply is excluded from MCP by EXCLUDED_PATHS (a full-path list)
- [Phase ?]: [Phase 23] 23-15: the canary found no leak; CANARY_EXEMPT (snapshot/ and *.gwbk, D-05) is one named constant, asserted present, holding the canary, 0600 and git-ignored; the apply harness moved to tests/common/mod.rs so the purity binary shares it; transport stderr is a scrubbed diagnostics tail by design (22-05) and no file body can reach it
- [Phase ?]: 23-16: the engine asks for a scan-lock wait of 1 s (ACQUIRE_WAIT_S); the real gateway refuses 0 with 400 'Timeouts must be positive'
- [Phase ?]: 23-16: spec section 9 amendment landed as a PR stacked on #60 (whk-environment-orchestration PR #63); user chose a PR for that repo
- [Phase ?]: [23-22] ListFormat is Posix/Windows, both marker-led; a name is read bare or after the current directory's prefix and a bare line must read as a date
- [Phase ?]: [23-22] lnxvm_cli_apply_rollback blocked: svc-ignition-deploy has traverse-only on the Linux data dir, the read half lists it; needs a host or design decision
- [Phase ?]: [23-22] Linux deploy account needs read and execute (list) on data and data/config (user-granted, non-recursive, write denied); all five lnxvm gates pass in one sequential run; PR #63 account-scope text should say so
- [Phase ?]: 24-01: backup lastSyncTimestamp advances on file-path and REST changes and on resync (A1 holds); syncPending was never true at 2 s polling
- [Phase ?]: 24-01: resource.json is not byte-equal across master and backup (config.json is); D-03 byte equality must compare config.json or a normalised resource.json (A2 fails)
- [Phase ?]: 24-01: toggleApproval false cut promotes the backup to Active at once despite pingMaxMissed 1200 (active_node reads Both); a docker network cut leaves the backup Cold and peerConnected true for 180 s (A3, A5)
- [Phase ?]: 24-01: API tokens are node-local on the pair; one token per node is required (A10)
- [Phase ?]: [Phase 24-02] K1=a
- [Phase ?]: [Phase 24-02] K2=b: resource.json compared as parsed JSON, no member excluded
- [Phase ?]: [Phase 24-02] K3=b: gate cuts link at lock_acquired via ApplyObserver
- [Phase ?]: [Phase 24-02] K5=d: sync-exhausted via log matcher, unit-tested only
- [Phase ?]: [Phase 24-02] K7=a plus HTTP 500 counts as CannotList
- [Phase ?]: [Phase 24-02] K8=a: skip 404 declared provider in preflight
- [Phase ?]: [Phase 24-02] V1: applied_backup_unsynced / deploy_backup_unsynced / exit 6
- [Phase ?]: 24-05: SYNC_PREDICATE is TimestampAdvanced (K1 a); FILE_NORMALIZATION holds one row, resource.json compared as a whole parsed document (K2 b)
- [Phase ?]: 24-05: BackupNode is the only backup door (private fields, reads only); backup_deploy replaced by backup: Option<&BackupNode>
- [Phase ?]: 24-06: backup phase runs after finalize outside the lock; one resync to the master, K6 a on sync-exhausted; applied_backup_unsynced never restores the master
- [Phase ?]: 24-07: preflight order is mode_master, mode_backup, wrong_target, write_rest, write_host, designers, hold_budget, master_active, backup_read, secrets; backup_read and secrets are skipped in rollback; K7/K8/K9 applied in the secrets preflight
- [Phase ?]: 24-09: verify --node secrets: 501 and 500 are unverifiable (warn), a declared provider answering 404 is missing, names are exact and case-sensitive (K7/K8/K9)
- [Phase ?]: 24-10: unsynced gate cuts the link at lock_acquired (K3 b, K4 a); secret red state made by removing a replicated name (C-P7)
- [Phase ?]: 24-10 finding: a resource.json with lastModification but no valid signature is re-saved by the backup alone and never converges (content_differs); product decision open
- [Phase ?]: 25-01: C8c probe verdict, zip the checkout layout verbatim; gateway stores zip paths opaquely (K1 input for 25-03)
- [Phase ?]: 25-01: overwrite=true replaces the whole project tree and never reports deletions; unlisted files and resource folders holding a subdirectory are dropped silently (K2, K3, K5 input)
- [Phase ?]: 25-02 C10: an API overwrite import is not reverted by the git module but deletes the module-managed project's .git permanently; the module then skips it (K8, D-11)
- [Phase ?]: 25-02 C-H: the .git probe must be stat (Linux) or Get-Item -Force / enumeration (Windows); test -e, Exists and Test-Path answer absent when the project dir cannot be traversed (K9)
- [Phase ?]: 25-02 C9: parent overwrite leaves the child export unchanged and settles within 5 s; 30 s ceiling suffices; session impact stays OPEN (K7, D-06)
- [Phase 25-03]: user approved all contract items K1-K15 and V1-V11 at their Recommended option (2026-10-22); K10, V8, V11 decided D-13, D-14, D-15
- [Phase 25-03]: OPEN for the cutover (D-11): long-term branch lifecycle of module-managed projects, not left on a stray production branch indefinitely; does not change K8
- [Phase 25]: [25-04] Canonical key is the zip path verbatim (K1 a); root files matching the module-id shape (project.json) are skipped, not refused — K1 a and the webdev resources directory; found by unit test
- [Phase 25]: 25-06: deploy is denied at the repo root only (K3), not in BUILTIN_DENY; an artifact holding only project.json refuses at build time; human output and human error lines escape control characters in repo-derived names
- [Phase 25]: 25-07: local .git probe uses symlink_metadata (no follow, any kind counts); a missing projects dir or unreadable project dir is transport_failed, never absent
- [Phase 25]: 25-08: K9 Linux probe is stat-over-exec and the Windows probe is Get-Item -Force per level; denied is transport_failed on both, proven on the capture VMs
- [Phase 25]: 25-09: same release means equal semver version including build metadata; downgrade gate runs before any gateway read, first_deploy gate after the live digest
- [Phase 25]: 25-09: mode projects is reserved in Target::validate with its own message (not a gateway collection)
- [Phase 25]: 25-10: parent check reads one projects/list call (K6 a); walk bounded at len+1 steps; invalidParent on a usable parent is a warning, not a refusal
- [Phase 25]: 25-11: a failed read-back after the import is imported_unverified with a warning, not an error; a record-only run re-checks the live digest before writing the record
- [Phase 25]: 25-11: DPRJ-01/03/04/05 stay open after 25-11; each is finished by 25-12 (verdict), 25-13 (ssh record), 25-14 (preflights), 25-16..25-19
- [Phase 25]: 25-12: deploy_project_unverified is a payload verdict (sixth success-path exit exception, exit 6), never a CoreError slug or exit-table row; restore command is repo-root-relative with no --profile and the human output names the gateway
- [Phase 25]: 25-12: DPRJ-03 and DPRJ-05 stay open after 25-12; DPRJ-03 needs ssh record writers (25-13), preflights (25-14) and live proof (25-19); DPRJ-05 needs 25-17..25-19
- [Phase 25]: 25-13: release record temp suffix is the record's validated run_id (no random fallback); the Windows replace is Move-Item -Force via WinScript::move_replace, whose atomicity on a real volume is not yet proven (see 25-13-SUMMARY open caveat)
- [Phase 25]: 25-13: DPRJ-03 stays open after 25-13; 25-14 (preflights), 25-15 (children sweep), 25-17 and 25-19 (live proof) still own parts
- [Phase 25]: 25-14: wrong_target runs for every confirmed deploy projects plan (record included); designers check covers any open session, skipped for record-only and noted in run.json/events only — Shared preflight implementation with deploy apply (T-25-44, T-25-45); refusal text names no gateway-reported value
- [Phase 25]: 25-15: CHILDREN_SETTLE_S is 30 (K7), not the plan's derived 10; only direct children are swept (user accepted); a drifted or unreadable child is a warning, never a failure; a child with no record is unverified without a warning
- [Phase 25]: 25-15: DPRJ-02 and DPRJ-03 stay open after 25-15; 25-19 (both) and 25-17 (DPRJ-03) still declare them; the S5 session residual stays open and unclaimed
- [Phase 25]: 25-16: --targets DIR on deploy render/plan/pull/verify/apply/rollback via additive _in entry points; old names delegate with targets = repo — Run directory, gateway tree, render --out and pull destination stay under --repo (D-07, D-10, D-14)
- [Phase 25]: 25-16: DPRJ-01 stays open after 25-16; 25-18 (reusable deploy-projects workflow) and 25-19 (live gates) still declare it, and 25-19 proves it on the rig
- [Phase ?]: Phase 25 P17: PROJECTS_CANARY_EXEMPT = [export]; the pre-image export/<project>.zip is the only run file that may hold project content (0600 in a 0700 directory); every canary suite asserts it exists and holds the canary
- [Phase ?]: Phase 25 P17: cutover runbook follows K8 a (module stays installed, administrator removes .git per project); the owner's stray-production-branch question and the WHK-Global images/themes gap are recorded in the README as OPEN, unanswered
- [Phase 25]: 25-18: the ungated deploy-projects check job holds only IGN_CHECK_TOKEN and IGN_CHECK_SSH_KEY; the write token and key are environment secrets of the apply job; no fallback
- [Phase 25]: 25-18: deploy-projects apply runs even when the check says skip, and a newer pending apply replaces an older pending one (user-accepted)
- [Phase 25]: 25-19: DPRJ-01..05 marked complete on the live rig gates and the installed-binary smoke; GitHub-side behaviour of deploy-projects.yml stays unproven and is not what any requirement rests on
- [Phase 25]: 25-21: Add-Type kept (cost: baseline median 1.51 s, record_replace median 2.04 s, share 0.53 s); WR-01 cause reporting proven live on ign-win-836; the blocked-replace live test failed in win32_code_in (first match is the source excerpt of the PS 5.1 error record), new gap G-25-6, G-25-5 stays failed until it is fixed and rerun live
- [Phase ?]: 25-22: Windows static CRT flag passed via setup-rust-toolchain rustflags matrix input (config-file flag would be overridden by exported RUSTFLAGS); dumpbin import check blocks packaging
