---
phase: 11-tag-bulk-transfer-xml-csv
plan: 01
subsystem: research-captures
tags: [ignition-8.3, exportTags, importTags, tag-xml, legacy-csv, scriptExec, webdev-routes, live-capture, fidelity-oracle]

# Dependency graph
requires:
  - phase: 09-agent-surface-api-diagnostics
    provides: rig recipe (headless commissioning + token provisioning + scriptExec secret deploy) and the 07-06 RpcContext constraint this plan tests
  - phase: 05-webdev-backend-tag-operations
    provides: the live-proven kwargs-only exportTags return-string form and the deployed 1.1.0 route bundle
provides:
  - "11-LIVE-CAPTURES.md: verbatim answers to all five roadmap-mandated probes (exportTags+xml form, importTags wire behavior, UDT XML byte shape + determinism oracle, mixed-parent corruption, legacy CSV coverage table) from both 8.3.6 and 8.3.3"
  - "artifacts/udt-multilevel.xml: real captured multi-level UDT XML bytes (2218 bytes, sha256 11ed1528…) — the 11-03 scan fixture the roadmap flag requires"
  - "Named fidelity-oracle strategy binding 11-06's live gate (transport sha256 / unchanged-subtree byte-identity / order-normalized structural round-trip; UdtType-bearing files excluded)"
  - "scriptExec exec-action scope semantics live-proven for the first time (fresh globals; `import system` bridge)"
affects: [11-02-route-design, 11-03-scan-module, 11-04-csv-generator, 11-05-readme, 11-06-live-gate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "capture-first probe discipline: plan hypotheses tested verbatim on rigs, corrections locked in 11-LIVE-CAPTURES.md for later plans to cite"
    - "jar-archaeology ground truth: TagCSVImporter PROP_COLUMNS static read from inside the container anchors the CSV vocabulary"

key-files:
  created:
    - .planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md
    - .planning/phases/11-tag-bulk-transfer-xml-csv/11-RIG-NOTES.md
    - .planning/phases/11-tag-bulk-transfer-xml-csv/artifacts/udt-multilevel.xml
    - .gitattributes
  modified: []

key-decisions:
  - "Fidelity oracle is THREE-tier: transport sha256 always; byte-identity ONLY for unchanged-subtree re-export (3x byte-stable proven); import→re-export uses order-normalized structural identity because sibling order permutes on model rebuild (1128==1128 length, byte-different, structurally identical — captured pair)"
  - "importTags REFUSES UDT type definitions (verbatim: 'Udt definitions can only be imported in the UDT Definitions tab') — UdtType-bearing XML files are excluded from the round-trip oracle; the loss-scan must surface type=\"UdtType\" presence"
  - "Mixed-parent exportTags is silent corruption (type=\"Unknown\" empty Tag), NOT an error, and PRE-EXISTS in the JSON baseline — any pre-resolution refusal is a both-formats concern"
  - "Collision 'a' on importTags/importCSV never throws — failures ride Bad_Failure(\"Tag 'X' already exists…\") QualityCode elements; the route/action layer must inspect the list, not catch exceptions"
  - "CSV import grammar locked by gateway jar evidence: 51 PROP_COLUMNS + 5 structural + special Permissions/AlarmStates headers; Path/Owner columns UNUSABLE (NPE / Bad_Unsupported); basePath IGNORED for CSV (tags land at provider root); alarms and permissions parse Good but silently land NOTHING"
  - "Rigs intentionally KEPT ALIVE for 11-06 (stated in 11-RIG-NOTES); probe tags remain, 11-06 uses P11Live* namespace"

patterns-established:
  - "Probe-evidence sections map 1:1 to research Open Questions so later plans cite sections instead of re-deriving wire truth"
  - "Raw probe JSONs staged outside the repo (/tmp/ign-p11-rigs/probe-outputs/) with verbatim evidence transcribed into the captures doc"

# Metrics
duration: 56min
completed: 2026-09-11
---

# Phase 11 Plan 01: Live Gateway Captures Summary

**All five roadmap-mandated probes answered with verbatim rig evidence from 8.3.6 + 8.3.3 — including a real multi-level UDT XML fixture (sha-pinned) and a jar-anchored legacy CSV coverage table that corrects four plan hypotheses.**

## Performance

- **Duration:** 56 min
- **Started:** 2026-09-11T15:23:42Z
- **Completed:** 2026-09-11T16:20:28Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments
- Both rigs (8.3.6 @18188 `ign-p11-836`, 8.3.3 @19188 `ign-p11-833`) spun, commissioned headlessly, token-provisioned, and the 1.1.0 bundle deployed with a working secret-gated scriptExec (fourth rig-generation proof of the recipe; ~3 min end-to-end).
- Probe 1: kwargs+xml **returns the full XML document as a unicode string** — CRLF endings, 3-space indent, trailing CRLF, **no `<?xml` declaration**; the documented positional form is the filePath-writing fallback (arg-1 interpreted relative to gateway cwd).
- Probe 2: `importTags` works from a script thread with BOTH `[provider]folder` AND provider-root basePaths — **the 07-06 RpcContext constraint does NOT apply to importTags**; returns `ArrayList[QualityCode]` (`Good` / `Bad_Failure("…already exists, and 'abort' collision policy has been specified")`); JVM temp dir `/tmp` writable; collision failures ride elements, never exceptions.
- Probe 3: real multi-level UDT XML captured byte-faithfully to `artifacts/udt-multilevel.xml` (UdtType + CompoundProperty alarms + UdtInstance nesting + parameter encodings); unchanged-subtree exports are 3× byte-identical; import→re-export is structurally identical but NOT byte-identical (sibling order permutes on rebuild); `importTags` refuses UdtType content outright; cross-rig delta is sibling order only.
- Probe 4: mixed-parent export **silently mangles** (`type="Unknown"` empty Tag) — and the JSON baseline shows the identical corruption, so the constraint is pre-existing, not XML-specific.
- Probe 5: legacy CSV vocabulary anchored to the gateway's own `TagCSVImporter.PROP_COLUMNS` (51 names, both rigs identical); full column-by-column coverage table recorded (LANDED/COERCED/DROPPED/UNUSABLE); Path and Owner columns are unusable (NPE / abort), basePath is ignored for CSV, alarms and permissions parse "Good" but land nothing; numeric enums coerced (TagType/DataType/ExpressionType maps captured); comma-in-value quoting trap proven live twice.

## Task Commits

Each task was committed atomically:

1. **Task 1: Spin, commission, provision, and deploy both rigs** - `4b3fbd6` (docs)
2. **Task 2: Run probes 1, 2, 4 via scriptExec on both rigs** - `86e80da` (docs)
3. **Task 3: Capture REAL multi-level UDT XML (probe 3) + legacy CSV coverage map (probe 5)** - `d7ea2a3` (docs)
4. **Fix: artifact byte fidelity under core.autocrlf=input** - `5689623` (fix)

**Plan metadata:** (next docs commit)

## Files Created/Modified
- `.planning/phases/11-tag-bulk-transfer-xml-csv/11-LIVE-CAPTURES.md` - the five probe answers + CSV coverage table + verbatim exception strings + open-question ledger
- `.planning/phases/11-tag-bulk-transfer-xml-csv/11-RIG-NOTES.md` - ops log (rigs, commissioning timeline, token NAMES, secret PATHS, teardown state)
- `.planning/phases/11-tag-bulk-transfer-xml-csv/artifacts/udt-multilevel.xml` - REAL captured gateway XML bytes (2218 bytes, sha256 `11ed1528…`, CRLF preserved via `.gitattributes -text`)
- `.gitattributes` - pins the phase XML artifacts as `-text` so `core.autocrlf=input` cannot normalize captured bytes

## Decisions Made
- **Fidelity oracle (binds 11-06):** three-tier — transport sha256 always; byte-identity only for unchanged-subtree re-export; order-normalized structural identity for import→re-export round-trips; UdtType-bearing files excluded (gateway refuses their import). This replaces the plan's "byte-identity if deterministic, else fallback" with the captured answer.
- **Artifact scope:** the fixture is a TWO-path export (`['[default]P11UDT','[default]_types_/MotorType']`) because instances export as references only (no children/alarms) — the CompoundProperty block lives in the type definition; folder-level multi-path merges clean (the probe-4 corruption is leaf-sibling-specific).
- **Keep-alive:** rigs stay up for 11-06 (explicitly permitted by that plan), noted in 11-RIG-NOTES with probe-tag residue and the 1.1.0→1.2.0 redeploy expectation.
- **CSV generator constraints locked for 11-04:** numeric enums, `# version=1` marker, EMPTY Path cells, RFC-4180 quoting mandatory (live-proven trap), and the lossy-field list from the coverage table.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] scriptExec exec scope does not carry `system`**
- **Found during:** Task 2 (first probe attempt)
- **Issue:** `exec code in g` runs in a fresh globals dict — the WebDev module scope (which carries `system`) is not inherited; every probe failed with `NameError: name 'system' is not defined`. Prior live gates only ever exercised the `version` action, so this was the first live proof of exec-action scope semantics.
- **Fix:** probe code prepends `import system` (verified to resolve the full scripting surface on both rigs); recorded as a method finding + caveat in 11-LIVE-CAPTURES.md for all later scriptExec use (including 11-06).
- **Files modified:** 11-LIVE-CAPTURES.md (Method section)
- **Verification:** `system.tag.browse` + all probes green on both rigs after the bridge
- **Committed in:** 86e80da

**2. [Rule 3 - Blocking] `core.autocrlf=input` silently normalized the captured artifact's CRLF bytes**
- **Found during:** Task 3 commit (git warning: "CRLF will be replaced by LF")
- **Issue:** the committed blob sha (`4dbe0b21…`) did not match the live capture (`11ed1528…`) — the fixture's byte fidelity (itself a captured finding: CRLF endings, no XML declaration) would have been corrupted for 11-03's scan validation.
- **Fix:** `.gitattributes` marks the phase XML artifacts `-text`; re-staged; committed blob verified byte-identical (`git show HEAD:… | shasum` == `11ed1528…`).
- **Files modified:** .gitattributes, artifacts/udt-multilevel.xml
- **Verification:** blob sha256 equality asserted in the fix commit
- **Committed in:** 5689623

---

**Total deviations:** 2 auto-fixed (2 blocking). **Impact on plan:** both fixes protect evidence integrity (scope semantics for later scriptExec use; artifact bytes for the 11-03 fixture). No scope creep; zero product code.

**Plan-hypothesis corrections (captured ground truth, not deviations — the plan anticipated them):** collision `'a'` does not throw (rides Bad_Failure elements); mixed-parent export silently corrupts instead of erroring; the CSV `Path`/`Owner` columns are unusable and CSV `basePath` is ignored; the docs' 47/48-column table is neither the parser's vocabulary (51 PROP_COLUMNS) nor required.

## Issues Encountered
- The CSV probe required an iterative bisect (header shapes, line endings, Path-cell variants) because two independent failure modes (broken folder-branch NPE; unquoted-comma column shift) confounded each other; resolved by isolating one variable per import and ultimately anchoring the vocabulary by reading `PROP_COLUMNS` from `common.jar` inside the container.
- Probe-1c side effect: the positional form wrote a stray file `/usr/local/bin/ignition/[default]P11Seed` inside each disposable container (documented; removed with the containers at teardown).

## User Setup Required

None - rigs are disposable and self-provisioned; no external service configuration required.

## Next Phase Readiness
- 11-02 (route design) can cite: exportTags+xml returns the document directly (no temp-file fallback needed for XML export); importTagsFile must use temp files + inspect QualityCode elements; provider-root importTags works from script threads (WebDev-thread behavior still gated on 11-06 as the final oracle).
- 11-03 (scan module): `artifacts/udt-multilevel.xml` is the mandatory validation fixture; `Unknown`-type tags, `CompoundProperty`, `Parameters`, and `boundValueType="parameter"` shapes are the scan targets with captured byte shapes.
- 11-04 (CSV generator): the coverage table IS the documented lossy-field list to warn about; generator must emit numeric enums + marker row + quoted cells + empty Path cells.
- 11-06 (live gate): oracle strategy is named in §Probe 3; rigs kept alive; expect to deploy 1.2.0 over the standing 1.1.0.

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-11*

## Self-Check: PASSED
- All 4 created files exist on disk (captures doc, rig notes, artifact fixture, .gitattributes)
- All 4 task/fix commits found in git log (4b3fbd6, 86e80da, d7ea2a3, 5689623)
- Artifact blob fidelity verified: committed sha256 == captured sha256 (11ed1528…)
