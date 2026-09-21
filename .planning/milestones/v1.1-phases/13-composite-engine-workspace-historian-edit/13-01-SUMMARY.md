---
phase: 13-composite-engine-workspace-historian-edit
plan: 01
subsystem: historian
tags: [ignition, historian, tag-history, spike, live-capture, tagconfig, trial-rigs]

# Dependency graph
requires:
  - phase: 05-webdev-backend-tag-operations
    provides: tagConfig route (the tag write path), 05-06 spike artifact (the historicalProvider candidate to test), live-run discipline
  - phase: 11-tag-bulk-transfer-xml-csv
    provides: trial-rig ops recipe (commission/token/unique-names/2h windows), 11-RIG-NOTES procedures
provides:
  - SPIKE VERDICT: closure — the tag↔historian binding recipe live-proven via the CLI tag write path on BOTH trial rigs (8.3.3 + 8.3.6)
  - The exact working field set [historyEnabled, historyProvider, sampleMode] as 13-04's TAGS-14 implementation contract
  - Wire truth: historical-tag-group key NOT required (gateway default group binds at sampleMode=TagGroup)
  - Wire truth: tags config edit REPLACES the whole node (dataType/defaultValue dropped) — recipe must carry full node shape
  - Corrected provider-API routes (find/list/create/delete) live-proven; e2e harness historian delete identified as silent no-op
  - 13-LIVE-CAPTURES.md spike record + committed before/after/diff capture artifacts
affects: [13-04 TAGS-14 implementation, e2e_webdev.rs harness route fix, SC-4 done-state]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "write-path replay as replacement oracle when a human-only oracle is declined"
    - "config-read-back + data-probe two-level binding proof (keys landed AND data rows returned)"
    - "resources-API generic mounts: /resources/find/{module}/{type}/{name}, /resources/list/{module}/{type}, delete /resources/{module}/{type}/{name}/{signature}"

key-files:
  created:
    - .planning/phases/13-composite-engine-workspace-historian-edit/13-LIVE-CAPTURES.md
    - .planning/phases/13-composite-engine-workspace-historian-edit/artifacts/config-after-836.json
    - .planning/phases/13-composite-engine-workspace-historian-edit/artifacts/config-after-833.json
    - .planning/phases/13-composite-engine-workspace-historian-edit/artifacts/replay-diff-836.txt
    - .planning/phases/13-composite-engine-workspace-historian-edit/artifacts/replay-diff-833.txt
  modified:
    - .planning/phases/13-composite-engine-workspace-historian-edit/13-RIG-NOTES.md

key-decisions:
  - "Designer-diff oracle ABORTED by user decision 2026-09-15 ('we can assume it works') — replaced by a live write-path replay using the research hypothesis field set; before-captures (0080dd2) remain pre-binding ground truth"
  - "Spike verdict: CLOSURE — field set [historyEnabled, historyProvider, sampleMode] landed and read back on both rigs, with written value 44 proven in history on both"
  - "historicalScanclass never pre-committed: the historical-tag-group key is not needed for a functional binding (gateway default group used at sampleMode=TagGroup)"
  - "e2e harness historian delete route correction deferred to 13-04 (zero product code in this plan)"

patterns-established:
  - "Replacement-oracle pattern: when a human-only capture step is declined, replay the write path against the hypothesis field set and let config read-back + data probe name the truth"
  - "Both-rig historian rule honored: identical replay + data proof on 8.3.6 AND 8.3.3"

# Metrics
duration: 15min (closing session 02:07–02:22Z; plan spanned 2026-09-14 21:46Z staging → 2026-09-15 02:22Z close)
completed: 2026-09-15
---

# Phase 13 Plan 01: Historian Binding Spike Summary

**Write-path replay proved the historian binding recipe LIVE on both trial rigs — field set [historyEnabled, historyProvider, sampleMode] landed, read back, and returned written values from history — replacing the user-declined Designer-diff oracle and answering 05-06's `historicalProvider` typo question with evidence.**

## Performance

- **Duration:** 15 min (closing session: replay → verdict → cleanup → teardown → docs); full plan staged 2026-09-14 21:46Z, closed 2026-09-15 02:22Z
- **Started:** 2026-09-15T02:07:41Z (session); Task 1 staged 2026-09-14T21:46:32Z
- **Completed:** 2026-09-15T02:22Z
- **Tasks:** 3 (Task 1 done prior session `0080dd2`; Task 2 SKIPPED by user decision; Task 3 executed modified)
- **Files modified:** 6 this session (1 created doc, 1 modified doc, 4 artifacts)

## Accomplishments

- **Closure verdict recorded:** `SPIKE VERDICT: closure — field set: [historyEnabled, historyProvider, sampleMode]` — binding landed AND read back verbatim on both rigs, and the historian returned DATA rows containing the written value 44 on both (rig A first query; rig B on re-query after its scan interval).
- **OQ1 answered with rig evidence:** `historyProvider` (doc-corrected) is the real key — it landed and bound. 05-06's `historicalProvider` is retired as the typo, not inferred.
- **OQ4 answered:** binding state lives IN the tag config (read-back proof); the provider record carries only storage settings — no out-of-band per-tag provider state exists for InternalHistorian.
- **Bonus wire truths:** the historical-tag-group key is unnecessary for a functional binding (gateway default group); `tags config edit` replaces the whole node (dataType/defaultValue dropped) so 13-04's recipe must send the full node shape.
- **Corrected the provider-API delete chain live:** found the working find/list/delete routes after the plan-recorded find URL 404'd, used them for verified cleanup, and identified the e2e harness's historian delete as a long-silent no-op (recorded follow-up).
- **Clean exit:** spike tags deleted, providers deleted and verified gone, both rigs torn down (zero ign-p13* containers/volumes), window END 02:15:47Z inside the re-armed budget.

## Task Commits

Each task was committed atomically:

1. **Task 1: Stage both trial rigs** — `0080dd2` (chore) — prior session
2. **Task 2: Designer step** — SKIPPED by user decision 2026-09-15 (no Designer capture; no commit)
3. **Task 3 (modified): Write-path replay + verdict + cleanup** — `c8900c8` (docs: spike record + artifacts), `2280306` (docs: rig cleanup/teardown record)

**Plan metadata:** see final docs commit.

## Files Created/Modified
- `.planning/phases/13-composite-engine-workspace-historian-edit/13-LIVE-CAPTURES.md` — the spike record + verdict + field-set table (13-04's contract)
- `.planning/phases/13-composite-engine-workspace-historian-edit/13-RIG-NOTES.md` — window ends, corrected route finding, cleanup + teardown record
- `artifacts/config-after-{836,833}.json` — replay read-backs (the binding-landed evidence)
- `artifacts/replay-diff-{836,833}.txt` — machine-diffed before→after (jq -S, never hand-edited)

## Decisions Made
- **Aborted Designer branch → write-path replay (user decision 2026-09-15):** the Designer step was declined ("we can assume it works"); the modified Task 3 replaced the Designer-diff oracle with a live replay of the research hypothesis field set. Recorded in 13-LIVE-CAPTURES.md §Aborted Designer branch.
- **One pass, no guessed keys:** only confidently-named research-checklist keys were sent; the historical-group key was never guessed, and the data probe then proved it wasn't needed.
- **Zero product code preserved:** the e2e harness route bug is RECORDED for 13-04, not fixed here (plan's zero-code rule).

## Deviations from Plan

### Plan Modifications (user-directed, not auto-fixes)

**1. Task 2 (Designer checkpoint) skipped by user decision**
- **Decision:** user declined the Designer capture 2026-09-15 ("we can assume it works")
- **Effect:** the Designer-diff oracle artifacts (`designer-diff-{836,833}.txt`) do not exist; Task 3 was executed in modified form per the orchestrator's instructions (replay oracle + honest verdict + cleanup)
- **Ground truth preserved:** Task 1's committed before-captures remain the pre-binding baseline; both tags were re-verified byte-identical before the replay

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan-recorded provider find route is malformed (recorded, fix deferred)**
- **Found during:** Task 3 cleanup
- **Issue:** `GET …/resources/com.inductiveautomation.historian/historian-provider/find/{name}` (e2e_webdev.rs:899 shape) 404s "No route match" on both rigs — and the harness treats non-200 as "already gone", making its historian DELETE a silent no-op all along
- **Fix (this plan):** discovered and live-proved the correct generic mounts — find `/resources/find/{module}/{type}/{name}`, list `/resources/list/{module}/{type}`, delete `/resources/{module}/{type}/{name}/{signature}` — used them for verified cleanup; recorded the harness correction as a 13-04 follow-up (zero product code rule)
- **Files modified:** 13-LIVE-CAPTURES.md, 13-RIG-NOTES.md
- **Verification:** find → 200 with signature pre-delete; find → 404 + list → 0 items post-delete on both rigs
- **Committed in:** c8900c8 / 2280306

**2. [Rule 1 - Bug] `tags config edit` drops unmentioned node keys (documented side effect)**
- **Found during:** Task 3 replay read-back
- **Issue:** whole-node replace semantics silently removed `dataType`/`defaultValue` and reset `value` to null
- **Fix:** documented in 13-LIVE-CAPTURES.md — 13-04's closure recipe must carry the complete node shape in one edit
- **Committed in:** c8900c8

---

**Total deviations:** 2 recorded findings (1 deferred bug, 1 documented side effect) + 1 user-directed plan modification.
**Impact on plan:** The user decision REMOVED the mandated oracle; the replacement oracle produced a strictly stronger result (binding proven live + data flowing on both rigs). No scope creep; zero product code.

## Issues Encountered
- The original capture.sh was unusable for a replay (diffs before/after of a Designer write) — wrote replay.sh/cleanup.sh instead (paths recorded in 13-RIG-NOTES.md, outside the repo).
- Rig B's first history query preceded its tag-group scan (only the initial 0 row); one re-query captured the written 44 — timing artifact, recorded honestly.

## User Setup Required

None — no external service configuration required. (Trial rigs were ephemeral; tokens ride /tmp outside the repo.)

## Next Phase Readiness
- **13-04 (TAGS-14) has its implementation contract:** the pinned field-set table + the full-node-shape edit caveat + the live gate recipe (provision historian → 3-key edit → write → query → expect data rows). 13-04's own live gate re-proves the recipe.
- **13-04 should also fold in:** the corrected e2e harness find/delete URLs (silent no-op fix).
- **SC-4 done-state:** closure branch selected — no longer loose.
- Phase 13 continues with plans 13-03…13-08 (workspace/edit slices unaffected by this spike).

---
*Phase: 13-composite-engine-workspace-historian-edit*
*Completed: 2026-09-15*

## Self-Check: PASSED

- All 6 capture artifacts exist on disk (before ×2 committed Task 1; after ×2 + replay-diff ×2 this session)
- Commits verified: `0080dd2` (Task 1), `c8900c8` (record), `2280306` (cleanup/notes)
- Verdict line present exactly once in 13-LIVE-CAPTURES.md
- `historicalScanclass` absent from every artifact (no-guess rule honored; the one captures-doc mention is prose stating it was never pre-committed)
- Zero tracked product-code changes (`git status` clean apart from pre-existing untracked scratch files)
- Both rigs confirmed removed (0 ign-p13* containers, 0 volumes)
