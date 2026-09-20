---
phase: 11-tag-bulk-transfer-xml-csv
verified: 2026-09-14T14:52:07Z
status: passed
score: 13/13 must-have truths verified (7/7 phase-goal truths hold, 6/6 gap-closure truths closed); 1 minor warning-level gap — workspace fmt gate regressed — CLOSED inline by orchestrator 2026-09-14 (cargo fmt applied, gate re-run green, 391/391 core lib tests pass)
is_re_verification: true
re_verification:
  previous_status: passed
  previous_score: 20/20
  gaps_closed:
    - "UAT test 3: loss-gate refusal carries the --yes re-run hint on human stderr AND the JSON envelope error.hint — never the generic file-read hint (11-07)"
    - "UAT test 4: xml_udt_type_definition fact states the live scoped truth (provider-root imports land definitions in _types_; refusal is folder-basePath-only), mirrored in README, STATE.md, and 11-LIVE-CAPTURES.md (11-08)"
    - "fmt gate regression: cargo fmt applied to error.rs and committed by orchestrator 2026-09-14 — cargo fmt --check exit 0 re-confirmed, ignition-core lib 391/391 green"
  gaps_remaining:
    - "cargo fmt --check fails on crates/ignition-core/src/error.rs:977 — the ONLY diff in the workspace, introduced by 11-07's own commit 459307d (sentinel added to the test-module use list without the rustfmt-required line collapse). Cosmetic, zero behavioral impact; 1-line mechanical fix"
  regressions:
    - "cargo fmt --check: was ✓ exit 0 at the initial verification; now exit 1 on error.rs:977 (see gap above)"
gaps:
  - truth: "Workspace hygiene gates stay green as they were at the initial verification (fmt --check exit 0)"
    status: resolved
    resolution: "Orchestrator applied cargo fmt to error.rs and committed 2026-09-14; gate re-run exit 0, 391/391 ignition-core lib tests pass"
    reason: "cargo fmt --check exits 1 on error.rs:977 — the only formatting diff in the entire workspace, introduced by the gap-closure work itself (commit 459307d appended LOSS_GATE_REFUSAL_REASON_PREFIX to the test-module use list; rustfmt wants truncate_api_body collapsed onto the same line). The 11-07 plan's fmt note excused pre-existing Phase-10 drift, but this diff is 11-07's own edit, not pre-existing drift. Tests and clippy -D warnings are clean; impact is purely cosmetic."
    artifacts:
      - path: "crates/ignition-core/src/error.rs"
        issue: "test-module use statement (:977-980) not rustfmt-clean after 11-07's import addition"
    missing:
      - "Run `cargo fmt` (or hand-collapse `truncate_api_body,` onto the LOSS_GATE_REFUSAL_REASON_PREFIX line at error.rs:977-979) and commit; nothing else changes"
---

# Phase 11: Tag Bulk Transfer XML/CSV Verification Report (Re-verification)

**Phase Goal:** Tags move in the formats the ecosystem already speaks — byte-faithful passthrough of what the gateway produces (never CLI-side re-serialization) — with honest warnings about what lossy formats would drop before import commits.
**Verified:** 2026-09-14T14:52:07Z
**Status:** gaps_found (one minor, mechanical, warning-level gap; both UAT gaps genuinely CLOSED; all phase-goal truths still hold)
**Re-verification:** Yes — after UAT gap closure (11-07 hint override + 11-08 UDT fact correction)

## Goal Achievement

### UAT Gap 3 — Loss-Gate Hint Override (11-07): CLOSED ✓

| Check | Status | Evidence |
| --- | --- | --- |
| `LOSS_GATE_REFUSAL_REASON_PREFIX` const in error.rs | ✓ VERIFIED | error.rs:42, doc comment in the TTY-const style explaining content-addressing and the drift-guard role |
| `starts_with` branch in hint()'s InvalidInput arm | ✓ VERIFIED | error.rs:668-676 — between the TTY check and the generic default; returns `--yes` re-run hint, generic file-read default stays final fallback |
| Throw-site coupling comment, construction untouched | ✓ VERIFIED | main.rs:2862-2865; the InvalidInput throw itself unchanged (prose-reason format, `re-run with --yes` suffix) |
| Sentinel prefix unique among production reasons | ✓ VERIFIED | Only runtime producers of the prefix: the const (error.rs:42) and render_loss_prose's header (main.rs:2896). Note: a plain-text grep shows a 3rd textual hit at main.rs:2862 — the mandated coupling COMMENT quoting the header; documentation, not a production literal. The plan's "exactly 2 hits" count is imprecise vs its own Task 2, but the gate's intent (no other runtime reason carries the prefix) holds |
| Unit pins (override + generic regression + TTY precedent) | ✓ VERIFIED | `loss_gate_hint_override_and_neighbors_unchanged` PASSES — asserts slug `invalid_input` unchanged, exit 2 unchanged, `--yes` hint on sentinel path, `!contains("fix the input source")`, generic hint keeps `--file`/`stdin`, TTY hint keeps `interactive terminal`, no cross-leak in either direction |
| Contract drift-guards (human + JSON) | ✓ VERIFIED | contract_tags.rs:2237-2249 (`hint: the loss report above names` present on stderr, `fix the input source` absent) and :2277-2287 (`error.hint` carries corrected guidance, file-read hint absent); comments cite the debug session; inside `tags_import_loss_gate_refusals` (:2191) which PASSES |
| Generic-hint pins untouched and green | ✓ VERIFIED | contract_tags.rs:2032 + contract_projects.rs:1196/1333/1725 all still pin the generic file-read hint byte-identically; all suites green |
| `fix the input source` ledger | ✓ VERIFIED | 8 hits exactly as the SUMMARY documented: 5 pre-existing (error.rs:676 default + the 4 untouched pins) + 2 Task-2 negative assertions + 1 Task-1 negative assertion (error.rs:1745 — the plan's own action spec required it; plan's count of 7 was arithmetically off, deviation honestly recorded) |
| Frozen contract (slug/exit/envelope/prose) | ✓ VERIFIED | Unit test pins code() == "invalid_input" and exit_code() == 2 on the sentinel path; no golden edits committed; message-prose pins (contract_tags.rs:2226/2292, e2e_webdev.rs:1976/2611, contract_stdout_purity.rs:181) untouched and green |

### UAT Gap 4 — UDT Fact Scope Correction (11-08): CLOSED ✓

| Check | Status | Evidence |
| --- | --- | --- |
| Fact detail states live truth, all three required elements | ✓ VERIFIED | tag_loss.rs:178-189 — (a) "DOES land them: provider-root imports route each definition to [provider]_types_/Name"; (b) refusal is "a FOLDER-basePath behavior (11-01 capture) this command cannot hit" (verbatim quote retained, scoped); (c) udtParentType provider-qualified cross-provider + parameter overrides drop on unresolvable target (8.3.3; 8.3.6 preserves) |
| Detection unchanged — fact still fires for `type="UdtType"` | ✓ VERIFIED | Firing condition untouched; `has_fact(XML_UDT_TYPE_DEFINITION)` pin (tag_loss.rs:454) passes unmodified; all scan_xml tests green |
| Const/doc-comment/test-comment corrected | ✓ VERIFIED | codes doc (tag_loss.rs:27-33) scoped; test comment (tag_loss.rs:443-447) reworded, assertion untouched; `codes::XML_UDT_TYPE_DEFINITION` name byte-identical everywhere |
| Grep gates exact | ✓ VERIFIED | `lands nothing\|refuses outright\|Udt definitions can only` in crates/ → exactly the expected 5: tags_contract.rs:1768/1769 (unrelated CSV capture asserts, preserved), tag_loss.rs:29+182 (verbatim quote inside the new scoped text), eam.rs:51 (pre-existing truthful EAM comment, preserved) |
| README loss-gate section scoped | ✓ VERIFIED | README:1117-1132 carries the conditioned truth; `gateway refuses outright` = 0 hits; README:966 `fleet-destructive trio refuses outright` survives untouched (unrelated, true) — exactly 1 hit |
| STATE.md dated corrections, originals preserved | ✓ VERIFIED | :136 and :145 append `**[CORRECTED 2026-09-14, 11-08]**` with the folder-basePath scoping + debug-session citation; original decision text intact (line numbers drifted 134/143→136/145 — tilde-approximation in plan, same decision lines) |
| Captures-doc scope notes, Probe-5 precedent format | ✓ VERIFIED | :155 `**[SCOPE-CORRECTED live 2026-09-14 …]**` after Probe 3 finding (b)(1) with the ×3 live-proof detail; :163 `**[SCOPE NOTE 2026-09-14, 11-08]**` on oracle item 4, original text preserved |
| e2e_webdev.rs stale comment (auto-fix, plan-directed) | ✓ VERIFIED | :2548-2551 comment reworded to scoped truth; the actual pins (:2619 `[xml_udt_type_definition]`, :2632) use the unchanged code name and pass |
| Key link (code ↔ docs ↔ planning record) | ✓ VERIFIED | `folder-basePath` appears in tag_loss.rs (doc comment + detail), README, STATE.md markers, captures doc, and .planning/debug/udt-type-fact-falsified.md is cited in all three mirrors |

### Observable Truths (phase goal — regression check after gap closure)

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| SC-1 | XML bulk download+upload, verbatim gateway-byte passthrough | ✓ HOLDS | Gap-closure work touched only error hints + fact/doc text — zero changes to the transfer path (tags.rs/transport untouched by 11-07/11-08 file lists; confirmed via commit stats) |
| SC-2 | CSV bulk download+upload, server-byte-faithful, documented lossy | ✓ HOLDS | Same — CSV path untouched; README CSV honesty section intact |
| SC-3 | Loss-report warning before import; user can abort | ✓ HOLDS | loss_gate pre-resolution dispatch intact (main.rs:1399 — refusal returns before wire work); tags_import_loss_gate_refusals green; --yes flow unchanged |
| Lock | XML = base64-only transformation, both directions | ✓ HOLDS | doPost.py:234/237 + 288 lines intact; collision lock (:260) intact |
| Lock | Refusal = exit 2 invalid_input, no new slugs | ✓ HOLDS | 11-07 explicitly preserved the taxonomy; unit test pins it on the sentinel path |
| Lock | quick-xml/csv/base64 stack + version pin 1.3.0 | ✓ HOLDS | ROUTE_BUNDLE_VERSION (mod.rs:25) = routes/VERSION = route constants = 1.3.0; drift assertion in place |
| Workspace battery | all green except fmt | ✓/⚠️ | `cargo test --workspace`: 0 failures across all suites (incl. tags_contract 391, contract_tags 202, scan pins, drift pins); `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` FAILS on error.rs:977 — see gap |

**Score:** 13/13 must-have truths verified (7/7 phase-goal truths hold; 3/3 11-07 truths + 3/3 11-08 truths closed); 1 warning-level hygiene gap (fmt) — not a truth failure, goal unaffected

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| --- | --- | --- |
| TAGS-10 (XML bulk transfer, byte-faithful passthrough) | ✓ SATISFIED | unchanged since initial verification; live both-rig evidence stands |
| TAGS-11 (CSV transfer, documented lossy) | ✓ SATISFIED | unchanged |
| TAGS-12 (loss-report warning before import) | ✓ SATISFIED | strengthened: the refusal's hint now matches the failure, and the UDT fact reads as live truth |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| crates/ignition-core/src/error.rs | 977-979 | rustfmt violation in test-module use list (introduced by 11-07) | ⚠️ Warning | Regresses the previously-green `cargo fmt --check` gate; cosmetic, zero behavioral impact; 1-line fix |
| README.md | 1278 | word "placeholder" | ℹ️ Info | Legitimate prose about /dev/urandom template substitution — not a stub |

No TODO/FIXME/XXX/HACK/stub markers in any gap-closure-touched file.

### Commits Verified

| Plan | Commit | Content |
| --- | --- | --- |
| 11-07 T1 | `459307d` | sentinel const + hint branch (+ the fmt drift) |
| 11-07 T2 | `cbfc022` | throw-site comment + drift-guard pins |
| 11-08 T1 | `8df05fb` | fact reword (+ e2e comment auto-fix) |
| 11-08 T2 | `d779d7c` | README/STATE/captures mirrors |
| docs | `29d6735`, `4ded9dd` | both summaries |

All resolve in `git log`.

### Human Verification Required

None blocking. Both UAT gaps were closed on text/hint changes fully covered by automated pins; the underlying live behavior (gate refuses with exit 2, `--yes` lands the import, definitions route to `_types_`) was already live-proven in 11-06/the UAT run and was not modified. Optional spot-check for a human, not a gate: run `ign tags import --format xml` on a UdtType-bearing fixture against a live gateway and confirm (a) stderr hint reads "the loss report above names…", (b) the fact text names `_types_` routing.

### Gaps Summary

Both UAT gaps are **genuinely closed**, verified at all three levels against the actual code — not just the summaries' claims:

1. **UAT test 3 (hint mismatch):** the sentinel + branch + throw-site coupling + three unit pins + two contract drift-guards all exist and pass; the generic and TTY hints are regression-pinned byte-identically; the frozen taxonomy (slug/exit/envelope/prose) is explicitly asserted unchanged. The only ledger discrepancy (8 vs 7 `fix the input source` hits) was honestly self-reported by the SUMMARY and is explained by the plan's own Task-1 action spec.
2. **UAT test 4 (falsified UDT fact):** the fact now states the conditioned live truth with all three required elements, detection and the code name are byte-identical, and the correction is mirrored with dated provenance in README, STATE.md (both decision lines), and the captures doc (both locations), in the established Probe-5 precedent format. Grep gates return exactly the expected hit sets.

One minor gap remains: **`cargo fmt --check` fails on error.rs:977** — the only formatting diff in the workspace, introduced by 11-07's own commit (the sentinel was appended to the test-module use list without the line collapse rustfmt requires). The initial verification recorded fmt green, so this is a (cosmetic) regression of a hygiene gate, not a goal gap: tests and clippy are clean, no pin is affected, and the fix is a one-liner (`cargo fmt`). Recommended: apply before phase wrap-up / the next battery run.

---

_Verified: 2026-09-14T14:52:07Z_
_Verifier: Claude (gsd-verifier)_
