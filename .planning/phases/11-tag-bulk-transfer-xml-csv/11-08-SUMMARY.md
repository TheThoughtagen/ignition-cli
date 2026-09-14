---
phase: 11-tag-bulk-transfer-xml-csv
plan: 08
subsystem: tags
tags: [rust, loss-report, udt, documentation-correction, advisory-fact, ignition]

# Dependency graph
requires:
  - phase: 11-tag-bulk-transfer-xml-csv
    provides: "11-01 probe-3(b) capture (the scope-wrong refusal), 11-03 loss-fact machinery + codes contract, 11-06 gate with Probe-5 correction precedent (commit 361d6b4)"
  - phase: 11-tag-bulk-transfer-xml-csv
    provides: ".planning/debug/udt-type-fact-falsified.md — root-cause session (live-proven ×3 on 8.3.6 + UAT)"
provides:
  - "Corrected xml_udt_type_definition fact detail stating live truth: provider-root imports DO land UdtType definitions, routed to [provider]_types_/Name; the verbatim refusal is folder-basePath-only; real caveats named (cross-provider udtParentType stays source-qualified; parameter overrides drop on unresolvable target type)"
  - "Mirrored scoped corrections in README (TAGS-12 loss-gate section), STATE.md (two dated [CORRECTED 2026-09-14, 11-08] markers, originals preserved), and 11-LIVE-CAPTURES.md (SCOPE-CORRECTED + SCOPE NOTE in the Probe-5 precedent format)"
  - "Closed UAT test 4 (Phase 11) — fact firing and gate behavior byte-identical, only text corrected"
affects: [phase-11-uat-reverification, udt-imports, loss-report-contract, agent-documentation]

# Tech tracking
tech-stack:
  added: []
  patterns: ["dated correction markers (provenance over silent rewrite) applied to STATE.md decisions and captures doc", "scoped fact text: unconditional firing, conditioned claims"]

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/tag_loss.rs
    - crates/ignition-cli/tests/e2e_webdev.rs
    - README.md
    - .planning/STATE.md
    - .planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md

key-decisions:
  - "Fact FIRES unconditionally — detection is correct, the transfer genuinely is not lossless (routing + cross-provider caveats are real losses); only the text was falsified"
  - "Corrections are dated and appended, originals preserved — provenance over silent rewrite (STATE.md 134/143, captures doc Probe-5 precedent)"
  - "codes::XML_UDT_TYPE_DEFINITION const name untouched — codes are stable pub consts, never renamed"

patterns-established:
  - "Doc-correction pattern: bold bracketed dated marker + corrected scoped truth, original capture text preserved in place"

# Metrics
duration: 12 min
completed: 2026-09-14
---

# Phase 11 Plan 08: UDT Fact Correction Summary

**Corrected the live-falsified xml_udt_type_definition loss fact (provider-root imports DO land UdtType definitions in _types_; refusal is folder-basePath-only) in tag_loss.rs with mirrored dated corrections in README, STATE.md, and the captures doc — zero behavior change**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-14T14:27:47Z
- **Completed:** 2026-09-14T14:40:09Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- UAT test 4 (Phase 11) closed: the advisory fact now states the live, conditioned truth with all three required elements — (a) definitions DO import and route to `[provider]_types_/Name`, (b) the verbatim refusal is folder-basePath-only and unreachable via this command, (c) real caveats (source-qualified udtParentType; parameter overrides drop on unresolvable target type, 8.3.3 / 8.3.6 preserves)
- Correction mirrored across every live artifact: README TAGS-12 loss-gate section, two dated STATE.md decision markers, and both captures-doc scope notes
- Firing behavior, gate behavior, exit slugs, and all detection pins byte-identical to before — `cargo test --workspace` green (54 binaries), including the frozen `readme_exit_table_agreement` and the untouched `has_fact(XML_UDT_TYPE_DEFINITION)` pins

## Task Commits

Each task was committed atomically:

1. **Task 1: Reword the falsified fact (tag_loss.rs)** — `8df05fb` (fix)
2. **Task 2: Mirror the correction (README, STATE.md, captures doc)** — `d779d7c` (docs)

_Note: commit `459307d` (feat(11-07)) interleaves between the two — plan 11-07 executed concurrently in this working tree; no file overlap._

## Files Created/Modified
- `crates/ignition-core/src/actions/tag_loss.rs` — fact detail, codes doc comment, test comment reworded to scoped truth (const name + detection untouched)
- `crates/ignition-cli/tests/e2e_webdev.rs` — stale comment repeating the falsified claim corrected (comment only; the test pins the code name, which is unchanged)
- `README.md` — TAGS-12 loss-gate clause replaced with scoped truth (line-966 fleet-destructive wording untouched — unrelated, true)
- `.planning/STATE.md` — dated `[CORRECTED 2026-09-14, 11-08]` markers appended to the 11-01 and 11-03 decision lines, originals preserved
- `.planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md` — SCOPE-CORRECTED paragraph on Probe 3 finding (b)(1) + SCOPE NOTE on oracle item 4, mirroring the Probe-5 correction precedent

## Decisions Made
- Fact fires unconditionally (detection correct and useful — the transfer genuinely isn't lossless); only WHAT it claims was fixed
- Corrections dated and appended rather than rewritten — provenance over silent rewrite, per STATE.md:131 and the Probe-5 precedent (commit 361d6b4)
- `codes::XML_UDT_TYPE_DEFINITION` name byte-identical — codes are stable consts, never renamed

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected stale comment repeating the falsified claim (e2e_webdev.rs:2549)**
- **Found during:** Task 1 (gate 2 grep)
- **Issue:** Plan's gate 2 expected exactly the 2 known CSV capture asserts; the sweep surfaced a third hit — a comment in the loss-gate e2e test reading "importTags refuses those outright, the captured verbatim refusal," repeating the falsified unconditional claim. The plan's gate instruction covers this: "any OTHER hit … update THAT pin to the new text in this task and note it in the SUMMARY."
- **Fix:** Reworded the comment to the scoped truth (definitions DO land via the provider-root import, routed to _types_; the transfer is not lossless). Comment-only — the test's actual pins (`[xml_udt_type_definition]` code name, exit 2, pre-resolution) are unchanged and green.
- **Files modified:** crates/ignition-cli/tests/e2e_webdev.rs
- **Verification:** gate-2 sweep now returns exactly the 2 expected CSV asserts; e2e compile green
- **Committed in:** 8df05fb (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 stale comment caught by the plan's own grep gate)
**Impact on plan:** None — comment-only fix directed by the plan's gate; no scope creep.

## Issues Encountered
- Plan 11-07 executed concurrently in this working tree; its mid-flight edit to `error.rs` transiently broke the `cargo test -p ignition-core` compile (missing-import race). Resolved by waiting ~30s and retrying — 11-07's commit (`459307d`) landed between the two Task commits with zero file overlap. No action required.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- UAT gap closure for this plan complete; the phase re-verification re-runs UAT tests 3/4 against the corrected fact text (already live-proven — no new live-rig work was required here)
- Plan 11-07 (loss-gate hint) was executing concurrently — its SUMMARY lands separately; phase wrap-up should confirm both 11-07 and 11-08 summaries present
- Fact text, README contract, and planning record now agree on the same conditioned truth — the agent-facing contract no longer misleads on UdtType-bearing imports

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-14*

## Self-Check: PASSED

All 6 files exist on disk; both task commits (8df05fb, d779d7c) verified in git log.
