---
phase: 11-tag-bulk-transfer-xml-csv
plan: 04
subsystem: tags
tags: [base64, passthrough, legacy-csv, wiremock, timeout-override, xml, bulk-transfer, TAGS-10, TAGS-11]

# Dependency graph
requires:
  - phase: 11-tag-bulk-transfer-xml-csv (11-02)
    provides: the 1.2.0 route surface — exportTags `format=xml` → `payload_b64`, `importTagsFile` (file_b64/basePath/collisionPolicy) — REQUEST-pinned at the raw call layer
  - phase: 11-tag-bulk-transfer-xml-csv (11-03)
    provides: tag_loss::scan_xml/scan_csv (top_level_names/tag_count/facts — the scan REPLACES a re-parse), codes::CSV_NO_ALARMS + xml_udt_type_definition, LossFact
  - phase: 11-tag-bulk-transfer-xml-csv (11-01)
    provides: 11-LIVE-CAPTURES.md — the locked wire truth every fidelity claim cites (Probe 1 byte constants, Probe 2 collision/element semantics, Probe 5 CSV coverage table)
provides:
  - "format-aware `tags_export` (ExportFormat{Json,Xml}): xml = decode payload_b64 → RAW gateway bytes, file mode writes with NO trailing newline, stdout mode carries the decoded string in TagsExportResult.raw — zero parse/normalize/re-serialize in the path"
  - "format-aware `tags_import` (ImportFormat{Json,Csv,Xml}): xml/csv = bytes → base64 file_b64 → importTagsFile verbatim; the scan feeds the abort pre-check (no re-parse); facts/names ride TagsImportResult for 11-05"
  - "TAGS_EXPORT_TIMEOUT = 300s (the 09-05 pattern) via default trait method `webdev_route_call_with_timeout` — 17 test doubles untouched; birth-pinned"
  - "generate_legacy_csv + CsvGenerationReport{csv,rows,dropped_keys,coerced} — the one sanctioned CLI-side conversion, every drop/coercion reported against §CSV-Coverage"
  - "workspace base64 0.22 (byte-faithful envelope carrier); LossFact: Serialize"
affects: [11-05 cli contract layer (clap --format, raw stdin/file dispatch, loss gate + report rendering), 11-06 live gate (transport-fidelity sha256 tier)]

# Tech tracking
tech-stack:
  added: [base64 0.22]
  patterns:
    - "default-trait-method override: webdev_route_call_with_timeout delegates to the plain call — only the real client overrides (17 doubles inherit untouched)"
    - "scan-as-name-source: import pre-check reads top_level_names from the 11-03 scan, never re-parses the input"
    - "capture-wins reconciliation: plan sketches vs live captures resolved in favor of §Probe 5 (empty Path cells, TagType-13 placeholders)"

key-files:
  created: []
  modified:
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-core/tests/tags_contract.rs
    - crates/ignition-core/src/client/mod.rs
    - crates/ignition-core/src/client/tags.rs
    - crates/ignition-core/src/actions/tag_loss.rs
    - Cargo.toml
    - crates/ignition-core/Cargo.toml
    - crates/ignition-cli/src/main.rs
    - crates/ignition-tui/src/update.rs

key-decisions:
  - "Timeout plumbing rides a DEFAULT trait method (webdev_route_call_with_timeout delegating to webdev_route_call) — only GatewayClient overrides with webdev_post_raw's optional per-request timeout; all 17 GatewayApi test doubles inherit the default untouched (minimal blast radius, research Pattern 4)"
  - "TAGS_EXPORT_TIMEOUT (300s) is shared by the bulk importTagsFile call — the same one-request-whole-file envelope in the opposite direction would die at the 30s client default identically (Rule 2); the json arms keep the plain call, byte-identical"
  - "TagsImportResult.failed carries verbatim Bad_Failure/Error_* QualityCode elements — the 11-01 probe-2d element-not-exception semantics made `imported = names.len()` a potential lie on the server-side backstop; recording keeps it honest"
  - "Capture-wins CSV generation: Path cells ALWAYS empty (any non-empty Path NPEs the importer wholesale, probe 5) — the plan's slash-prefixed-path sketch and `_types_/` rows were reconciled toward the captures per the plan's own reconciliation clause; folder rows still emit (TagType 6) with children flattened, UDT types emit as TagType-13 placeholders, all reported"
  - "Numeric enum tables grounded against the LIVE official docs page (fetched during execution): DataType 0..9 (0=Int1…6=Boolean,7=String,8=DateTime,9=DataSet), TagType 0/1/2/6/10/13, ExpressionType 0/1/2 — capture cross-check passes (2=Int4, 7=String, 6=Folder, 10=UdtInstance); ExpressionType 3=named_query is capture-proven (docs table stops at 2)"
  - "Task 3's contradictory signatures (Result<Vec<u8>> vs 'return them in the CsvGenerationReport') resolved by carrying the bytes INSIDE the report: CsvGenerationReport{csv: Vec<u8>, rows, dropped_keys, coerced}"

patterns-established:
  - "Bulk-transfer fidelity seam: base64 is the ONLY transformation; tags.rs contains zero XML/CSV Readers (verified by grep — the csv Writer exists only inside generate_legacy_csv)"
  - "Result-struct additive growth: envelope data grows new keys (format/raw/top_level_names/loss_facts/failed), never renames — the 11-05 render layer's stable input"
  - "Caller-owns-dispatch: core accepts &[u8]; the CLI's raw stdin/file read is 11-05's (11-04's callers serialize the parsed Value — value-identical, json behavior unchanged)"

# Metrics
duration: 49 min
completed: 2026-09-12
---

# Phase 11 Plan 04: Core Transfer Layer Summary

**Byte-faithful XML/CSV transfer actions (payload_b64 decode out, file_b64 encode in) with the scan-fed abort pre-check, a 300s bulk-transfer timeout override, and the honest documented-lossy legacy CSV generator — 10 new contract pins, 1027 workspace tests green**

## Performance

- **Duration:** 49 min
- **Started:** 2026-09-12T13:06:41Z
- **Completed:** 2026-09-12T13:56:02Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments

- **The byte-faithfulness seam is real:** xml export decodes `payload_b64` to raw gateway bytes (byte-equality pinned against the Probe-1b canned XML — CRLF/no-declaration/trailing-CRLF intact); xml/csv import encodes input bytes to `file_b64` (round-trip injectivity pinned at the request layer). The invariant "no codec in the transfer path" is grep-provable: tags.rs carries zero XML/CSV Readers — the csv Writer exists only inside the generator.
- **The collision matrix extends to the new formats with the scan as the name source:** browse → compare → refuse `tag_collision` (exit 6) with the importTagsFile mock at `expect(0)` (the zero-write proof); overwrite skips the pre-check (browse `expect(0)`).
- **The 300s bulk-transfer ceiling** (research Pitfall 6, the 09-05 pattern) rides a default trait method so no test double changed; birth-pin test asserts exactly 300s.
- **generate_legacy_csv**: docs-sample 48-column header byte-for-byte + `# version=1` marker, numeric enums grounded against the live official docs table AND the captures, RFC-4180 quoting via the csv crate (embedded-newline round-trip pinned), and every drop/coercion reported — the §CSV-Coverage silent-drop vocabulary becomes reported drops.
- JSON arms are byte-identical: every pre-11-04 pin passes untouched (only mechanical format-arg additions at call sites).

## Task Commits

Each task was committed atomically:

1. **Task 1: tags_export format param — XML raw-byte passthrough + export timeout** - `80b24e8` (feat) — 10 files; also carries the tags_import signature change (shared file, atomic for compilation — see Deviations)
2. **Task 2: tags_import action-layer importTagsFile pins** - `cd83e18` (test) — 5 wiremock pins (body verbatim, csv arm, zero-write abort, overwrite no-pre-check, provider-root refusal)
3. **Task 3: generate_legacy_csv** - `5b2d6c2` (feat) — generator + 3 fixture pins

**Plan metadata:** see final docs commit (SUMMARY + STATE)

## Files Created/Modified

- `crates/ignition-core/src/actions/tags.rs` — ExportFormat/ImportFormat, TagsExportResult.{format,raw}, TagsImportResult.{format,top_level_names,loss_facts,failed}, format-aware tags_export/tags_import (+ tags_import_bulk, refuse_on_collision, policy_char), generate_legacy_csv + LEGACY_CSV_HEADER + CsvGenerationReport + enum/DataType mappers
- `crates/ignition-core/tests/tags_contract.rs` — 10 new pins (2 xml export, 5 bulk import, 3 csv generation) + decoded CANNED_XML literal (the byte-equality oracle)
- `crates/ignition-core/src/client/mod.rs` — default `webdev_route_call_with_timeout` trait method + real override; `webdev_post_raw` gained an optional per-request timeout (route_call/route_probe pass None — unchanged behavior)
- `crates/ignition-core/src/client/tags.rs` — `TAGS_EXPORT_TIMEOUT: Duration = 300s` + birth-pin test
- `crates/ignition-core/src/actions/tag_loss.rs` — LossFact derives Serialize (additive; facts ride the import envelope)
- `Cargo.toml` / `crates/ignition-core/Cargo.toml` — workspace base64 0.22 with the byte-faithful-carrier rationale comment
- `crates/ignition-cli/src/main.rs`, `crates/ignition-tui/src/update.rs` — mechanical caller updates: ExportFormat::Json / ImportFormat::Json + Value→bytes bridge (raw-byte dispatch is 11-05's)

## Decisions Made

- **Default-trait-method timeout override** — adding a REQUIRED method would touch all 17 GatewayApi impls; the default body delegates to `webdev_route_call`, so only the real client overrides and the doubles inherit (behavior-identical: same classify + envelope parse, only the ceiling changes).
- **The bulk import shares TAGS_EXPORT_TIMEOUT** — the plan locked the override for the exportTags call only; the importTagsFile call carries the same whole-file one-request envelope and would truncate identically (Rule 2, small + tested by construction).
- **`failed: Vec<String>` on the import result** — probe-2d proved collision failures ride `Bad_Failure` ELEMENTS; ignoring them (the json arm's `_qualities` precedent) would let the server-side abort backstop report as success. Verbatim recording, additive envelope key.
- **Capture-wins on Path/UDT-type rows** — the plan's slash-prefixed `Path` and `_types_/` sketches contradict the captures (any non-empty Path NPEs the importer all-or-nothing); the plan's own reconciliation clause ("in favor of the captures") governs: empty Path everywhere, folder flattening + TagType-13 placeholders REPORTED as coercions.
- **Docs enums grounded live** — the plan mandates "the docs 0..9 table" while the captures pin only two pairs; the official docs page was fetched during execution (DataType 0=Int1…9=DataSet, TagType, ExpressionType 0/1/2) and the capture pairs cross-check exactly. InterpolationMode Analog→1 uses the capture over the docs table (docs list 0/2/3; probe proved 1 lands Analog).
- **Header width 48, not 47** — the plan's "47 columns" refers to the docs' prose; the mandated byte-for-byte skeleton (research §Legacy CSV skeleton, verified identical on the live docs page) is 48 columns. 11-03 already recorded this discrepancy.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] tags_import signature change rode the Task-1 commit; CLI/TUI callers updated**
- **Found during:** Task 1 (tags.rs edit)
- **Issue:** both actions live in tags.rs and both changed signature (format param + `&[u8]` input); committing export alone would leave the tree uncompilable. The plan's files_modified list excluded main.rs/update.rs, but the signature change breaks both crates.
- **Fix:** Task-1 commit carries the whole tags.rs action-surface change; main.rs (2 sites) and update.rs (3 sites) pass `ExportFormat::Json`/`ImportFormat::Json` and bridge the parsed Value to bytes (`serde_json::to_vec` — value-identical, json wire behavior unchanged). The raw stdin/file dispatch rework remains 11-05's, as planned.
- **Files modified:** crates/ignition-cli/src/main.rs, crates/ignition-tui/src/update.rs (inside 80b24e8)
- **Verification:** workspace build + all pre-11-04 goldens pass untouched
- **Committed in:** 80b24e8

**2. [Rule 2 - Missing Critical] importTagsFile rides the 300s bulk-transfer override**
- **Found during:** Task 2 (bulk arm implementation)
- **Issue:** the plan locks the timeout override for the exportTags call only; the importTagsFile call has the same whole-file request shape and would die at the 30s client default identically on large imports.
- **Fix:** the bulk arm calls `webdev_route_call_with_timeout` with TAGS_EXPORT_TIMEOUT; json arms keep the plain call (byte-identical).
- **Files modified:** crates/ignition-core/src/actions/tags.rs (tags_import_bulk)
- **Verification:** contract pins exercise the override path (35 tagConfig-request pins green)
- **Committed in:** 80b24e8 (implementation) / cd83e18 (pins)

**3. [Plan contradiction — resolved] Task 3's `Result<Vec<u8>>` vs the CsvGenerationReport**
- **Found during:** Task 3
- **Issue:** the task mandates signature `Result<Vec<u8>, CoreError>` AND requires dropped/coerced fields returned "in the result (`CsvGenerationReport`)" — mutually exclusive.
- **Fix:** `CsvGenerationReport { csv: Vec<u8>, rows, dropped_keys, coerced }` — the report carries the bytes; the union of both plan statements.
- **Committed in:** 5b2d6c2

**4. [Rule 1 - Bug, caught by clippy -D warnings] collapsible-if + type_complexity in the generator**
- **Found during:** Task 3 (verification)
- **Fix:** `let`-chain for the Folder recursion; `type EnumTable` alias. Zero behavior change.
- **Committed in:** 5b2d6c2

---

**Total deviations:** 4 auto-fixed (1 blocking, 2 missing-critical/bug class, 1 plan-internal contradiction) + the capture-wins Path reconciliation (plan-mandated, not a deviation).
**Impact on plan:** all fixes required for compilation, honesty, or the captures' own wire truth. No scope creep.

## Issues Encountered

- **First-pass generator implementation used recursive closures** (borrow-checker gymnastics with shims) — replaced before commit with a standalone recursive `emit_tag_row` over a small `LegacyCsvWriter` state struct. The tangled version never compiled; no intermediate commit carries it.
- **Hand-written CSV base64 fixture was wrong** (caught before running: the pin's `file_b64` didn't match the encode of the input); recomputed via python and corrected in the test before its first run.
- The exec-environment web-search tool was unavailable (missing EXA_API_KEY); the official docs page was fetched via the browser tool instead — same grounding, one tool swap.

## User Setup Required

None - no external service configuration required. (Wiremock-driven plan; both 11-RIG-NOTES rigs remain untouched for 11-06.)

## Next Phase Readiness

- **11-05 (CLI contract layer)** consumes: `ExportFormat`/`ImportFormat` (clap value derivation is explicitly deferred here), `TagsExportResult.{format,raw}` (stdout-mode raw-byte printing), `TagsImportResult.{top_level_names,loss_facts}` (the loss-gate input — `facts` non-empty && !`--yes` → exit-2 refusal, the 11-03 gate semantics), `CsvGenerationReport.{dropped_keys,coerced}` (warn-and-continue on CSV download)
- **11-06 (live gate)** consumes: the transport-fidelity tier (`file bytes == decode(payload_b64)` — pinned here at the unit layer), the importTagsFile request shape (pinned here at the action layer), and the provider-root WebDev-thread open question (the action surfaces `provider_root_unsupported` verbatim — pinned here)
- MIN_CLI unchanged; no README exit-table movement (zero new slugs — `tag_collision`, `provider_root_unsupported`, `webdev_route_error` all pre-existing)
- Known follow-ups for 11-05: csv-generation warning text should cite the dropped-keys vocabulary (the report carries it); TagsImportResult.failed should render if non-empty (backstop honesty)

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-12*

## Self-Check: PASSED

- All 9 modified files + this summary exist on disk
- All 3 task commits verified in git log: 80b24e8 (feat) / cd83e18 (test) / 5b2d6c2 (feat)
- Key symbols present: ExportFormat/ImportFormat/generate_legacy_csv in tags.rs (33 hits), TAGS_EXPORT_TIMEOUT birth-pin in client/tags.rs, workspace base64 dep
- Transfer-path invariant grep-proven: zero csv::Reader / quick_xml in tags.rs; tag_loss scans appear only as export tally (tags.rs:1810) and import names/facts (tags.rs:2121-2125)
- Final sweep: cargo fmt --check OK · clippy -D warnings clean · 1027 tests passed / 0 failed
