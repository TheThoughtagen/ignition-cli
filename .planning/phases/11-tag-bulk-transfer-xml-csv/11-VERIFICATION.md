---
phase: 11-tag-bulk-transfer-xml-csv
verified: 2026-09-14T01:30:00Z
status: passed
score: 20/20 must-haves verified (3/3 success criteria, 6/6 plans' artifacts, all key links wired)
is_re_verification: false
human_verification: []
---

# Phase 11: Tag Bulk Transfer XML/CSV Verification Report

**Phase Goal:** Tags move in the formats the ecosystem already speaks — byte-faithful passthrough of what the gateway produces (never CLI-side re-serialization) — with honest warnings about what lossy formats would drop before import commits.
**Verified:** 2026-09-14T01:30:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| SC-1 | XML bulk download+upload with verbatim gateway-byte passthrough, round-trip proven on a real multi-level UDT export from a LIVE gateway | ✓ VERIFIED | `live_tags_xml_fidelity_roundtrip` PASS on both rigs (8.3.6 + 8.3.3) with recorded sha256s; transport fidelity is byte-exact (export file sha == route `payload_b64` sha per rig: `71221d02…` Rig A, `1797ef51…` Rig B); post-import re-export is structurally identical under the capture-selected order-normalized oracle (sanctioned fallback per 11-06 key_link — determinism disproven in Probe 3). The byte-difference is gateway-side sibling permutation, NOT CLI re-serialization. Fixture `artifacts/udt-multilevel.xml` is a real capture (UdtType/CompoundProperty present), byte-identical to the test fixture |
| SC-2 | CSV bulk download+upload: server-byte-faithful upload, documented lossy download | ✓ VERIFIED | Upload: raw bytes → base64 → `importTagsFile` with format-aware `.csv` temp suffix → `system.tag.importTags` (gateway's own parser; no CLI-side conversion exists in the path). Download: `generate_legacy_csv` (tags.rs:2795) drops/coerces per the live-proven Probe-5 coverage table; `live_tags_csv_roundtrip` PASS on both rigs; README documents "CLI-GENERATED and LOSSY" + no-alarms/legacy-columns behavior |
| SC-3 | Loss-report warning before import; user can abort | ✓ VERIFIED | `loss_gate()` (main.rs:2841) scans input bytes pre-resolution — refusal exits 2 `invalid_input` with profile null and ZERO wire work; `--yes` passes and report rides `data.loss_report`; `live_tags_loss_gate_refusal` PASS on both rigs (zero route calls, prose carrying `xml_export_edited_only` + `xml_udt_type_definition` + `--yes` hint); contract goldens `tags_import_loss_gate_refusals`/`_yes_flow` |
| Lock | XML = gateway-byte passthrough BOTH directions; only transformation is base64 (byte-exact by construction) | ✓ VERIFIED | tags.rs:1855 decode (`payload_b64` → raw) and :2209 encode (`input` → `file_b64`); zero parse/normalize/re-serialize in the XML path |
| Lock | Loss-gate refusal = exit 2 `invalid_input`, no new slugs | ✓ VERIFIED | Existing error class reused; README exit table unchanged (no new rows confirmed in contract_lint agreement tests) |
| Lock | Collision matrix abort/overwrite only; gateway's `'i'` never surfaced | ✓ VERIFIED | doPost.py:261 refuses anything but `'a'`/`'o'`; `[default]` policy is `'a'` |
| Lock | quick-xml 0.41 Event-loop NO serde + csv crate + base64 carrier | ✓ VERIFIED | Cargo.toml:39 `quick-xml = "0.41"` default features only, `serialize` rejection documented in rationale comment; `csv = "1"`, `base64 = "0.22"`; tag_loss.rs uses Event loop with `CompoundProperty` name checks, no serde derives |
| Pin | Three-way version pin discipline holds at **1.3.0** (post 11-06 bump) | ✓ VERIFIED | All 6 route constants = `1.3.0` (tagConfig, tags, alarms, tagHistory, scriptExec), `ROUTE_BUNDLE_VERSION = "1.3.0"` (mod.rs:25), `routes/VERSION` = `1.3.0`, `MIN_CLI` unchanged `"1.0"`; drift test in mod.rs fails the suite on any partial bump; 1.3.0 comment records the 11-06 live-truth fix (format-aware temp suffix) |

**Score:** 20/20 must-haves verified (7 truth rows above consolidating the 6 plans' must_have truth sets)

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/ignition-core/webdev/routes/.../cli/tagConfig/doPost.py` | exportTags format param + importTagsFile | ✓ VERIFIED | 288 lines; `payload_b64` export (:234), `importTagsFile` (:237) with File.createTempFile + try/finally delete + system.tag.importTags; collision lock |
| `crates/ignition-core/src/webdev/mod.rs` | Three-way ROUTE_BUNDLE_VERSION pin | ✓ VERIFIED | 261 lines; 1.3.0 + drift assertion + MIN_CLI pin |
| `crates/ignition-core/webdev/routes/VERSION` | 1.3.0 | ✓ VERIFIED | `1.3.0` |
| `crates/ignition-core/tests/tags_contract.rs` | Wiremock pins for new bodies | ✓ VERIFIED | 1795 lines; `payload_b64` export pin, `importTagsFile` body pins (file_b64/basePath/policy/format) |
| `crates/ignition-core/src/actions/tag_loss.rs` | scan_xml/scan_csv, ≥150 lines | ✓ VERIFIED | 591 lines; LossFact/XmlScan/CsvScan, top_level_names, CompoundProperty detection; validates against real fixture via `include_str!` (:410) |
| `crates/ignition-core/src/actions/mod.rs` | `pub mod tag_loss` | ✓ VERIFIED | :26 |
| `Cargo.toml` | quick-xml (no serialize) + csv + base64 w/ rationale | ✓ VERIFIED | Lines 33–50, rationale comments on all three |
| `crates/ignition-core/tests/fixtures/udt-multilevel.xml` | Real capture verbatim | ✓ VERIFIED | `diff -q` vs `artifacts/udt-multilevel.xml` → identical |
| `crates/ignition-core/src/actions/tags.rs` | ExportFormat, generate_legacy_csv, 300s timeout | ✓ VERIFIED | 4924 lines; ExportFormat (:1765), generate_legacy_csv (:2795), TAGS_EXPORT_TIMEOUT used on both export (:1842) and import (:2215), scan-fed pre-check (:2190) |
| `crates/ignition-cli/src/cli.rs` | `--format` on Export+Import | ✓ VERIFIED | TransferFormat on both arms (:610, :633) |
| `crates/ignition-cli/src/main.rs` | Loss gate pre-resolution dispatch | ✓ VERIFIED | 3088 lines; loss_gate fn + pre-resolution match arm, zero wire work on refusal |
| `crates/ignition-cli/src/render.rs` | Loss-report stderr; raw xml/csv stdout | ✓ VERIFIED | 1839 lines; TagsExport raw write (:116), loss_report prose in both json + human modes |
| `crates/ignition-cli/tests/contract_tags.rs` | Loss-gate, --yes, xml/csv stdout goldens | ✓ VERIFIED | 2701 lines; refusal (:2191), --yes flow (:2329), xml raw bytes (:2620) |
| `crates/ignition-cli/tests/e2e_webdev.rs` | Env-gated live gates | ✓ VERIFIED | 2634 lines; three 11-06 gates present, `IGNITION_LIVE_MUTATIONS` gating, self-deploy-first pattern |
| `README.md` | CSV honesty + loss-gate + exit-table sync | ✓ VERIFIED | 1372 lines; "CLI-GENERATED and LOSSY" + dedicated loss-gate section (:1115) |
| `.planning/.../11-LIVE-CAPTURES.md` | 5 probes + CSV coverage table | ✓ VERIFIED | All 5 probes with verbatim answers + coverage table + probe-5 corrected finding (dated note) |
| `.planning/.../artifacts/udt-multilevel.xml` | Real multi-level UDT capture | ✓ VERIFIED | UdtType/CompoundProperty present |
| `.planning/.../11-LIVE-GATE.md` | Both-rig PASS evidence | ✓ VERIFIED | Per-gate PASS tables × 2 rigs, sha256 evidence, 8 live-truth deltas w/ fixes, SC checklist |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| doPost.py importTagsFile | system.tag.importTags | gateway temp file, try/finally delete | ✓ WIRED | File-path signature per Probe 2; temp deleted on every path |
| webdev/mod.rs test | route ROUTE_VERSION + VERSION | three-way pin | ✓ WIRED | Drift assertion holds at 1.3.0; suite green |
| collisionPolicy | abort/overwrite lock | route refuses `'i'` | ✓ WIRED | doPost.py:261 |
| fixture udt-multilevel.xml | scan_xml tests | `include_str!` | ✓ WIRED | tag_loss.rs:410 |
| scan top_level_names | tags_import abort pre-check | scan feeds browse comparison | ✓ WIRED | tags.rs:2174–2190 |
| LossFact | loss-report rendering | structured facts → stderr + envelope | ✓ WIRED | main.rs:2860 + render.rs:132 |
| tags_export(Xml) | payload_b64 | base64 decode → raw; sha asserted | ✓ WIRED | tags.rs:1845–1856 |
| tags_import(Xml\|Csv) | importTagsFile | input → base64 → file_b64 | ✓ WIRED | tags.rs:2209 |
| loss-gate refusal | exit 2 invalid_input | existing error class | ✓ WIRED | main.rs + live gate evidence |
| render.rs TagsExport xml/csv | raw stdout bytes | sanctioned-exception pattern | ✓ WIRED | render.rs:116; stdout purity suite green |
| README CSV section | captures coverage table | documented lossy behavior | ✓ WIRED | contract_lint README-agreement tests green |
| live gate | `ign webdev deploy` | self-deploy before new-format calls | ✓ WIRED | e2e_webdev.rs deploy pattern |
| captures Probe 3 | fidelity oracle | byte-identity if deterministic, structural fallback | ✓ WIRED | LIVE-GATE.md applies the selected oracle |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| --- | --- | --- |
| TAGS-10 (XML bulk transfer, byte-faithful passthrough) | ✓ SATISFIED | SC-1 live evidence both rigs |
| TAGS-11 (CSV transfer, documented lossy) | ✓ SATISFIED | SC-2 live evidence both rigs |
| TAGS-12 (loss-report warning before import) | ✓ SATISFIED | SC-3 live evidence both rigs |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| (none) | — | — | — | Zero TODO/FIXME/placeholder/stub markers across all 6 phase-touched source files |

### Workspace Battery (re-run during this verification)

| Check | Result |
| --- | --- |
| `cargo test --workspace` | ✓ All suites 0 failed (incl. contract_tags 202+ goldens, tags_contract 390, stdout purity 5/5, contract_lint README agreement 4/4, tui_coverage 4/4; live gates correctly `#[ignore]`d without envs) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✓ exit 0 |
| `cargo fmt --check` | ✓ exit 0 |
| Commit integrity | ✓ All 14 commit hashes referenced in 11-LIVE-GATE.md resolve in git |

### Notes (non-blocking)

- **Plan-text staleness, not a code gap:** 11-02's must_haves literal says `1.2.0`; the bundle advanced to 1.3.0 during 11-06 (format-aware import temp suffix — a live-truth fix). The pin *discipline* (three-way equality, atomic bump) is what the plan actually required and holds at 1.3.0, recorded in the route comment and LIVE-GATE.md.
- **Round-trip is structurally identical, not byte-identical, after import.** This is the capture-selected oracle (Probe 3 disproved gateway determinism; sibling order permutes on gateway-side rebuild) sanctioned by the 11-06 plan key_link. Transport fidelity — the CLI's responsibility under the goal's "never CLI-side re-serialization" — is byte-exact (sha-equal file ⇄ `payload_b64` on both rigs). Not a gap.
- Live rigs were intentionally left up (keep-alive, 1.3.0 deployed, teardown of seeded paths verified) per LIVE-GATE.md; trial-window expiry may have since claimed them, which is expected ops lifecycle, not a phase gap.

### Human Verification Required

None blocking. All three success criteria closed on recorded live-gateway runs with sha256 evidence; everything else is covered by the green automated battery.

### Gaps Summary

None. All must-haves verified at the exists/substantive/wired levels: the route surface is real (temp-file + try/finally + locked collision matrix), the scan module is substantive (591 lines, validated on real captured bytes), the transfer path is byte-exact by construction (base64 is the only transformation), the loss gate refuses pre-resolution with zero wire work, and the three-way version pin holds at 1.3.0. All 8 live-truth deltas found during 11-06 were fixed in committed code (hashes verified) and re-proven live.

---

_Verified: 2026-09-14T01:30:00Z_
_Verifier: Claude (gsd-verifier)_
