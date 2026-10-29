---
phase: quick-261029-fyc
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - .planning/debug/pulled-tree-cobranding-readback.md
  - .planning/debug/resolved/pulled-tree-cobranding-readback.md
  - .planning/debug/knowledge-base.md
  - .planning/phases/22-transport-plan-pull-verify/22-UAT.md
  - .planning/quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/261018-nr2-SUMMARY.md
  - .planning/STATE.md
  - .planning/debug/eam-create-422.md
  - .planning/debug/eam-history-raw.json
  - .planning/debug/eam-working-definition.json
  - .planning/debug/resolved/eam-create-422.md
  - .planning/debug/resolved/eam-history-raw.json
  - .planning/debug/resolved/eam-working-definition.json
  - .planning/debug/loss-gate-hint-mismatch.md
  - .planning/debug/udt-type-fact-falsified.md
  - .planning/debug/resolved/loss-gate-hint-mismatch.md
  - .planning/debug/resolved/udt-type-fact-falsified.md
  - .planning/ROADMAP.md
  - .planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep
  - .planning/phases/24-redundant-pair/deferred-items.md
  - .planning/phases/25-guarded-project-deploy/deferred-items.md
autonomous: true
requirements: [QUICK-fyc]

estimate:
  tokens: 70000
  raw_tokens: 70000
  tasks: 3
  confidence: low

must_haves:
  truths:
    - "audit-open lists none of pulled-tree-cobranding-readback, eam-create-422, loss-gate-hint-mismatch or udt-type-fact-falsified. Each now lives in .planning/debug/resolved/ with frontmatter status: resolved and a Resolution section that names the fixing plan and commit hashes (22-15 ba00845; 07-05 4bdbb81; 11-07 459307d; 11-08 8df05fb). A4 only applies when its evidence check passed. A file whose check failed stays where it is and is flagged in the SUMMARY."
    - "eam-history-raw.json and eam-working-definition.json moved with git mv into .planning/debug/resolved/ beside eam-create-422.md. Their content is unchanged."
    - ".planning/debug/knowledge-base.md is still at its path, its entries unchanged. It carries frontmatter status: complete, because scanDebugSessions in ~/.claude/gsd-core/bin/lib/audit.cjs skips only status resolved or complete and has no rule that excludes by name. audit-open no longer lists it."
    - "audit-open no longer lists quick task 261018-nr2. Its SUMMARY frontmatter reads status: complete, original_status: incomplete, superseded_by: 23-22. Its row in the STATE.md Quick Tasks table says it was superseded by 23-22."
    - "ROADMAP.md ends with a ## Backlog section (the last ## heading in the file). It holds Phase 999.1, which names both timing-sensitive tests and their deferred-items sources. .planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep exists. phase.next-decimal 999 now returns 999.2."
    - "audit-open reports 0 deferred items. Every original line of both deferred-items.md files is still present (git numstat shows 0 deleted lines). Each of the three entries has an indented status: resolved line and a resolution line, which say it moved to backlog 999.1 and the test was not fixed."
    - "No file under crates/ changed, and nothing was built. .planning/config.json, .planning/v1.6-MILESTONE-AUDIT.md and the untracked .ps1 file in the repo root are not in any commit."
    - "The SUMMARY records the final audit-open output (the run after the SUMMARY was written with status: complete) and lists every item still open."
  artifacts:
    - path: ".planning/debug/resolved/pulled-tree-cobranding-readback.md"
      provides: "G-22-2 session, resolved, citing 22-15"
      contains: "status: resolved"
    - path: ".planning/debug/resolved/eam-create-422.md"
      provides: "EAM create 422 evidence note, resolved by 07-05 (4bdbb81)"
      contains: "4bdbb81"
    - path: ".planning/debug/resolved/loss-gate-hint-mismatch.md"
      provides: "Phase 11 UAT test 3 session, resolved by 11-07"
      contains: "459307d"
    - path: ".planning/debug/resolved/udt-type-fact-falsified.md"
      provides: "Phase 11 UAT test 4 session, resolved by 11-08"
      contains: "8df05fb"
    - path: ".planning/debug/knowledge-base.md"
      provides: "resolved-sessions index kept in place, excluded from the open list by frontmatter"
      contains: "status: complete"
    - path: ".planning/quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/261018-nr2-SUMMARY.md"
      provides: "nr2 closed as superseded by 23-22"
      contains: "superseded_by: 23-22"
    - path: ".planning/ROADMAP.md"
      provides: "## Backlog with Phase 999.1 for the two timing-sensitive tests"
      contains: "### Phase 999.1:"
    - path: ".planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep"
      provides: "backlog phase directory per the add-backlog convention"
  key_links:
    - from: "debug session frontmatter status"
      to: "audit.cjs scanDebugSessions"
      via: "status resolved or complete is skipped, and files in debug/resolved/ are never read (readdir is not recursive, and only files are scanned)"
      pattern: "^status: (resolved|complete)$"
    - from: "quick SUMMARY frontmatter status"
      to: "audit.cjs scanQuickTasks"
      via: "only the exact value complete counts as closed"
      pattern: "^status: complete$"
    - from: "deferred-items.md entry continuation line"
      to: "uat.cjs parseDeferredItems via extractGapEntryFields"
      via: "an indented status: resolved line inside the bullet entry (first status key wins)"
      pattern: "^  status: resolved$"
    - from: "ROADMAP.md ## Backlog"
      to: "complete-milestone.md backlog preservation awk (/^## Backlog/ to EOF)"
      via: "Backlog must be the last ## section, so the extraction to EOF captures only backlog content"
      pattern: "^## Backlog$"
---

<objective>
Close the 10 stale items that `gsd-tools query audit-open` reports before the v1.6 milestone close. The user decided each one (items A1 to A6 in the task brief). Every closure must rest on evidence already in the repo. Each change keeps its history: tracked files move with `git mv`, and edits only add or replace frontmatter fields and add notes. Nothing is deleted.

Purpose: `/gsd-complete-milestone` can then close v1.6 as a verified closeout without an override.
Output: four debug sessions resolved and moved to `debug/resolved/` (plus two EAM evidence JSON files). The debug knowledge base is excluded from the open list by frontmatter. Quick task nr2 is closed as superseded by 23-22. A ROADMAP backlog item 999.1 tracks the two timing-sensitive tests. The deferred-items entries are marked as moved to backlog. The final audit-open output goes in the SUMMARY.

Scope: planning and docs files only. No Rust source edits, no cargo commands, no worktrees.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@.claude/CLAUDE.md

Scanner rules (already read by the planner; re-read only if a verify step disagrees):
- `~/.claude/gsd-core/bin/lib/audit.cjs` `scanDebugSessions`: every `*.md` file directly in `.planning/debug/` counts as a session (no recursion, so `debug/resolved/` is ignored). A file is open unless its frontmatter `status` is `resolved` or `complete`, and a missing status reads as `unknown`. No rule excludes a file by name.
- `scanQuickTasks`: a quick task dir is open unless its `*-SUMMARY.md` frontmatter `status` is exactly `complete`.
- `scanDeferredItems` calls `uat.cjs` `parseDeferredItems`. These files have no `## Deferred Items` heading, so the whole file is scanned, and each top-level `- ` bullet is one entry. More deeply indented lines fold into the current entry. An entry is resolved only if a line in it, once trimmed, starts `status: resolved` (the first `status` key wins).
- Backlog convention: `~/.claude/gsd-core/workflows/add-backlog.md` puts a `## Backlog` section at the END of ROADMAP.md with `### Phase 999.x: ... (BACKLOG)` and creates `.planning/phases/999.x-slug/.gitkeep`. `phase.next-decimal 999` returns 999.1, and `project_code` is null, so the directory has no prefix. Milestone close keeps 999.x dirs and the Backlog section (milestone.cjs skips `^999`).

Evidence sources the tasks cite:
- `.planning/phases/22-transport-plan-pull-verify/22-15-SUMMARY.md`, `22-VERIFICATION.md`, `22-UAT.md`
- `.planning/phases/23-guarded-apply-rollback/23-VERIFICATION.md`, `23-22-SUMMARY.md`
- `.planning/milestones/v1.0-phases/07-ecosystem-interop-advanced-ops/07-05-SUMMARY.md`, `07-VERIFICATION-GAPS.md`, `07-UAT.md`
- `.planning/milestones/v1.1-phases/11-tag-bulk-transfer-xml-csv/11-07-SUMMARY.md`, `11-08-SUMMARY.md`, `11-VERIFICATION.md`, `11-UAT.md`
- `.planning/phases/24-redundant-pair/deferred-items.md`, `.planning/phases/25-guarded-project-deploy/deferred-items.md`
</context>

<tasks>

<task type="tracer">
  <name>Task 1 (tracer): close one debug session and one quick task end to end (A1, A2, A5), and confirm audit-open drops them</name>
  <files>.planning/debug/pulled-tree-cobranding-readback.md (moved to .planning/debug/resolved/pulled-tree-cobranding-readback.md), .planning/debug/knowledge-base.md, .planning/phases/22-transport-plan-pull-verify/22-UAT.md, .planning/quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/261018-nr2-SUMMARY.md, .planning/STATE.md</files>
  <action>
This task proves the closure mechanism for the debug and quick scanners first.

A1 (G-22-2, user decision A1): run `git mv .planning/debug/pulled-tree-cobranding-readback.md .planning/debug/resolved/pulled-tree-cobranding-readback.md`, then edit the file at its new path.
- Frontmatter: change `status: diagnosed` to `status: resolved`, set `updated: 2026-10-29`, and add `resolved: 2026-10-15` (the fix commit ba00845 and the passing UAT re-test are both dated 2026-10-15). Keep `trigger`, `created` and `goal`.
- In the existing `## Resolution` block, replace the empty `fix:` value with a double-quoted sentence. It says plan 22-15 made N1 symmetric: check_read_back null-strips the REST config as well as the file config before diff_exact and diff_subset, and diff_attributes strips both compared values. Cite the commits RED 90920e6, GREEN ba00845, guard tests 6a60a05, docs 4353824 and live gate bc6bb24 (pull_then_verify_is_green_live).
- Replace `files_changed: []` with the paths from `git show --name-only --format= ba00845 6a60a05 bc6bb24`. Keep only paths under crates/ and README.md, deduplicated.
- Keep the existing diagnosis `verification:` value as it is. Add a new key `fix_verification:` saying: 22-UAT.md test 2 re-test result pass on a fresh 8.3.6 rig on 2026-10-15 (verify on the unmodified pulled tree exit 0, read_back pass, a Bad probe still exit 6), and 22-VERIFICATION.md status passed with G-22-2 in re_verification.gaps_closed.
- Add `closed_by: "quick 261029-fyc, 2026-10-29, from the existing record"`.
- Before writing, confirm every cited fact. Run `git merge-base --is-ancestor` for each of the five commits against HEAD. Read 22-UAT.md test 2 for `result: pass`. Read the 22-VERIFICATION.md frontmatter for `status: passed` and the G-22-2 line under gaps_closed.
- Do not touch any other part of the file. That includes the Current Focus block and the doubled `## Symptoms` heading.

Also in Task 1, in `.planning/phases/22-transport-plan-pull-verify/22-UAT.md`, change the one `debug_session:` value on the G-22-2 gap entry (currently around line 59) to `.planning/debug/resolved/pulled-tree-cobranding-readback.md`, so the live link still resolves. Change nothing else in that file. Do not edit 22-15-PLAN.md; its references are historical plan text.

A2 (user decision A2): the scanner has no rule that excludes a file by name. It does skip a frontmatter status of resolved or complete, and the user's rule says to apply that kind of field. Prepend a frontmatter block to `.planning/debug/knowledge-base.md`, above the existing `# GSD Debug Knowledge Base` line:
- `status: complete`
- `kind: debug-knowledge-base`
- a double-quoted `note:` value saying this file is the resolved-sessions index, not a debug session. Explain that scanDebugSessions in gsd-core bin/lib/audit.cjs treats every .md file directly under .planning/debug as a session unless its status is resolved or complete, and has no rule that excludes by name, so status complete keeps the index off the open list.

Keep the note free of inner double quotes. Leave every existing line of the file unchanged and in place. No gsd-core code parses this file (the planner checked with grep), and the debugger only appends to it and reads it by keyword overlap, so the frontmatter is safe. Do not append knowledge-base entries for the sessions closed in this task or in Task 2. That was not requested.

A5 (user decision A5): in `261018-nr2-SUMMARY.md` frontmatter:
- change `status: incomplete` to `status: complete`
- add directly below it `original_status: incomplete`, then `superseded_by: 23-22`, then a double-quoted `closure_note:`. The note says: closed as superseded on 2026-10-29 by quick 261029-fyc; the sftp listing gap this task stopped on was fixed by plan 23-22 (UAT G1), whose five-gate live ssh run passed 5 of 5 on ign-lnx-836; see 23-VERIFICATION.md re_verification.gaps_closed and 23-22-SUMMARY.md.

In the body, insert one blockquote line directly under the H1 that says the same thing in a sentence. Do not change the H1 or any existing body text.

Before writing, confirm the facts:
- 23-VERIFICATION.md frontmatter has `status: passed` and a gaps_closed line naming UAT G1 and 23-22 with 5 of 5.
- 23-22-SUMMARY.md has `status: complete` and the five-gate run #2 row reading 5 of 5 PASS.

Then, in `.planning/STATE.md`, edit only the description cell of the `| 261018-nr2 |` row in the Quick Tasks Completed table. Replace its parenthetical with `(superseded by 23-22: the sftp listing gap was closed there; five-gate live ssh run 5 of 5 on ign-lnx-836)`. Leave the date, commit, directory cells and every other STATE.md line untouched.

Commit with explicit paths only: `node ~/.claude/gsd-core/bin/gsd-tools.cjs query commit "docs(quick-261029-fyc): resolve G-22-2 debug session, exclude debug KB index, close nr2 as superseded by 23-22" --files` followed by both the old and new pulled-tree paths, knowledge-base.md, 22-UAT.md, the nr2 SUMMARY and STATE.md. Never use git add -A or git add with a dot. Afterwards run `git show --name-status --format= HEAD` and confirm it lists only those paths. In particular, .planning/config.json, .planning/v1.6-MILESTONE-AUDIT.md, .planning/research/.cache and the repo-root .ps1 file must not be in the commit.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && R=.planning/debug/resolved/pulled-tree-cobranding-readback.md && Q=.planning/quick/261018-nr2-live-ssh-gate-for-the-linux-applyhost-co/261018-nr2-SUMMARY.md && test -f "$R" && test ! -e .planning/debug/pulled-tree-cobranding-readback.md && grep -q '^status: resolved$' "$R" && grep -q 'ba00845' "$R" && grep -q '^fix_verification:' "$R" && grep -q 'debug_session: .planning/debug/resolved/pulled-tree-cobranding-readback.md' .planning/phases/22-transport-plan-pull-verify/22-UAT.md && head -1 .planning/debug/knowledge-base.md | grep -qx -- '---' && grep -q '^status: complete$' .planning/debug/knowledge-base.md && grep -q '^# GSD Debug Knowledge Base$' .planning/debug/knowledge-base.md && grep -q '^status: complete$' "$Q" && grep -q '^original_status: incomplete$' "$Q" && grep -q '^superseded_by: 23-22$' "$Q" && grep -q '^| 261018-nr2 |.*superseded by 23-22' .planning/STATE.md && node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open --json | jq -e '([.items.debug_sessions[].slug] | (index("pulled-tree-cobranding-readback") == null and index("knowledge-base") == null)) and ([.items.quick_tasks[].slug] | index("261018-nr2-live-ssh-gate-for-the-linux-applyhost-co") == null)'</automated>
  </verify>
  <done>audit-open --json no longer lists pulled-tree-cobranding-readback, knowledge-base or 261018-nr2. The G-22-2 session is in debug/resolved/ with status resolved and its 22-15 citations. The 22-UAT.md link points at the new path. The KB is still in place, carrying frontmatter status complete. The nr2 SUMMARY reads complete, superseded_by 23-22, original_status incomplete. The STATE.md row says it was superseded by 23-22. One commit holds only the listed paths.</done>
</task>

<task type="auto">
  <name>Task 2: resolve the EAM 422 note and the two Phase 11 UAT sessions only on confirmed evidence (A3, A4)</name>
  <files>.planning/debug/eam-create-422.md, .planning/debug/eam-history-raw.json, .planning/debug/eam-working-definition.json, .planning/debug/loss-gate-hint-mismatch.md, .planning/debug/udt-type-fact-falsified.md (all moved to .planning/debug/resolved/)</files>
  <action>
Run the evidence checks first. They are read-only: grep, git and reading docs, with no cargo. Write nothing for an item until its checks pass.

A3 (EAM create 422, user decision A3). Confirm each of these:
- `git merge-base --is-ancestor 4bdbb81 HEAD` and `git merge-base --is-ancestor 6e8236f HEAD` both succeed.
- `crates/ignition-core/src/actions/eam.rs` still defines `fn compose_task_definition`, and `crates/ignition-core/src/client/classify.rs` still defines `fn is_config_resource_url`.
- `07-VERIFICATION-GAPS.md` has `status: passed`, truth 3 marked VERIFIED, and the gap row `3 — eam task new 422` marked CLOSED.
- `07-05-SUMMARY.md` records the live create of uat-backup-demo exiting 0 on gateway A (:9088).
- The `07-UAT.md` round 2 Summary shows passed 5 and issues 0.
- Note one inconsistency for the SUMMARY: in `07-UAT.md` round 2, the test 2 line reads `result: [pending]` while the round's Summary counts 5 of 5 passed. Do not edit that archived file.

If all of that holds:
- `git mv` all three files (eam-create-422.md, eam-history-raw.json, eam-working-definition.json) into `.planning/debug/resolved/`. Leave the two JSON files' content unchanged.
- Prepend a frontmatter block to `resolved/eam-create-422.md` with:
  - `status: resolved`
  - a double-quoted `trigger:` saying 07-UAT round 1 test 6: ign eam task new uat-backup-demo eam_backup got HTTP 422 'Settings cannot be null' from the 8.3.3 gateway, surfaced as internal_error (single quotes only inside)
  - `created: 2026-08-29`
  - `updated: 2026-10-29`
  - `resolved: 2026-08-29`
  - `goal: find_root_cause_only`
  - `evidence_files: [eam-working-definition.json, eam-history-raw.json]`
- Append a `## Resolution` section at the end with these keys:
  - root_cause: the create body put targetGateways in config.profile and sent no config.settings, which the gateway requires (null gives 422). Secondary: classify had no arm for a config-resource 4xx, so the 422 surfaced as internal_error.
  - fix: plan 07-05 Task 2, commit 4bdbb81. compose_task_definition splits profile from settings, and zero targets default to _controller. A path-scoped 422 arm (is_config_resource_url) maps the gateway messages to invalid_input, exit 2.
  - verification: the facts confirmed above.
  - companion_evidence: eam-working-definition.json is the working definition read back, the basis for the fix. eam-history-raw.json is the raw eam history capture behind the 07-UAT round 1 UUID taskId gap, closed by 07-05 Task 1 commit 6e8236f.
  - files_changed: from `git show --name-only --format= 4bdbb81`.
  - caveat: the 07-UAT test 2 pending line above.
  - closed_by: quick 261029-fyc on 2026-10-29.
- Keep every existing body line.

A4 (Phase 11 UAT sessions, user decision A4: CONFIRM before resolving).

For loss-gate-hint-mismatch.md, confirm each of these:
- `git merge-base --is-ancestor` succeeds for 459307d and cbfc022.
- `crates/ignition-core/src/error.rs` defines `LOSS_GATE_REFUSAL_REASON_PREFIX`, and its InvalidInput hint arm has a `reason.starts_with(LOSS_GATE_REFUSAL_REASON_PREFIX)` branch that returns the re-run-with---yes hint.
- `crates/ignition-cli/tests/contract_tags.rs` references the prefix (the drift-guard pin).
- The `11-VERIFICATION.md` frontmatter has `status: passed` and a gaps_closed line for UAT test 3 (11-07).

For udt-type-fact-falsified.md, confirm each of these:
- `git merge-base --is-ancestor` succeeds for 8df05fb and d779d7c.
- `crates/ignition-core/src/actions/tag_loss.rs` near `XML_UDT_TYPE_DEFINITION` now states that provider-root imports land the definitions in `[provider]_types_` and scopes the refusal to a folder basePath.
- `11-VERIFICATION.md` has a gaps_closed line for UAT test 4 (11-08).

Note one caveat for both files in the SUMMARY: the archived `11-UAT.md` still has `status: diagnosed` with tests 3 and 4 recorded as issue, and no live re-test was recorded after 11-07 and 11-08. Their closure rests on 11-VERIFICATION.md re-verification, the contract tests and the code. Do not edit the archived UAT.

For each A4 file whose checks pass:
- `git mv` it into `.planning/debug/resolved/`.
- Prepend frontmatter: `status: resolved`, a double-quoted `trigger:` (loss gate: UAT Phase 11 test 3, the loss-gate refusal printed the generic file-read hint; udt: UAT Phase 11 test 4, the xml_udt_type_definition fact text was falsified live), `created: 2026-09-14`, `updated: 2026-10-29`, `resolved: 2026-09-14`, `goal: find_root_cause_only`.
- Append a `## Resolution` section with fix, verification, files_changed (from `git show --name-only --format=` on the fix commits), caveat and closed_by keys. Loss gate fix: plan 11-07, commits 459307d and cbfc022, prefix sentinel in CoreError::hint. UDT fix: plan 11-08, commits 8df05fb and d779d7c, reworded fact plus mirrored README, STATE and captures corrections.
- Keep every existing body line.

If any check FAILS for an A4 file, leave that file exactly where it is, unedited, and record in the SUMMARY which check failed and that the file is flagged for the user. Do not mark it resolved.

Commit with explicit paths only, old and new for every move: `node ~/.claude/gsd-core/bin/gsd-tools.cjs query commit "docs(quick-261029-fyc): resolve EAM create 422 (07-05) and Phase 11 UAT debug sessions (11-07, 11-08)" --files ...`. Then confirm with `git show --name-status --format= HEAD` that only these debug paths are in the commit.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && D=.planning/debug/resolved && test -f $D/eam-create-422.md && test -f $D/eam-history-raw.json && test -f $D/eam-working-definition.json && test -f $D/loss-gate-hint-mismatch.md && test -f $D/udt-type-fact-falsified.md && test "$(/bin/ls .planning/debug | tr '\n' ' ')" = "knowledge-base.md resolved " && for f in eam-create-422 loss-gate-hint-mismatch udt-type-fact-falsified; do head -1 $D/$f.md | grep -qx -- '---' && grep -q '^status: resolved$' $D/$f.md && grep -q '^## Resolution' $D/$f.md || exit 1; done && grep -q 4bdbb81 $D/eam-create-422.md && grep -q 459307d $D/loss-gate-hint-mismatch.md && grep -q 8df05fb $D/udt-type-fact-falsified.md && test -z "$(git diff --name-only a9cb111 -- crates)" && node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open --json | jq -e '.counts.debug_sessions == 0'</automated>
  </verify>
  <done>All three EAM files and both Phase 11 sessions are in debug/resolved/. Each .md has status resolved frontmatter and a Resolution section citing its fixing plan and commits. Only knowledge-base.md and resolved/ remain at the debug top level. audit-open reports 0 debug sessions, and nothing under crates/ changed. If an A4 check failed instead, that file is untouched, the verify reports it, and the SUMMARY flags it for the user with the failed check named.</done>
</task>

<task type="auto">
  <name>Task 3: add backlog item 999.1 for the timing-sensitive tests, mark the deferred entries as moved to backlog, and run the final audit (A6)</name>
  <files>.planning/ROADMAP.md, .planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep, .planning/phases/24-redundant-pair/deferred-items.md, .planning/phases/25-guarded-project-deploy/deferred-items.md</files>
  <action>
A6 (user decision A6). Follow the add-backlog convention (`~/.claude/gsd-core/workflows/add-backlog.md`).

First run `node ~/.claude/gsd-core/bin/gsd-tools.cjs query phase.next-decimal 999 --raw`. It must print 999.1; if it prints anything else, use that number everywhere below in place of 999.1.

Then use Edit (never Write) on `.planning/ROADMAP.md`. Append after the last line of the file, which is the italic v1.6 footer line ending "parked behind it*". The Backlog section must be the last `## ` section in the file, because complete-milestone extracts everything from `## Backlog` to the end of the file. Append:
- a blank line, then `## Backlog`, a blank line, then `### Phase 999.1: Stabilize timing-sensitive tests (BACKLOG)`, then a blank line
- `**Goal:**` Two tests give a different result under load or alone than in a normal full run. Make each one deterministic, or bound its timing, so a full `cargo test --workspace` run and a single-test run agree.
- `**Requirements:** TBD`
- `**Plans:** 0 plans`
- a `Tests:` list with two bullets:
  - `crates/ignition-core/tests/live_gateway.rs::guard_drop_during_unwind_inside_runtime_still_cleans_up` fails under heavy machine load. It failed in the 24-04 and 24-05 full workspace runs and in three single reruns at load average 36 to 44: the Drop-path cleanup find and delete did not reach the wiremock server within 10 s (the assertion at line 1069). It passes alone at normal load. Source: `.planning/phases/24-redundant-pair/deferred-items.md`.
  - `deploy::transport::process::tests::timeout_kills_child` in `crates/ignition-core/src/deploy/transport/process.rs` fails when run alone in the agent sandbox and passes in the full run (2847 passed). The suspected cause is sandbox timing or process-group behaviour. Source: `.planning/phases/25-guarded-project-deploy/deferred-items.md`.
- then `Plans:` and `- [ ] TBD (promote with /gsd-review-backlog when ready)`.

Do not touch the Progress table or any phase entry.

After the ROADMAP edit, create the empty file `.planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep` with the Write tool. Use exactly that directory name: project_code is null, so there is no prefix. Do not put a literal null prefix on it.

Then mark the three deferred entries without deleting any line (insertions only):
- In `.planning/phases/24-redundant-pair/deferred-items.md`, directly below the line-5 bullet (the 24-04 entry, before the blank line) and directly below the line-9 bullet (the 24-05 entry), insert two lines, each indented by exactly two spaces. The first reads `status: resolved`. The second is a `resolution:` line saying: moved to backlog Phase 999.1 (ROADMAP.md ## Backlog) by quick 261029-fyc on 2026-10-29; the test is not fixed and stays timing-sensitive.
- In `.planning/phases/25-guarded-project-deploy/deferred-items.md`, insert the same two indented lines directly below the single 25-13 bullet.

The parser reads a two-space-indented line as part of the bullet's entry, and the first `status` key wins. No original bullet starts with a key-colon shape, so the inserted status line is the one it reads.

Commit with explicit paths: `node ~/.claude/gsd-core/bin/gsd-tools.cjs query commit "docs(quick-261029-fyc): backlog 999.1 for timing-sensitive tests; mark phase 24/25 deferred items moved to backlog" --files .planning/ROADMAP.md .planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep .planning/phases/24-redundant-pair/deferred-items.md .planning/phases/25-guarded-project-deploy/deferred-items.md`. Then confirm the commit's name-status lists only those four paths.

Final check (part of A6 and required by the brief): run `node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open` and capture the full human-readable output.

Before the SUMMARY exists, the only item expected to remain is quick task `261029-fyc-close-stale-v1-6-pre-close-audit-items [missing]`. That item clears once the SUMMARY is written with frontmatter `status: complete`. An A4 file flagged in Task 2 would also still be listed.

When writing `261029-fyc-SUMMARY.md`:
- Use frontmatter `status: complete`.
- Re-run audit-open after writing it, and Edit the SUMMARY to paste that final output under a `## Final audit-open` heading, listing every item still open.
- Also record in the SUMMARY:
  - A2's outcome: excluded by frontmatter status complete, citing the scanDebugSessions rule. It is no longer a false positive.
  - The 07-UAT and 11-UAT caveats from Task 2.
  - That the archived docs (07-UAT.md, 07-05-PLAN.md, 11-UAT.md, 11-08-SUMMARY.md) and 22-15-PLAN.md still cite the old .planning/debug paths, left unedited as history.
  - Any A4 file flagged for the user.
  </action>
  <verify>
    <automated>cd /Users/pmannion/whiskeyhouse/ignition-cli && R=.planning/ROADMAP.md && P24=.planning/phases/24-redundant-pair/deferred-items.md && P25=.planning/phases/25-guarded-project-deploy/deferred-items.md && test "$(grep -c '^## Backlog$' $R)" = 1 && test "$(grep -c '^### Phase 999.1: ' $R)" = 1 && test "$(sed -n '/^## Backlog$/,$p' $R | grep -c '^## ')" = 1 && sed -n '/^## Backlog$/,$p' $R | grep -q 'guard_drop_during_unwind_inside_runtime_still_cleans_up' && sed -n '/^## Backlog$/,$p' $R | grep -q 'timeout_kills_child' && test -f .planning/phases/999.1-stabilize-timing-sensitive-tests/.gitkeep && test "$(node ~/.claude/gsd-core/bin/gsd-tools.cjs query phase.next-decimal 999 --raw)" = 999.2 && test "$(grep -c '^  status: resolved$' $P24)" = 2 && test "$(grep -c '^  status: resolved$' $P25)" = 1 && test "$(git diff --numstat a9cb111 -- $P24 $P25 $R | awk '{s+=$2} END {print s+0}')" = 0 && test -z "$(git diff --name-only a9cb111 -- crates)" && node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open --json | jq -e '.counts.deferred_items == 0 and .counts.debug_sessions == 0 and ([.items.quick_tasks[].slug] | map(select(. != "261029-fyc-close-stale-v1-6-pre-close-audit-items")) | length == 0)'</automated>
  </verify>
  <done>ROADMAP.md ends with a ## Backlog section holding Phase 999.1, which names both tests and their sources, and the Progress table is unchanged. The 999.1 directory and its .gitkeep exist, and next-decimal returns 999.2. Both deferred-items files keep every original line, and each of the three entries carries status resolved plus a resolution line pointing at 999.1. audit-open reports 0 deferred items and 0 debug sessions, and the only quick task left is this one, which clears once its SUMMARY is written with status complete. The SUMMARY holds the final audit-open output from the run after the SUMMARY was written.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| planning docs to milestone close gate | audit-open trusts the frontmatter status fields. Whatever this task writes decides whether v1.6 closes as verified. |
| working tree to git history | Commits carry these planning files. Unrelated modified and untracked files in the tree must not ride along. |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-fyc-01 | Tampering | debug session status fields (A1, A3, A4) | high | mitigate | Each status: resolved is written only after its evidence checks pass: git merge-base --is-ancestor on the fix commits, a grep of the code each session points at, and the verification doc lines. A4 is conditional, and a failed check leaves the file open and flagged. This keeps an unfixed defect from being hidden at milestone close. |
| T-fyc-02 | Repudiation | history of moved and edited records | medium | mitigate | git mv keeps the rename history. Frontmatter and Resolution edits only add fields. nr2 keeps original_status: incomplete. The deferred-items edits are insert-only, gated by git numstat showing 0 deleted lines against a9cb111. |
| T-fyc-03 | Information Disclosure | commits staging unrelated files (repo-root svc-ignition-deploy .ps1, .planning/config.json, v1.6-MILESTONE-AUDIT.md, research cache) | high | mitigate | Every commit uses gsd-tools commit with an explicit --files list. git add -A and git add with a dot are forbidden. After each commit, git show --name-status HEAD must list only the intended paths. |
| T-fyc-04 | Tampering | knowledge-base.md frontmatter status: complete | low | accept | The field only keeps the index off the open list. No gsd-core code parses the file (checked with grep), and the debugger only appends to it, so the field cannot hide a real session. |
</threat_model>

<verification>
- `node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open --json | jq '.counts'` shows debug_sessions 0, deferred_items 0, quick_tasks 0 (after the SUMMARY with status complete exists), and total 0. A count above 0 is acceptable only for an A4 file flagged with a named failed check.
- `test -z "$(git diff --name-only a9cb111 -- crates)"`: no source change.
- `git log --name-status a9cb111..HEAD` shows only the paths in files_modified plus this task's PLAN and SUMMARY.
- `node ~/.claude/gsd-core/bin/gsd-tools.cjs query audit-open` (the human-readable run) is pasted into the SUMMARY.
</verification>

<success_criteria>
- All 10 audit items from the brief are closed per decisions A1 to A6, or explicitly flagged (A4 only) with evidence.
- Every closure cites in-repo evidence (plan, commit hash, verification or UAT line).
- No history was deleted, no Rust source changed, and nothing was built.
- The final audit-open output is recorded in the SUMMARY.
</success_criteria>

<output>
Create `.planning/quick/261029-fyc-close-stale-v1-6-pre-close-audit-items/261029-fyc-SUMMARY.md` with frontmatter `status: complete`. Then re-run audit-open and paste the final output under `## Final audit-open`.
</output>
