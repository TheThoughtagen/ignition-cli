---
phase: 11-tag-bulk-transfer-xml-csv
plan: 03
subsystem: tags
tags: [quick-xml, csv, loss-scan, advisory-parse, tdd, tag-transfer, TAGS-12]

# Dependency graph
requires:
  - phase: 11-tag-bulk-transfer-xml-csv (11-01)
    provides: the real captured multi-level UDT XML (artifacts/udt-multilevel.xml, sha 11ed1528…) and the legacy CSV lossy-field table (51 PROP_COLUMNS, probe 5)
provides:
  - "tag_loss::scan_xml / scan_csv — pure advisory scans over raw bytes (lenient by contract: never refuse, partial rides the report)"
  - "XmlScan / CsvScan / LossFact structs + pub mod codes (seven stable fact-code constants) — the report contract 11-04/11-05/11-06 consume"
  - "crates/ignition-core/tests/fixtures/udt-multilevel.xml — the live capture byte-verbatim in the test tree (blob sha verified after commit)"
  - "workspace deps: quick-xml 0.41 (DEFAULT features ONLY, serde-derive rejection documented in rationale comment) + csv 1.4"
affects: [11-04 tags import/export actions, 11-05 loss gate + report rendering, 11-06 live gate fidelity oracle]

# Tech tracking
tech-stack:
  added: [quick-xml 0.41 (default features, no serialize), csv 1.4]
  patterns: [advisory-parse posture (report-what-you-saw, never refuse), depth-tracked top-level element detection in a quick-xml Event loop, order-preserving dedupe for report vocabulary, stable fact-code constants module]

key-files:
  created:
    - crates/ignition-core/src/actions/tag_loss.rs
    - crates/ignition-core/tests/fixtures/udt-multilevel.xml
  modified:
    - Cargo.toml
    - crates/ignition-core/Cargo.toml
    - crates/ignition-core/src/actions/mod.rs
    - .gitattributes
    - Cargo.lock

key-decisions:
  - "quick-xml 0.41 Event loop WITHOUT the serialize feature (planner lock): dynamic <Property name=...> attribute VALUES can't map to struct fields and a strict schema parser would refuse files the gateway accepts"
  - "csv_no_alarms + csv_legacy_columns_only are UNCONDITIONAL facts for any non-empty CSV scan — they are format-level truths ('CSV does not include support for alarm configurations'), not column-dependent"
  - "Added xml_udt_type_definition fact beyond the plan's code list — traceable to the 11-01 capture decision ('the loss-scan must surface type=UdtType'): importTags refuses type definitions and lands nothing, so the advisory is mandatory honesty for 11-04/11-05"
  - "Marker-row position is lenient: '# version=N' recognized before OR after the header row (docs skeleton puts it second); marker rows never count as data rows"
  - "Attribute values via quick-xml normalized_value(XmlVersion::Implicit1_0) — 0.41 deprecated unescape_value; Implicit1_0 matches the captures exactly (no XML declaration ⇒ 1.0 assumed)"
  - "Fixture byte-fidelity guarded by a .gitattributes -text rule for crates/ignition-core/tests/fixtures/*.xml — the 11-01 core.autocrlf=input lesson applied preemptively; committed blob sha asserted == 11ed1528…"

patterns-established:
  - "Loss-scan contract: scan output REPLACES a re-parse — 11-04 takes top_level_names/tag_count from the scan, never re-reads the file"
  - "LossFact{code, detail}: stable machine code + human detail line; codes are never renamed, only added"
  - "Partial-parse honesty: Err in the event loop sets partial=true + xml_parse_partial fact and breaks — whatever was readable before the error rides the report"

# Metrics
duration: 20h 32m wall-clock (overnight gap included)
completed: 2026-09-12
---

# Phase 11 Plan 03: Loss-Report Advisory Scans Summary

**TDD'd TAGS-12 loss scans — quick-xml Event-loop `scan_xml` + csv-crate `scan_csv` in `tag_loss.rs`, proven against the REAL 11-01 multi-level UDT capture (8 Tags, CompoundProperty alarms, P11UDT+MotorType top-level) with zero hard refusals**

## Performance

- **Duration:** 20h 32m wall-clock (session spanned an overnight gap; three TDD commits)
- **Started:** 2026-09-11T16:28:13Z
- **Completed:** 2026-09-12T13:00:17Z
- **Tasks:** 2 (TDD — produced 3 commits: RED, GREEN, REFACTOR)
- **Files modified:** 7 (2 created, 5 modified)

## Accomplishments
- `scan_xml` + `scan_csv` pure advisory functions, full RED→GREEN→REFACTOR discipline with every behavior case pinned by a failing test first
- The roadmap's "validate the loss-report design against real exports" flag is CLOSED: the anchor test parses the live-captured 2218-byte UDT XML cleanly (partial==false, tag_count==8, alarms detected, top_level_names==[P11UDT, MotorType])
- Both workspace dependencies landed with the mandated rationale comments; quick-xml carries default features ONLY (cargo tree verified — no `serialize`)

## Task Commits

Each task was committed atomically (TDD sequence):

1. **Task 1 (RED): deps + fixture + failing scan tests** - `9049ca9` (test)
2. **Task 2 (GREEN): implement the scans** - `f0be0f4` (feat)
3. **Task 2 (REFACTOR): fact-code constants + posture docs** - `88d9193` (refactor)

_Note: 11-02 committed concurrently in the shared tree (`34d6594`, `20acc77`) — interleaved history, zero file conflicts._

## Files Created/Modified
- `crates/ignition-core/src/actions/tag_loss.rs` — scan_xml/scan_csv + XmlScan/CsvScan/LossFact + `pub mod codes` (7 stable fact codes), 9 inline tests
- `crates/ignition-core/tests/fixtures/udt-multilevel.xml` — the real 11-01 capture byte-verbatim (2218 bytes, CRLF preserved, blob sha `11ed1528…` asserted after commit)
- `Cargo.toml` — workspace deps quick-xml 0.41 + csv 1.4 with serde-derive rejection rationale
- `crates/ignition-core/Cargo.toml` — the two workspace references
- `crates/ignition-core/src/actions/mod.rs` — `pub mod tag_loss;`
- `.gitattributes` — `crates/ignition-core/tests/fixtures/*.xml -text` (fixture fidelity rule)
- `Cargo.lock` — resolved quick-xml 0.41.0 + csv 1.4.0

## Decisions Made
- Unconditional CSV facts (`csv_no_alarms`, `csv_legacy_columns_only`) fire for any non-empty scan — they are properties of the FORMAT, not the columns present; empty input yields a zero-value scan with no facts
- Numeric-enum detection covers TagType/DataType/AccessRights (plan's implementation note); one `csv_numeric_enum` fact per numeric cell, naming the column and the raw value
- `xml_udt_type_definition` fact added beyond the plan's example code list (see Deviations) — surfaces the capture-proven fact that UdtType-bearing files import to NOTHING
- Top-level name derivation: empty Path ⇒ the row's own Name; non-empty Path ⇒ first `/`-segment of the Path (folder membership rides Path, per the plan's `P11Csv/` ⇒ `P11Csv` rule); order-preserving dedupe
- quick-xml `check_end_names` pinned explicitly; truncated documents therefore error (partial) rather than silently EOF

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added .gitattributes -text rule for the test fixture**
- **Found during:** Task 1 (fixture placement)
- **Issue:** `.gitattributes` only covered `.planning/**/artifacts/*.xml`; with `core.autocrlf=input` the CRLF capture would have been LF-normalized on commit, violating the must-have "verbatim from 11-01" (exactly the 11-01 pitfall recorded in STATE.md)
- **Fix:** `crates/ignition-core/tests/fixtures/*.xml -text`; committed blob sha256 asserted == `11ed1528…` after the RED commit
- **Files modified:** .gitattributes (outside the declared file list — required by the plan's own byte-fidelity artifact contract)
- **Verification:** `git cat-file -p HEAD:crates/ignition-core/tests/fixtures/udt-multilevel.xml | shasum -a 256` == `11ed15286879…`
- **Committed in:** 9049ca9

**2. [Rule 1 - Bug] scan.columns was never assigned to the struct field**
- **Found during:** Task 2 (GREEN — caught by the failing docs-sample test after 7/9 passed)
- **Issue:** the local `columns` vec fed idx_of/facts but `scan.columns` stayed empty — every column assertion failed with `columns.len() == 0` despite the csv crate parsing 4 clean records
- **Fix:** `scan.columns = columns.clone();`
- **Files modified:** crates/ignition-core/src/actions/tag_loss.rs
- **Verification:** docs-sample test asserts columns.len() == 48 + the plan-specified subset; full suite green
- **Committed in:** f0be0f4

**3. [Rule 1 - Bug] quick-xml 0.41 deprecated unescape_value; normalized_value requires an XmlVersion**
- **Found during:** Task 2 (clippy -D warnings gate)
- **Issue:** `Attribute::unescape_value` is deprecated in 0.41; its replacement `normalized_value` takes `XmlVersion`
- **Fix:** `normalized_value(XmlVersion::Implicit1_0)` — the variant meaning "no declaration ⇒ 1.0 assumed", which matches the gateway exports (no XML declaration) exactly
- **Files modified:** crates/ignition-core/src/actions/tag_loss.rs
- **Verification:** clippy --workspace --all-targets -D warnings clean
- **Committed in:** f0be0f4 / 88d9193

### Test-authoring corrections (not code defects)

- **Truncated-XML test truncation point:** the RED test cut `<Tag name="X"` mid-header, but quick-xml never emits a Start event for an unterminated element header — the error fires before anything is seen. Reshaped the const to include one COMPLETE self-closing Tag before the cut, so "whatever was seen before the error is reported" is actually exercised (partial==true, tag_count==1, name X reported). Fixed during Task 2 (f0be0f4).
- **mod.rs wiring committed with RED** (plan's file list placed it in Task 2): `cargo test -p ignition-core tag_loss` cannot compile — and per Task 1's verify, "reach its asserts" — without `pub mod tag_loss;`. No later change to mod.rs was needed (committed in 9049ca9).
- **Docs-sample header count:** the plan said "the 47 documented legacy headers"; the research §Code Examples skeleton (the mandated verbatim asset) is 48 columns. Test asserts the plan-specified representative subset AND len == 48, with a comment citing the docs' 47/48 discrepancy.

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs) + 3 documented test/process corrections.
**Impact on plan:** all fixes were required for correctness or the plan's own byte-fidelity contract. No scope creep; the scan surface matches the 11-04/11-05 consumers' expectations exactly.

## Issues Encountered
- Parallel-wave hygiene: 11-02's in-flight edits (`live_gateway.rs`, `ignition-tui/src/ui/mod.rs`) sat dirty in the shared working tree throughout; all commits staged files individually so nothing foreign landed in an 11-03 commit. Full workspace suite (1016 tests) ran green over the combined tree.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Ready for 11-04 (tags export/import actions): `top_level_names` + `tag_count` come straight from the scans; `codes::CSV_NO_ALARMS` is the gate-triggering fact its tests expect; the temp-file route contract in 11-LIVE-CAPTURES.md §Probe 2 is unmodified
- Ready for 11-05 (loss gate + report): `facts: Vec<LossFact>` with stable `codes` is the render source; gate semantics ("facts non-empty && !yes") hold — a minimal hand-written XML without MinVersion/UdtType/CompoundProperty produces zero facts and skips the gate
- Ready for 11-06 (live gate): `types_seen.contains("UdtType")` + the `xml_udt_type_definition` fact identify files excluded from the round-trip oracle (capture §3 item 4)

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-12*

## Self-Check: PASSED

- tag_loss.rs + tests/fixtures/udt-multilevel.xml exist on disk
- Commits 9049ca9 (RED) / f0be0f4 (GREEN) / 88d9193 (REFACTOR) all present in git log
- scan_xml + scan_csv both exported (2 pub fns confirmed)
- Fixture blob sha verified == 11ed1528… after commit
