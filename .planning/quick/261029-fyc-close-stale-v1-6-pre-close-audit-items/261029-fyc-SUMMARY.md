---
phase: quick-261029-fyc
plan: 01
subsystem: planning-hygiene
tags: [audit-open, milestone-close, debug-sessions, backlog]
status: complete
requirements: [QUICK-fyc]
key-files:
  modified:
    - .planning/debug/knowledge-base.md
    - .planning/phases/22-transport-plan-pull-verify/22-UAT.md
    - .planning/quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/261018-nr2-SUMMARY.md
    - .planning/STATE.md
    - .planning/ROADMAP.md
    - .planning/phases/24-redundant-pair/deferred-items.md
    - .planning/phases/25-guarded-project-deploy/deferred-items.md
  moved:
    - .planning/debug/{pulled-tree-cobranding-readback,eam-create-422,loss-gate-hint-mismatch,udt-type-fact-falsified}.md -> .planning/debug/resolved/
    - .planning/debug/{eam-history-raw,eam-working-definition}.json -> .planning/debug/resolved/
  created:
    - .planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep
decisions:
  - "Debug KB index stays at .planning/debug/knowledge-base.md and is excluded by frontmatter status: complete."
  - "Timing-sensitive tests tracked as backlog Phase 999.1; the tests themselves are not fixed."
actuals:
  tokens: 40000
  tasks: 3
  commits: 3
---

# Phase quick-261029-fyc Plan 01: Close stale v1.6 pre-close audit items Summary

Ten stale `audit-open` items closed on in-repo evidence (four debug sessions resolved and moved, the debug KB index excluded by frontmatter, quick task nr2 closed as superseded by 23-22, three deferred entries moved to a new backlog Phase 999.1), with history preserved through `git mv` and insert-only edits. No Rust source changed and nothing was built.

## Commits

| Task | Commit | Content |
| ---- | ------ | ------- |
| 1 (tracer) | b5f11bd | A1 G-22-2 session resolved (22-15: 90920e6, ba00845, 6a60a05, 4353824, bc6bb24) and moved; 22-UAT.md debug_session link repointed; A2 KB frontmatter; A5 nr2 closed as superseded by 23-22 plus the STATE.md nr2 row |
| 2 | 9b6869b | A3 EAM 422 resolved (07-05, 4bdbb81) with the two EAM JSON evidence files moved; A4 loss-gate-hint-mismatch (11-07, 459307d and cbfc022) and udt-type-fact-falsified (11-08, 8df05fb and d779d7c) resolved and moved |
| 3 | 82fb870 | A6 ROADMAP `## Backlog` with Phase 999.1, `.gitkeep`, three deferred entries marked `status: resolved` plus a resolution line |

## Outcomes by item

- **A1** pulled-tree-cobranding-readback: resolved by 22-15. Confirmed all five commits are ancestors of HEAD, 22-UAT.md test 2 `result: pass`, and 22-VERIFICATION.md `status: passed` with G-22-2 under `gaps_closed`.
- **A2** knowledge-base.md: stays in place with its entries unchanged. It now has frontmatter `status: complete` and `kind: debug-knowledge-base`. `scanDebugSessions` in gsd-core `bin/lib/audit.cjs` skips only status `resolved` or `complete` and has no name-based exclusion, so the frontmatter status is what keeps the index off the open list. It is no longer a false positive.
- **A3** eam-create-422: resolved by 07-05 Task 2 (4bdbb81). `eam-history-raw.json` and `eam-working-definition.json` moved beside it with content unchanged (R100).
- **A4** loss-gate-hint-mismatch and udt-type-fact-falsified: every evidence check passed (commits are ancestors; `LOSS_GATE_REFUSAL_REASON_PREFIX` is defined and branched on in `CoreError::hint` and pinned in `contract_tags.rs`; `tag_loss.rs` states the corrected provider-root vs folder-basePath scope; 11-VERIFICATION.md `gaps_closed` has UAT tests 3 and 4). No A4 file is flagged.
- **A5** 261018-nr2: SUMMARY now `status: complete`, `original_status: incomplete`, `superseded_by: 23-22`, plus a closure note and a blockquote under the H1. Confirmed 23-VERIFICATION.md `status: passed` naming UAT G1 / 23-22 at 5 of 5, and 23-22-SUMMARY.md `status: complete` with five-gate run #2 at 5 of 5 PASS. The STATE.md nr2 row description says superseded by 23-22.
- **A6** deferred items: ROADMAP.md ends with `## Backlog` (the only `##` heading from that point to EOF) holding Phase 999.1 with both tests and their sources. `phase.next-decimal 999` now returns 999.2. Both deferred-items files keep every original line (numstat: 0 deletions); each of the three entries carries indented `status: resolved` and a `resolution:` line.

## Deviations from Plan

None in substance. Two mechanical notes:

- Commits were made with `git add` of named paths followed by `git commit` rather than `gsd-tools query commit`. The staged name-status was checked before each commit and matched the intended path list. For Task 1 the old pulled-tree path was already staged by `git mv`, so only the new path was added.
- The ROADMAP backlog block was appended with an in-place append (a Python script) instead of the Edit tool. The diff is 15 insertions and 0 deletions.

## Caveats

- **07-UAT.md (archived, unedited):** round 2 test 2 still reads `result: [pending]` while the round's Summary counts passed 5, issues 0. The EAM closure rests on the 07-VERIFICATION-GAPS.md pass, the 07-05 live create exiting 0 on gateway A, and the contract tests. This is recorded as a caveat in the resolved eam-create-422.md.
- **11-UAT.md (archived, unedited):** still `status: diagnosed` with tests 3 and 4 recorded as issue. No live re-test was recorded after 11-07 and 11-08. Closure of both Phase 11 sessions rests on 11-VERIFICATION.md re-verification, the contract tests and the code. This is recorded as a caveat in both resolved session files.
- **Stale paths in archived docs:** 07-UAT.md, 07-05-PLAN.md, 11-UAT.md, 11-08-SUMMARY.md and 22-15-PLAN.md still cite the old `.planning/debug/<name>.md` paths. They were left unedited as history. Only the live 22-UAT.md `debug_session:` link was repointed.
- **Backlog 999.1 is tracking only.** The two timing-sensitive tests (`guard_drop_during_unwind_inside_runtime_still_cleans_up`, `timeout_kills_child`) are not fixed and stay timing-sensitive. The deferred entries read `status: resolved` because they moved to backlog, not because the tests were repaired.
- Not committed by this task, per the orchestrator: this SUMMARY, the PLAN, and the new 261029-fyc row in STATE.md. `.planning/config.json`, `.planning/v1.6-MILESTONE-AUDIT.md`, `.planning/research/.cache/` and the repo-root `svc-ignition-deploy@*.ps1` were never staged.

## Threat Flags

None.

## Final audit-open

Output of `node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open` run after this SUMMARY was written with `status: complete`:

```
Milestone Close: Open Artifact Audit

  All artifact types clear. Safe to proceed.
```

No items remain open. JSON counts: debug_sessions 0, quick_tasks 0, threads 0, todos 0, seeds 0, uat_gaps 0, verification_gaps 0, context_questions 0, deferred_items 0, total 0.

## Self-Check: PASSED

All moved/created files exist, commits b5f11bd, 9b6869b and 82fb870 exist, `git diff a9cb111 -- crates` is empty.
