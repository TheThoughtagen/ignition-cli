---
phase: 11-tag-bulk-transfer-xml-csv
plan: 05
subsystem: tags
tags: [cli-contract, loss-gate, TAGS-12, xml, csv, raw-bytes, stdout-purity, envelope, snapbox, wiremock]

# Dependency graph
requires:
  - phase: 11-tag-bulk-transfer-xml-csv (11-04)
    provides: format-aware tags_export/tags_import (ExportFormat/ImportFormat, TagsExportResult.raw, TagsImportResult.{top_level_names,loss_facts,failed}), generate_legacy_csv + CsvGenerationReport
  - phase: 11-tag-bulk-transfer-xml-csv (11-03)
    provides: tag_loss::scan_xml/scan_csv advisory scans + the stable codes vocabulary (csv_no_alarms, xml_udt_type_definition, …) — the gate's input
  - phase: 11-tag-bulk-transfer-xml-csv (11-01)
    provides: 11-LIVE-CAPTURES.md — Probe 1b XML byte constants (the raw-passthrough oracle), Probe 4 mixed-parent answer (NO gate — corruption pre-exists in JSON), Probe 5 CSV coverage table (the README honesty citations)
provides:
  - "--format json|xml|csv on `ign tags export` and `ign tags import` (TransferFormat value enum, json default byte-identical); format-aware default export filenames (.json/.xml/.csv)"
  - "THE TAGS-12 loss gate: one loss_gate() fn in dispatch — pre-resolution scan, prose report IS the exit-2 invalid_input refusal (profile null, zero requests), --yes attaches the structured scan summary as data.loss_report; imports only — csv generation warns-and-continues"
  - "data.loss_report envelope key (LossReport struct on TagsExport/TagsImportResult, skip-serialized at None) — additive, pre-11-05 envelopes byte-identical"
  - "xml/csv stdout mode writes RAW bytes in every render mode (fourth-exception extended); human-mode artifact summary + csv lossy warnings on STDERR only"
  - "13 new CLI contract pins (35 in contract_tags, 4 in the purity harness); README CSV-honesty + loss-gate + streaming-exception reconciliation"
affects: [11-06 live gate (transport-fidelity tier through the real CLI), Phase 12 TUI (loss-report surfaces could mirror the human renderers), agent consumers of data.loss_report]

# Tech tracking
tech-stack:
  added: [base64 0.22 (ignition-cli dev-deps — fixture byte-pinning only)]
  patterns:
    - "One-gate function: loss_gate() composes scan + prose + refusal — the 10-04 preview_then_confirm lesson, the refusal shape cannot drift per format"
    - "Prose-IS-the-message: the loss report rides error.message (rendered stderr in every mode) — never a separate stdout/print path"
    - "json-default invisibility: every new flag defaults to today's behavior; goldens prove the default is byte-identical"

key-files:
  created: []
  modified:
    - crates/ignition-cli/src/cli.rs
    - crates/ignition-cli/src/main.rs
    - crates/ignition-cli/src/render.rs
    - crates/ignition-core/src/actions/tags.rs
    - crates/ignition-cli/tests/contract_tags.rs
    - crates/ignition-cli/tests/contract_stdout_purity.rs
    - crates/ignition-cli/Cargo.toml
    - Cargo.lock
    - README.md

key-decisions:
  - "data.loss_report is ONE additive key covering both shapes (import scan: facts+top_level_names; csv generation: dropped_keys+coerced+rows) — the envelope lock mandates the key; skip-serializing members at empty keeps each shape tight and legacy envelopes byte-identical"
  - "JSON-mode csv export stays stderr-prose-free — the envelope's data.loss_report IS the agent-facing report (mirrors the refusal, whose prose rides error.message machine-readably); human mode gets the warnings on stderr"
  - "Mixed-parent export gets NO gate — 11-01 Probe 4 proved silent type=\"Unknown\" corruption PRE-EXISTS in the JSON interchange (not a hard failure, not new-format), exactly the planner lock's no-check branch; cited in a dispatch comment"
  - "Loss_report attaches only on --yes per the envelope lock; json skips the gate entirely and TagsImportResult.loss_facts (11-04) keeps riding unchanged"
  - "Default export filename extension rides the format (.xml/.csv) — a .json-named XML file would be dishonest; the json path keeps <last-segment>.json byte-identical"
  - "TUI untouched: the cockpit's import stays JSON-only — --format is CLI contract surface (files_modified scope honored)"

patterns-established:
  - "Raw-byte stdout exception extended to a FORMAT FAMILY: TagsExportResult.raw is the verbatim-bytes channel (xml gateway document / csv generator output), payload stays the json-pretty channel — render intercepts raw first, payload second"
  - "expect(0)-style absence: pre-resolution refusals are proven by mounting probe+route mocks at expect(0) — a leaked request FAILS verification instead of silently connecting nowhere"
  - "Human summary on stderr for stdout-mode artifacts: pipes stay pure while humans still see what happened"

# Metrics
duration: 33 min
completed: 2026-09-12
---

# Phase 11 Plan 05: CLI Contract Layer Summary

**`--format xml|csv` on tags export/import with the TAGS-12 loss gate (pre-resolution exit-2 refusal, --yes carries data.loss_report), raw-byte stdout passthrough, and the README's honest CLI-generated-CSV contract — 13 new pins, 1034 workspace tests green, json goldens untouched**

## Performance

- **Duration:** 33 min
- **Started:** 2026-09-12T14:08:46Z
- **Completed:** 2026-09-12T14:41:48Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments

- **TAGS-12 is user-visible and agent-visible:** a loss-bearing xml/csv import without `--yes` refuses exit 2 `invalid_input` with the loss report AS the message — proven pre-resolution by `expect(0)` mounts (zero requests) and by the refusal firing with no reachable gateway. With `--yes`, the importTagsFile request pins `file_b64 == base64(fixture)` verbatim and `data.loss_report` carries the structured scan summary.
- **The raw-byte seam reaches the terminal:** `tags export --format xml -o -` stdout is byte-equal to the decoded gateway payload (CRLF/trailing-CRLF intact, byte assert, not a golden); csv export emits `generate_legacy_csv`'s bytes exactly in file and stdout modes with no added trailing newline; human mode's summary line goes to stderr so pipes stay pure.
- **The CSV honesty is written down where agents read it:** the README now states the gateway cannot export CSV, that `--format csv` is CLI-generated and lossy (no alarms, legacy columns, numeric coercion), that generation warns-and-continues, and that the loss gate is advisory with the gateway as parsing authority — plus the importTags `Ignore ('i')` deliberate-omission note and the extended streaming-exception list.
- **The frozen contract held:** all 30 pre-existing contract_tags goldens pass UNTOUCHED (json default invisible), the purity harness gained two scenarios and stays byte-exact, no new exit codes or slugs (readme_exit_table_agreement green).

## Task Commits

Each task was committed atomically:

1. **Task 1: CLI flags + dispatch + loss gate + renderers** - `04a8559` (feat) — also carries the core LossReport envelope field (Rule 3, see Deviations)
2. **Task 2: Goldens + contract tests** - `eb25768` (test) — 5 new contract_tags tests + 2 purity scenarios + base64 dev-dep
3. **Task 3: README contract reconciliation** - `67f8f6a` (docs)
4. **Cargo.lock refresh** - `3abc7b6` (chore — rides Task 2's dev-dep addition)

**Plan metadata:** see final docs commit (SUMMARY + STATE)

## Files Created/Modified

- `crates/ignition-cli/src/cli.rs` — TransferFormat ValueEnum (+Display) on the existing Export/Import leaves; zero new tui_coverage rows
- `crates/ignition-cli/src/main.rs` — ONE `loss_gate()` (scan → prose refusal exit-2 pre-resolution → structured summary on --yes), `render_loss_prose`, `read_input_bytes` (raw stdin/file beside read_json_input, which now delegates), format-aware `default_export_path`, `export_csv_arm` (tags_export(Json) + generate_legacy_csv composition)
- `crates/ignition-cli/src/render.rs` — fourth-exception interception prints `raw` for xml/csv (every mode) + human stderr summary + csv lossy warnings; import human arm renders loss facts + the Bad_Failure backstop; format-aware export artifact line
- `crates/ignition-core/src/actions/tags.rs` — LossReport struct + `loss_report: Option` on both result structs (skip-serialized at None; all six construction sites pass None)
- `crates/ignition-cli/tests/contract_tags.rs` — 5 new tests: gate refusals (xml/csv/stdin, expect(0) absence), --yes flow (request pin + envelope), human --yes stream discipline, csv export warn-and-continue (byte equality + envelope), xml raw-byte export (stdout/file/human)
- `crates/ignition-cli/tests/contract_stdout_purity.rs` — loss-prose-never-on-stdout + export-refusal-stdout-empty scenarios
- `crates/ignition-cli/Cargo.toml` / `Cargo.lock` — base64 dev-dep (fixture pinning)
- `README.md` — two command rows, CSV honesty section, loss-gate section, collision-policy note, streaming exceptions

## Decisions Made

- **One `data.loss_report` key, two shapes** — the envelope lock mandates the key for both csv export and --yes imports; a single LossReport struct with skip-serialized members covers both without renaming or duplicating keys.
- **JSON mode gets no stderr prose from csv generation** — agents read `data.loss_report` from the envelope; prose on stderr in json mode would be unparseable noise. The refusal, by contrast, puts prose in `error.message` (machine-readable envelope field) — consistent.
- **No mixed-parent gate** — the planner lock's own condition ("only if the constraint is new") resolves against gating: 11-01 Probe 4 proves the corruption is silent and pre-existing in the JSON baseline. Cited in a dispatch comment so the reasoning survives.
- **loss_report rides only on --yes** — the envelope lock's exact wording; TagsImportResult.loss_facts (11-04) still rides every xml/csv import, so clean scans are not envelope-silent at the field level.
- **Format-aware default export extension** — `<last-segment>.xml`/`.csv`; writing gateway XML into a `.json`-named file would violate the README's honesty posture. Json path byte-identical.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] data.loss_report required a core struct change**
- **Found during:** Task 1 (envelope design)
- **Issue:** the plan's files_modified list excluded core, but the envelope lock mandates `data.loss_report` in the success envelope — the envelope IS the serialized result struct, so the field must live on TagsExportResult/TagsImportResult in core.
- **Fix:** additive `LossReport` struct + `loss_report: Option<LossReport>` with `skip_serializing_if = "Option::is_none"`; all six existing construction sites pass None, keeping pre-11-05 envelopes byte-identical (proven by the untouched json goldens).
- **Files modified:** crates/ignition-core/src/actions/tags.rs
- **Verification:** 30 pre-existing contract_tags goldens pass unmodified; workspace 1034/0
- **Committed in:** 04a8559 (Task 1 commit)

**2. [Rule 3 - Blocking] base64 dev-dependency for byte-exact fixture pins**
- **Found during:** Task 2 (file_b64/payload_b64 assertions)
- **Issue:** the request pins need to COMPUTE base64 expectations in-test; base64 was not an ignition-cli dev-dep.
- **Fix:** added `base64 = { workspace = true }` with the zip-precedent rationale comment (same crate core already depends on — not a new dependency).
- **Files modified:** crates/ignition-cli/Cargo.toml, Cargo.lock (refresh committed separately, 3abc7b6)
- **Verification:** request-body pins pass; lockfile consistent
- **Committed in:** eb25768

**3. [Test-authoring fixes, pre-commit] wrong-stream envelope parses and a mis-targeted --yes flag**
- **Found during:** Task 2 (first test run)
- **Issue:** (a) success envelopes ride STDOUT (errors to stderr) — two tests parsed stderr and panicked; (b) an edit landed `--yes` on the refusals test's spawn instead of the --yes flow test; (c) the abort policy's collision pre-check needs a `browse` mock mounted before importTagsFile can be reached (discovered via `server.received_requests()` dump — the 404 was the un-mocked browse).
- **Fix:** added `stdout_envelope` helper; corrected flag placement; mounted the browse pre-check mock (expect 1) in both --yes tests; removed the debug dump after diagnosis.
- **Verification:** all 35 contract_tags tests green
- **Committed in:** eb25768 (never an intermediate commit)

---

**Total deviations:** 2 auto-fixed (both blocking-class, both plan-internal contradictions between the files_modified list and the plan's own must_haves) + 3 pre-commit test-authoring corrections.
**Impact on plan:** both deviations were required by the plan's own must_haves (envelope key, byte-exact pins). No scope creep.

## Issues Encountered

- **Stale repo `target/debug/ign`**: the global `~/.cargo/config.toml` redirects target-dir to `~/Library/Caches/cargo-target`; the repo's target/ holds a Sep-10 leftover binary that silently answered early smoke calls with the old CLI surface. Diagnosed via mtime; all later verification used the cache-dir binary (and `cargo test` was unaffected — it resolves the binary through cargo).
- **expect(0) mock responses must still be shape-valid**: the zero-request probe mock's body ("version" key) was written casually; had a request ever leaked, the failure mode would have been confusing (internal no-routeVersion) — worth remembering that absence-proof mocks should carry real shapes.

## User Setup Required

None - no external service configuration required. (Wiremock-driven plan; the 11-RIG-NOTES rigs stay keep-alive for 11-06.)

## Next Phase Readiness

- **11-06 (live gate)** consumes: the full CLI surface exercised here (format flags, gate refusals, --yes flow, raw stdout) now wired end-to-end over the real route actions; the transport-fidelity tier (`sha256(file bytes) == sha256(decode(payload_b64))`) has its binary-layer pins; the 11-01 oracle strategy (order-normalized structural identity for import→re-export; UdtType files excluded from round-trip) is the gate's binding contract.
- **Agent surface:** `data.loss_report` is stable and additive; the README documents it — MCP/catalog layers (Phase 14) inherit the envelope unchanged.
- Known follow-up candidates (non-blocking): the loss-gate prose could name the file label in the report header (currently format-only); TagsImportResult.failed renders one stderr line per element — fine for the backstop scale it targets.
- MIN_CLI unchanged; zero new exit codes or slugs; README exit table machine-checked green.

---
*Phase: 11-tag-bulk-transfer-xml-csv*
*Completed: 2026-09-12*

## Self-Check: PASSED

- All 9 modified files verified on disk; README contains "cannot export" CSV truth + loss-gate section
- All 4 commits verified in git log: 04a8559 (feat) / eb25768 (test) / 67f8f6a (docs) / 3abc7b6 (chore)
- Final sweep: cargo fmt --check clean · clippy -D warnings clean · 1034 tests passed / 0 failed
- JSON-unchanged pin holds: all 30 pre-existing contract_tags goldens pass unmodified
- Purity harness green with the two new scenarios (4/4)
