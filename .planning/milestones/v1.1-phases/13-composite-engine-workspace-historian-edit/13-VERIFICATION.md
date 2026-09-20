---
phase: 13-composite-engine-workspace-historian-edit
verified: 2026-09-15T16:05:00Z
status: passed
score: 5/5 must-haves verified
human_verification:
  - test: "Run `ign workspace checkout <proj> ./tmp-ws --decode-scripts`, open a decoded .py sidecar in a real editor, `ign workspace status`, then `ign workspace push ./tmp-ws --yes` against a live licensed gateway"
    expected: "Tree checks out with editable sidecars; status shows the local edit as local_edit; push lands it gateway-side and a re-status reports clean"
    why_human: "Wiremock tests pin mechanics and bytes; the tactile real-gateway loop (real export/import latency, real descriptor landing) was not re-driven by this verifier"
  - test: "Run `VISUAL='code --wait' ign edit <proj> <resource> --yes` in a real terminal and save an edit"
    expected: "VS Code opens the file, blocks until close, the staged summary prints to stderr, the push lands; unchanged save exits clean with no push"
    why_human: "The five editor archetypes are proven with real spawned sh shims, but the UX of a real IDE editor holding the terminal is a human observation"
---

# Phase 13: Composite Engine — Workspace, Historian, Edit — Verification Report

**Phase Goal:** The generalized MemberSource diff engine turns local directories into a first-class authoring surface (workspace checkout), the historian gap closes honestly (spike-first), and `ign edit` delivers the kubectl-edit loop — hardening decode/encode at workspace scale before edit rides the same codec leg.
**Verified:** 2026-09-15T16:05:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth (SC) | Status | Evidence |
| --- | --- | --- | --- |
| 1 | SC-1: checkout → status drift → `--yes`-guarded push | ✓ VERIFIED | `actions/workspace.rs` (1248 ln): `workspace_checkout` (export→map→write→manifest), `workspace_status` three-way compare (clean/local_edit/gateway_drift/conflict + add/delete set-difference), `workspace_push` splices into FRESH export via `replace_member`/`remove_member`. CLI family in `cli.rs` (`Commands::Workspace` → Checkout/Status/Push, `Session::resolve` dispatch at main.rs:1177/1198/1218). `workspace_status_push.rs` 17/17 (matrix, conflict refusal, zero-write traffic pins); `contract_workspace.rs` 7/7 incl. the push-refusal golden whose message IS the blast radius. Conflict refusal is pre-gate and NOT `--yes`-able (workspace.rs:867–879, doc'd "never `--yes`-able, Pitfall W2"). |
| 2 | SC-2: injective hostile-name-safe mapping + proptests; `--decode-scripts` round-trip; tag values never in tree | ✓ VERIFIED | `client/workspace.rs` (423 ln): `MemberSource` (Zip\|Tree) + `build_mapping` + escape scheme. `tests/workspace_path_mapping.rs` **RAN GREEN: 21/21**, incl. properties p1 round-trip-exact, p2 injective-byte-wise, p3 refusals-name-the-member, p4 idempotent-safe-domain, p5 build_mapping total+pairwise-injective, p6 case-fold collision refusals naming both members; NFC/NFD distinct; NUL/traversal refusals. `--decode-scripts` rides `decode_export_tree` unchanged; `workspace_checkout.rs` 9/9 incl. byte-exact unedited re-encode and `tree_contains_exactly_the_zip_derived_set` (tag-exclusion structurally pinned — checkout ingests only the export zip). proptest is dev-dep only (`ignition-core/Cargo.toml:1` hit). |
| 3 | SC-3: tag↔historian bindings visible in `tags config` on licensed gateway | ✓ VERIFIED | `actions/tags.rs:1299` `history_summary` — pure fn over the config `Value` (passthrough posture; no typed structs; returns None when no history keys → absence renders byte-identically). `render.rs` imports + renders the additive block. Wiremock pin `tags_config_get_history_render_contract` (contract_tags.rs:672). **Live gate: both rigs PASSED** (13-LIVE-GATE.md, SC-3 ✅ on 8.3.6 + 8.3.3, verbatim renders recorded). |
| 4 | SC-4: historian binding resolved honestly per spike outcome | ✓ VERIFIED | **Closure branch — the legitimate done state.** 13-LIVE-CAPTURES.md: verdict line `closure — field set: [historyEnabled, historyProvider, sampleMode]`; write-path replay landed + read back on BOTH rigs; data probe proved written value 44 returned as history rows on both; OQ1 (`historicalProvider` typo) answered with rig evidence, not inference. 13-04 implemented exactly the captured field set (tags.rs unit test pins the captured shape verbatim) and corrected the harness find/delete routes (e2e_webdev.rs:904–936 now uses `find/{module}/{type}/{name}` + signature delete — the spike's silent-no-op finding fixed). 13-LIVE-GATE.md re-proved the recipe end-to-end both rigs (written 44 in history; SC-4 ✅). See Deviations for the Designer-branch note. |
| 5 | SC-5: `ign edit` kubectl-edit loop with no-op detection, fail-closed encode, staleness gate | ✓ VERIFIED | `actions/edit.rs` (474 ln): `Editor` trait + `TokioEditor` (arg-vector), `EditTempDir` (0700 + Drop), pipeline = fetch → snapshot (`member_hashes`, reused unchanged — no invented etag) → decode → baseline re-encode comparator → content-decided NoOp (no push/prompt) → fail-closed encode → staleness re-export + hash compare (refuses, NOT `--yes`-able — error returned from the pipeline before any gate) → staged diff summary. `edit_pipeline.rs` **RAN GREEN: 11/11** — all five archetypes as real spawned `sh` processes through the production `run_editor_argv` site (blocking, `--wait` advisory, daemon content-decided, no-change no-op, JSON-breaker fail-closed keeping the tree) + staleness refuse/allow. `contract_edit.rs` 8/8 binary-level: zero-stdout byte-scan under max diagnostics, guard golden (= staged summary), stale refusal, fail-closed exit-2 with kept path, happy-path push. Dispatch (`main.rs:434`) composes ONE `require_confirmation` gate (preview_then_confirm). |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `13-LIVE-CAPTURES.md` | Spike record + verdict (≥40 ln, "designer-diff") | ✓ VERIFIED | 92 ln; verdict line + field-set table + honest §Aborted Designer branch + replay/data evidence |
| `13-RIG-NOTES.md` | Rig ops log | ✓ VERIFIED | 79 ln; windows, tokens (names only), teardown |
| `artifacts/designer-diff-836.txt` | Designer diff (8.3.6) | ⚠️ SUPERSEDED | Does not exist — **by recorded user decision** (see Deviations). Replacement evidence committed: `config-before/after-{836,833}.json` + `replay-diff-{836,833}.txt` all present |
| `13-LIVE-GATE.md` | Both-rig gate record (≥30 ln) | ✓ VERIFIED | 71 ln; both rigs passed on commit `65456a9`, incidents + hardening trail, clean teardown |
| `client/workspace.rs` | MemberSource + mapping | ✓ VERIFIED | 423 ln, substantive, wired (used by actions/workspace.rs:38,701) |
| `tests/workspace_path_mapping.rs` | proptest suite (≥60 ln) | ✓ VERIFIED | 684 ln; **21/21 green** |
| `Cargo.toml` (core) | proptest dev-dep | ✓ VERIFIED | present, dev-only |
| `actions/workspace.rs` | manifest + checkout + status + push | ✓ VERIFIED | 1248 ln; manifest recorded not re-derived; conflicts pre-gate |
| `tests/workspace_checkout.rs` | round-trip/exclusion/refusal (≥60 ln) | ✓ VERIFIED | 562 ln; **9/9 green** |
| `tests/workspace_status_push.rs` | matrix + guard proofs (≥80 ln) | ✓ VERIFIED | 987 ln; **17/17 green** |
| `actions/tags.rs` | history_summary | ✓ VERIFIED | 5061 ln; fn at :1299 + captured-shape unit tests |
| `render.rs` | additive history block | ✓ VERIFIED | imports `history_summary`, renders block; workspace render arms also present |
| `tests/e2e_webdev.rs` | live gate test + corrected harness | ✓ VERIFIED | `live_tags_history_bindings` `#[ignore]`/LIVE_GATE; `delete_internal_historian` corrected to find+signature chain |
| `actions/edit.rs` | Editor seam + pipeline | ✓ VERIFIED | 474 ln; `TokioEditor` ×11; staleness via `member_hashes` |
| `tests/edit_pipeline.rs` | five-archetype harness (≥80 ln) | ✓ VERIFIED | 625 ln; **11/11 green** |
| `cli.rs` | Workspace + Edit commands | ✓ VERIFIED | `Commands::Workspace` (:133) + `Commands::Edit` (:142) + `EditArgs` (:493) with flags per plan |
| `routes.rs` (tui) | `edit` OutOfBand row | ✓ VERIFIED | CliRoute `path: "edit", mapping: OutOfBand` (:85–88) |
| `tests/tui_coverage.rs` | pinned set + workspace rows | ✓ VERIFIED | OutOfBand pinned to `["api call", "completions", "edit"]` set-equality; `workspace_rows_cover_the_family` = exactly 3 leaves; **5/5 green** |
| `tests/contract_workspace.rs` | goldens (≥60 ln) | ✓ VERIFIED | 833 ln; **7/7 green** |
| `tests/contract_edit.rs` | binary contracts (≥80 ln) | ✓ VERIFIED | 797 ln; **8/8 green** |
| `contract_stdout_purity.rs` | workspace verbs covered | ✓ VERIFIED | workspace status/push/checkout refusal tests present (:214–283) |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| 13-01 capture artifacts | 13-04 field set | captured names only | ✓ WIRED | Field-set table `[historyEnabled, historyProvider, sampleMode]` → tags.rs recipe + unit test verbatim; no guessed keys (group key explicitly absent from every capture) |
| Spike verdict | SC-4 done-state | record → branch | ✓ WIRED | closure verdict → 13-04 closure branch + live gate re-proof |
| `build_mapping` | checkout tree writer | recorded manifest | ✓ WIRED | checkout records pairs; push/status consume manifest (`ign-workspace` ×6) |
| `MemberSource` | status compare | one implementation | ✓ WIRED | workspace.rs:38 imports; :701 `Zip.member_hashes`, Tree side via manifest mapping |
| checkout/push | `decode_export_tree` / `replace_member` | codec + landing rules unchanged | ✓ WIRED | both referenced and executed in action code + pinned by tests |
| `edit_pipeline` | codec leg | unchanged | ✓ WIRED | `encode_export_tree` ×4, `decode_export_tree` in pipeline steps 3/4/7 |
| staleness gate | `member_hashes` | reuse, no etag | ✓ WIRED | edit.rs:265,349–353 — snapshot vs fresh hash compare |
| diff summary | CLI gate | preview_then_confirm | ✓ WIRED | main.rs `dispatch_edit`: summary composed once, IS the refusal, ONE gate |
| edit clap + routes row | pinned set | atomic landing | ✓ WIRED | routes.rs:86 `edit` OutOfBand + tui_coverage set `["api call","completions","edit"]` green |
| stdout purity harness | workspace + edit | byte-scan over real binary | ✓ WIRED | purity tests for 3 workspace verbs + edit zero-stdout pin (contract_edit.rs:266) |
| Session seam | dispatch | no second construction | ✓ WIRED | main.rs `Session::resolve` for workspace family; `dispatch_edit` resolves via seam |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
| --- | --- | --- |
| IDE-04 (workspace checkout/status/push) | ✓ SATISFIED | — |
| TAGS-13 (bindings visible) | ✓ SATISFIED | wiremock-pinned + live-proven both rigs |
| TAGS-14 (binding create/update, spike-gated) | ✓ SATISFIED | closure branch: recipe pinned + live-proven (written values returned from history on both rigs) |
| IDE-01 (`ign edit` loop) | ✓ SATISFIED | all three guard clauses live at binary level |

(REQUIREMENTS.md checkboxes remain unchecked — bookkeeping for the orchestrator; substance verified above.)

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| — | — | none found | — | TODO/FIXME/placeholder/unimplemented/`return null`-style scans across all phase files came back clean; no console-only handlers, no empty implementations |

### Deviations (judged, not gaps)

1. **`designer-diff-836.txt` absent — by recorded user decision, honestly.** The Designer binding step was declined on 2026-09-15 ("we can assume it works"); the spike record states plainly that no designer-diff artifacts exist or were committed (13-LIVE-CAPTURES.md §Aborted Designer branch) and substitutes the live write-path replay as oracle, with replacement evidence committed (before/after JSON + replay diffs, machine-diffed). The roadmap's SC-4 explicitly sanctions "spike outcome recorded either way," the outcome (closure) is one of the two legitimate done states, and the closure recipe was re-proven end-to-end at the 13-04 live gate on both rigs. Judged: legitimate, consistent with SC-4's contract.
2. **Plan-13-05's artifact marker `contains: "Command"` not literally matched** in `edit_pipeline.rs` — the tests spawn real processes through the production `run_editor_argv` site instead of constructing `Command` directly. Substance (real spawned processes, five archetypes) fully satisfied; the marker was looser than the requirement.

### Test Evidence (run by this verifier)

`cargo test` green: `workspace_path_mapping` 21/21 · `edit_pipeline` 11/11 · `workspace_checkout` 9/9 · `workspace_status_push` 17/17 · `contract_edit` 8/8 · `contract_workspace` 7/7 · `tui_coverage` 5/5 — **78/78, zero failures**. (Live gates were pre-run on licensed trial rigs per 13-LIVE-GATE.md; `#[ignore]` env-gated tests not re-run by this verifier.)

### Human Verification Required

1. **Real-gateway workspace loop**
   **Test:** `ign workspace checkout <proj> ./ws --decode-scripts` → edit a `.py` sidecar in a real editor → `ign workspace status` → `ign workspace push ./ws --yes` on a licensed rig.
   **Expected:** sidecars nvim-editable; status shows `local_edit`; push lands and re-status is clean.
   **Why human:** wiremock pins bytes/traffic; real export/import latency and descriptor landing on live infra not re-driven here.
2. **Real-IDE `ign edit` session**
   **Test:** `VISUAL='code --wait' ign edit <proj> <resource> --yes`; save a change; then run again and save nothing.
   **Expected:** editor blocks the loop, summary on stderr, push lands; unchanged save = clean no-op exit 0 with zero imports.
   **Why human:** archetype shims prove spawn mechanics; the tactile real-IDE experience is human-observable.

### Gaps Summary

None blocking. All five success criteria verified against the codebase at all three levels (exists / substantive / wired), with 78/78 tests executed green by this verifier and both live-gate evidence artifacts recorded in-phase. Two noted deviations are honest, documented, and within the roadmap's sanctioned outcome space.

---

_Verified: 2026-09-15T16:05:00Z_
_Verifier: Claude (gsd-verifier)_
